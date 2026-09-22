//! The Windows process collector and the `windows.processes` provider.
//!
//! ```text
//! windows.processes                3 machine-wide metrics, in the catalog
//!   └── WindowsProcessCollector    several hundred rows, outside the catalog
//!         ├── CreateToolhelp32Snapshot   the inventory: PID, parent, threads, name
//!         ├── OpenProcess                a short-lived, minimally privileged handle
//!         ├── GetProcessTimes            CPU time, and the creation-time identity
//!         ├── K32GetProcessMemoryInfo    the working set
//!         ├── GetProcessIoCounters       storage bytes read and written
//!         └── QueryFullProcessImageNameW the executable, for grouping
//! ```
//!
//! # No `tasklist`, no `wmic`, no PowerShell
//!
//! Each of those spawns a process — PowerShell spawns a *runtime* — to format
//! for a console what these calls return as numbers. On a machine with four
//! hundred processes that is a per-refresh cost measured in hundreds of
//! milliseconds, plus a text format to parse back, plus a dependency on a
//! shell PULSE has no business requiring.
//!
//! # Two tiers, because protected processes exist
//!
//! `CreateToolhelp32Snapshot` needs no special rights and yields, for every
//! process on the machine: PID, parent PID, thread count and image name. Those
//! four facts are therefore **always** available.
//!
//! Everything else needs a handle, and `OpenProcess` legitimately fails for
//! protected processes — anti-malware services, `csrss`, parts of the kernel's
//! own process — even for an administrator. PULSE asks for
//! `PROCESS_QUERY_LIMITED_INFORMATION`, the narrowest right that answers these
//! questions, never elevates, and when it is refused keeps the inventory row
//! with `permissionDenied` on the columns that needed the handle. The process
//! is running; hiding it would understate the machine.
//!
//! # Handles do not outlive the call that opened them
//!
//! Every handle is wrapped in a guard that closes it on drop, including on an
//! early return. No handle is kept between refreshes: the rate baselines hold
//! an identity and two integers, never an OS resource. Keeping four hundred
//! handles open across refreshes would pin every one of those processes'
//! kernel objects in memory for as long as PULSE ran.
//!
//! # Not physically executed
//!
//! At the time of writing this code compiles for `x86_64-pc-windows-msvc` and
//! its pure logic is unit-tested on Fedora, but it has **not** been run on a
//! Windows machine. `docs/platforms/windows.md` says so too.

pub mod classify;
pub mod times;

use std::sync::Arc;

use crate::metrics::model::{
    Availability, MetricDefinition, MetricError, MetricRef, MetricSample, MetricValue, ProviderId,
};
use crate::metrics::providers::MetricProvider;
use crate::metrics::wellknown::process as wellknown;
use crate::processes::{ProcessCollector, ProcessCounts, RawProcessScan};

/// Identifier of the Windows process provider.
pub const PROVIDER_ID: &str = "windows.processes";

/// How much of each process to read.
///
/// The provider needs three numbers, and the Toolhelp snapshot alone carries
/// all three. Opening a handle per process to publish them would be several
/// hundred `OpenProcess` calls for nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// The Toolhelp inventory only.
    Counts,
    /// Everything a process row shows.
    Full,
}

/// Why Windows reports no per-process state.
///
/// Windows schedules *threads*; a process is a container that has no state of
/// its own. `NtQuerySystemInformation` can be asked for every thread's state
/// and a process could be called "running" when any of its threads is, but
/// that is PULSE's invention rather than the operating system's answer — and
/// it would mean the same column meant two different things on the two
/// platforms. It is reported as unavailable instead.
pub const STATE_UNSUPPORTED: &str =
    "Windows schedules threads rather than processes, so it reports no process-level state";

/// Reads Windows' process table.
#[derive(Debug, Default)]
pub struct WindowsProcessCollector;

impl WindowsProcessCollector {
    pub fn new() -> Self {
        Self
    }
}

impl ProcessCollector for WindowsProcessCollector {
    fn collect(&self) -> RawProcessScan {
        scan(Depth::Full)
    }
}

/// Builds the collector Windows' snapshot service uses.
pub fn collector() -> Arc<dyn ProcessCollector> {
    Arc::new(WindowsProcessCollector::new())
}

/// Walks the process table at the requested depth.
pub fn scan(depth: Depth) -> RawProcessScan {
    imp::scan(depth)
}

// --- the machine-wide provider ---------------------------------------------

/// Publishes the three machine-wide process metrics on Windows.
///
/// `process.count.running` is deliberately **unavailable** here rather than
/// fabricated: see [`STATE_UNSUPPORTED`]. The other two are real.
#[derive(Debug)]
pub struct WindowsProcessProvider {
    id: ProviderId,
}

impl WindowsProcessProvider {
    pub fn new() -> Self {
        Self {
            id: ProviderId::new(PROVIDER_ID).expect("provider id must be valid"),
        }
    }
}

impl Default for WindowsProcessProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for WindowsProcessProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError> {
        let mut definitions = wellknown::definitions(&self.id, Availability::Available);

        // The one metric this platform genuinely cannot produce. Declared
        // anyway, with the reason, so the interface can explain the gap rather
        // than silently omit a metric Fedora has.
        for definition in &mut definitions {
            if definition.metric.key.as_str() == wellknown::COUNT_RUNNING {
                definition.availability = Availability::unsupported(STATE_UNSUPPORTED);
            }
        }

        Ok(definitions)
    }

    fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError> {
        let counts = crate::processes::counts(&scan(Depth::Counts));

        Ok(requested
            .iter()
            .map(|reference| sample_for(reference, counts))
            .collect())
    }
}

/// Resolves one requested reference against a set of counts.
fn sample_for(reference: &MetricRef, counts: ProcessCounts) -> MetricSample {
    if reference.key.as_str() == wellknown::COUNT_RUNNING {
        return MetricSample::unavailable(
            reference.clone(),
            Availability::unsupported(STATE_UNSUPPORTED),
        );
    }

    let value = match reference.key.as_str() {
        wellknown::COUNT_TOTAL => Some(f64::from(counts.total)),
        wellknown::THREAD_COUNT_TOTAL => Some(f64::from(counts.threads)),
        _ => None,
    };

    match value.and_then(MetricValue::number) {
        Some(value) => MetricSample::available(reference.clone(), value),
        None => MetricSample::unavailable(
            reference.clone(),
            Availability::not_registered(format!(
                "'{}' is not a metric the Windows process provider owns",
                reference.key
            )),
        ),
    }
}

/// Builds the Windows process provider.
pub fn provider() -> Arc<dyn MetricProvider> {
    Arc::new(WindowsProcessProvider::new())
}

// --- the Windows API ------------------------------------------------------

#[cfg(target_os = "windows")]
mod imp {
    use windows_sys::Win32::Foundation::{
        CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE, MAX_PATH,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::SystemInformation::{
        GetSystemWindowsDirectoryW, GlobalMemoryStatusEx, MEMORYSTATUSEX,
    };
    use windows_sys::Win32::System::Threading::{
        GetActiveProcessorCount, GetProcessIoCounters, GetProcessTimes, OpenProcess,
        QueryFullProcessImageNameW, ALL_PROCESSOR_GROUPS, IO_COUNTERS,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    use crate::metrics::model::Availability;
    use crate::processes::{
        ProcessInstanceId, ProcessState, RawProcess, RawProcessIo, RawProcessScan,
    };

    use super::{classify, times, Depth, STATE_UNSUPPORTED};

    /// An owned Windows `HANDLE` that is always closed.
    ///
    /// The whole reason handles are wrapped rather than closed by hand: a
    /// process walk has a dozen early returns, and one forgotten
    /// `CloseHandle` on a path taken by a few protected processes leaks a
    /// kernel object per refresh, forever.
    struct OwnedHandle(HANDLE);

    impl OwnedHandle {
        /// Wraps a handle, rejecting the two values that are not handles.
        ///
        /// `OpenProcess` returns null on failure, `CreateToolhelp32Snapshot`
        /// returns `INVALID_HANDLE_VALUE`. Closing either would be a bug.
        fn new(handle: HANDLE) -> Option<Self> {
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                None
            } else {
                Some(Self(handle))
            }
        }

        fn raw(&self) -> HANDLE {
            self.0
        }
    }

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            // SAFETY: `self.0` was checked to be a real handle at
            // construction and this is the only owner, so it is closed exactly
            // once.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    /// Reads a NUL-terminated UTF-16 buffer into a string.
    fn utf16_to_string(buffer: &[u16]) -> String {
        let end = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(buffer.len());

        String::from_utf16_lossy(&buffer[..end])
    }

    /// How many logical processors the CPU percentages are normalised against.
    ///
    /// `GetActiveProcessorCount(ALL_PROCESSOR_GROUPS)` rather than
    /// `GetSystemInfo`: the latter reports only the calling thread's processor
    /// group and would say 64 on a 128-thread workstation, doubling every
    /// process's CPU percentage.
    fn logical_processor_count() -> Option<u32> {
        // SAFETY: a pure query taking one integer and touching no memory.
        let count = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) };
        (count > 0).then_some(count)
    }

    /// Installed physical memory, for the memory-percentage denominator.
    fn physical_memory_total() -> Option<u64> {
        let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;

        // SAFETY: `status` is live, zeroed and carries its own size in
        // `dwLength`, which is how the API decides what to write. The return
        // value is checked before any field is read.
        let ok = unsafe { GlobalMemoryStatusEx(&mut status) };

        (ok != 0).then_some(status.ullTotalPhys)
    }

    /// The Windows directory, for classifying processes.
    fn system_root() -> Option<String> {
        let mut buffer = [0_u16; MAX_PATH as usize + 1];

        // SAFETY: the buffer is live and its length is passed correctly. The
        // call writes at most that many units and returns how many it wrote.
        let written =
            unsafe { GetSystemWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };

        if written == 0 || written as usize >= buffer.len() {
            return None;
        }

        Some(utf16_to_string(&buffer[..written as usize]))
    }

    /// Walks the process table once.
    pub fn scan(depth: Depth) -> RawProcessScan {
        // SAFETY: a documented call taking two integers. The returned handle
        // is validated by `OwnedHandle::new`, which rejects
        // `INVALID_HANDLE_VALUE`.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };

        let Some(snapshot) = OwnedHandle::new(snapshot) else {
            // No inventory at all. An empty scan, never a panic.
            return RawProcessScan::default();
        };

        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        // SAFETY: `snapshot` is a live snapshot handle and `entry` is a live,
        // correctly sized `PROCESSENTRY32W` whose `dwSize` the API requires.
        if unsafe { Process32FirstW(snapshot.raw(), &mut entry) } == 0 {
            return RawProcessScan::default();
        }

        let root = (depth == Depth::Full).then(system_root).flatten();
        let mut processes = Vec::with_capacity(512);

        loop {
            processes.push(read_process(&entry, depth, root.as_deref()));

            // SAFETY: as above; the iteration ends when the call returns zero.
            if unsafe { Process32NextW(snapshot.raw(), &mut entry) } == 0 {
                break;
            }
        }

        RawProcessScan {
            processes,
            physical_memory_total: (depth == Depth::Full).then(physical_memory_total).flatten(),
            logical_processor_count: logical_processor_count(),
        }
    }

    /// Builds one row from the inventory, then enriches it if a handle opens.
    fn read_process(entry: &PROCESSENTRY32W, depth: Depth, root: Option<&str>) -> RawProcess {
        let pid = entry.th32ProcessID;
        let name = utf16_to_string(&entry.szExeFile);

        // The inventory tier: always available, no handle needed.
        let mut process = RawProcess {
            // Replaced below once the creation time is known. A process whose
            // handle is refused keeps a token of zero, which is stable for as
            // long as that PID is that process — the honest best available,
            // and documented as such.
            instance: ProcessInstanceId::new(pid, 0),
            parent_pid: Some(entry.th32ParentProcessID),
            name: name.clone(),
            executable_path: None,
            executable_availability: Availability::not_detected("not read"),
            cpu_time_nanos: None,
            cpu_availability: Availability::not_detected("not read"),
            resident_memory_bytes: None,
            memory_availability: Availability::not_detected("not read"),
            thread_count: Some(entry.cntThreads),
            thread_availability: Availability::Available,
            io: None,
            io_availability: Availability::not_detected("not read"),
            state: ProcessState::Other,
            state_availability: Availability::unsupported(STATE_UNSUPPORTED),
            class: classify::classify(pid, None, root),
        };

        if depth == Depth::Counts {
            return process;
        }

        // SAFETY: a documented call taking three integers. The handle is
        // validated and closed by `OwnedHandle`.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };

        let Some(handle) = OwnedHandle::new(handle) else {
            // A protected process. Everything needing a handle is refused,
            // and the row survives on its inventory.
            let refused = Availability::permission_denied(format!(
                "Windows refused a query handle for '{name}'; it is a protected process and \
                 PULSE does not elevate"
            ));
            process.cpu_availability = refused.clone();
            process.memory_availability = refused.clone();
            process.io_availability = refused.clone();
            process.executable_availability = refused;
            return process;
        };

        read_times(&handle, &mut process);
        read_memory(&handle, &mut process);
        read_io(&handle, &mut process);
        read_image_name(&handle, &mut process, root);

        // The handle is closed here, by `Drop`, before the next process is
        // looked at. Nothing is retained.
        process
    }

    /// CPU time, and the creation stamp the identity is built from.
    fn read_times(handle: &OwnedHandle, process: &mut RawProcess) {
        let mut creation: FILETIME = unsafe { std::mem::zeroed() };
        let mut exit: FILETIME = unsafe { std::mem::zeroed() };
        let mut kernel: FILETIME = unsafe { std::mem::zeroed() };
        let mut user: FILETIME = unsafe { std::mem::zeroed() };

        // SAFETY: four live, correctly typed `FILETIME` locals; the handle is
        // valid and carries `PROCESS_QUERY_LIMITED_INFORMATION`, which is the
        // right this call requires.
        let ok = unsafe {
            GetProcessTimes(
                handle.raw(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        };

        if ok == 0 {
            process.cpu_availability = Availability::temporarily_unavailable(
                "GetProcessTimes failed; the process may have exited",
            );
            return;
        }

        process.instance = ProcessInstanceId::new(
            process.instance.pid,
            times::start_token(creation.dwLowDateTime, creation.dwHighDateTime),
        );

        let kernel = times::filetime_to_u64(kernel.dwLowDateTime, kernel.dwHighDateTime);
        let user = times::filetime_to_u64(user.dwLowDateTime, user.dwHighDateTime);

        match times::cpu_time_nanos(kernel, user) {
            Some(nanos) => {
                process.cpu_time_nanos = Some(nanos);
                process.cpu_availability = Availability::Available;
            }
            None => {
                process.cpu_availability = Availability::temporarily_unavailable(
                    "this process reported a CPU time PULSE cannot express",
                );
            }
        }
    }

    /// The working set — Windows' equivalent of the Linux resident set.
    fn read_memory(handle: &OwnedHandle, process: &mut RawProcess) {
        let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;

        // SAFETY: `counters` is live, zeroed and carries its own size in `cb`,
        // which the API uses to decide how much to write.
        let ok = unsafe {
            K32GetProcessMemoryInfo(
                handle.raw(),
                &mut counters,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            )
        };

        if ok == 0 {
            process.memory_availability = Availability::temporarily_unavailable(
                "the process memory counters could not be read",
            );
            return;
        }

        process.resident_memory_bytes = Some(counters.WorkingSetSize as u64);
        process.memory_availability = Availability::Available;
    }

    /// Storage bytes read and written.
    fn read_io(handle: &OwnedHandle, process: &mut RawProcess) {
        let mut counters: IO_COUNTERS = unsafe { std::mem::zeroed() };

        // SAFETY: `counters` is a live, correctly typed local the API fills.
        let ok = unsafe { GetProcessIoCounters(handle.raw(), &mut counters) };

        if ok == 0 {
            process.io_availability =
                Availability::temporarily_unavailable("the process I/O counters were refused");
            return;
        }

        // `ReadTransferCount`/`WriteTransferCount` are byte totals;
        // `ReadOperationCount`/`WriteOperationCount` are operation counts and
        // `Other*` covers device control, which is neither a read nor a write.
        process.io = Some(RawProcessIo {
            read_bytes: counters.ReadTransferCount,
            write_bytes: counters.WriteTransferCount,
        });
        process.io_availability = Availability::Available;
    }

    /// The executable's full path, for grouping and classification.
    fn read_image_name(handle: &OwnedHandle, process: &mut RawProcess, root: Option<&str>) {
        const PROCESS_NAME_WIN32: u32 = 0;

        let mut buffer = [0_u16; MAX_PATH as usize + 1];
        let mut size = buffer.len() as u32;

        // SAFETY: the buffer is live and `size` holds its capacity on entry;
        // the API writes at most that many units and updates `size` with how
        // many it wrote.
        let ok = unsafe {
            QueryFullProcessImageNameW(
                handle.raw(),
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &mut size,
            )
        };

        if ok == 0 || size == 0 || size as usize > buffer.len() {
            process.executable_availability =
                Availability::permission_denied("Windows would not name this process's executable");
            return;
        }

        let path = utf16_to_string(&buffer[..size as usize]);
        process.class = classify::classify(process.instance.pid, Some(&path), root);
        process.executable_path = Some(path);
        process.executable_availability = Availability::Available;
    }
}

/// Compiled on non-Windows hosts so Fedora builds and tests this file. Never
/// reached: only the Windows platform builds the collector.
#[cfg(not(target_os = "windows"))]
mod imp {
    use crate::processes::RawProcessScan;

    use super::Depth;

    pub fn scan(_depth: Depth) -> RawProcessScan {
        RawProcessScan::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_provider_declares_exactly_the_three_machine_wide_metrics() {
        let definitions = WindowsProcessProvider::new().describe().expect("describes");

        assert_eq!(definitions.len(), 3);
        for definition in &definitions {
            assert_eq!(definition.provider_id.as_str(), PROVIDER_ID);
            assert_eq!(
                definition.metric.source_id.as_str(),
                wellknown::SOURCE,
                "no per-PID source may ever reach the catalog"
            );
        }
    }

    #[test]
    fn the_running_count_is_declared_unsupported_rather_than_omitted_or_faked() {
        let definitions = WindowsProcessProvider::new().describe().expect("describes");

        let running = definitions
            .iter()
            .find(|definition| definition.metric.key.as_str() == wellknown::COUNT_RUNNING)
            .expect("declared, so the interface can explain the gap");

        assert_eq!(running.availability.status_str(), "unsupported");

        for definition in &definitions {
            if definition.metric.key.as_str() != wellknown::COUNT_RUNNING {
                assert!(definition.availability.is_available());
            }
        }
    }

    #[test]
    fn sampling_the_running_count_yields_no_value_and_a_reason() {
        let reference =
            MetricRef::parse(wellknown::COUNT_RUNNING, wellknown::SOURCE).expect("valid");
        let sample = sample_for(
            &reference,
            ProcessCounts {
                total: 300,
                running: 0,
                threads: 900,
            },
        );

        assert!(sample.value.is_none());
        assert_eq!(sample.availability.status_str(), "unsupported");
    }

    #[test]
    fn the_other_two_keys_resolve_to_their_counts() {
        let counts = ProcessCounts {
            total: 300,
            running: 0,
            threads: 4_210,
        };

        let value_of = |key: &str| {
            let reference = MetricRef::parse(key, wellknown::SOURCE).expect("valid");
            match sample_for(&reference, counts).value {
                Some(MetricValue::Number(value)) => value,
                other => panic!("expected a number for {key}, got {other:?}"),
            }
        };

        assert_eq!(value_of(wellknown::COUNT_TOTAL), 300.0);
        assert_eq!(value_of(wellknown::THREAD_COUNT_TOTAL), 4_210.0);
    }

    #[test]
    fn an_unknown_reference_is_reported_as_unregistered() {
        let reference = MetricRef::parse("memory.used", "memory:system").expect("valid");
        let sample = sample_for(&reference, ProcessCounts::default());

        assert_eq!(sample.availability.status_str(), "notRegistered");
    }

    #[test]
    fn both_platforms_name_their_process_provider_after_their_platform() {
        assert_eq!(PROVIDER_ID, "windows.processes");
        assert_eq!(
            crate::platform::linux::processes::PROVIDER_ID,
            "linux.processes"
        );
        assert_ne!(PROVIDER_ID, crate::platform::linux::processes::PROVIDER_ID);
    }
}
