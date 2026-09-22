//! The Fedora process collector and the `linux.processes` provider.
//!
//! ```text
//! linux.processes                 3 machine-wide metrics, in the catalog
//!   └── LinuxProcessCollector     several hundred rows, outside the catalog
//!         ├── /proc                     which PIDs exist
//!         ├── /proc/<pid>/stat          name, state, parent, threads, CPU, RSS, start token
//!         ├── /proc/<pid>/io            storage bytes read and written
//!         ├── /proc/<pid>/exe           the executable, for grouping
//!         └── stat(/proc/<pid>)         the owning user, for classification
//! ```
//!
//! # No subprocesses, ever
//!
//! PULSE never runs `ps`, `top`, `htop`, `pidstat`, `pgrep` or `cat`. Each of
//! those is a program that opens the same files this module opens, formats the
//! result for a terminal, and hands back text in a locale-dependent layout
//! that then has to be parsed back into numbers. On a machine with four
//! hundred processes that would be a process spawn — often several — per
//! refresh, to obtain data already sitting in `/proc`.
//!
//! # What a refresh costs
//!
//! Per process, at most:
//!
//! ```text
//! 1  open + read   /proc/<pid>/stat
//! 1  open + read   /proc/<pid>/io        skipped for kernel threads
//! 1  readlink      /proc/<pid>/exe       skipped for kernel threads
//! 1  stat          /proc/<pid>           the owning user
//! ```
//!
//! plus one `read_dir` of `/proc`, one read of `/proc/meminfo` and one of
//! `/proc/stat` for the whole pass. `/proc/<pid>/statm` and
//! `/proc/<pid>/status` are deliberately **not** read: `stat` already carries
//! the resident set and the thread count, and adding them would be several
//! hundred more opens for numbers PULSE already has.
//!
//! The walk is sequential. It is a few hundred reads of files the kernel
//! generates on demand, and threading it would add synchronisation and a
//! scheduling cost to a loop whose measured duration is reported in every
//! snapshot — see `docs/metrics/processes.md`.
//!
//! # Processes disappear while you look at them
//!
//! Between `read_dir("/proc")` and the first `open`, and between any two
//! opens, a process may exit. `ENOENT` is therefore **not** a provider
//! failure: the instance is dropped from this snapshot and nothing else
//! changes. Nothing in this module panics on a missing file, a truncated read
//! or a refused permission.
//!
//! # Unprivileged by design
//!
//! PULSE does not run as root and does not ask to. The consequence is precise
//! and visible: `/proc/<pid>/io` and `/proc/<pid>/exe` belong to the process's
//! own user, so another user's processes report `permissionDenied` on their
//! I/O and executable path while keeping every other column. The row stays.

pub mod io;
pub mod stat;

use std::sync::Arc;

use crate::metrics::model::{
    Availability, MetricDefinition, MetricError, MetricRef, MetricSample, MetricValue, ProviderId,
};
use crate::metrics::providers::MetricProvider;
use crate::metrics::wellknown::process as wellknown;
use crate::processes::{ProcessCollector, ProcessCounts, RawProcessScan};

/// Identifier of the Linux process provider.
pub const PROVIDER_ID: &str = "linux.processes";

/// How much of each process to read.
///
/// The provider needs three numbers and would otherwise pay for a full walk —
/// several hundred extra opens — to publish them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// `/proc/<pid>/stat` only: enough for the counts.
    Counts,
    /// Everything a process row shows.
    Full,
}

/// Reads Fedora's process table.
#[derive(Debug, Default)]
pub struct LinuxProcessCollector;

impl LinuxProcessCollector {
    pub fn new() -> Self {
        Self
    }
}

impl ProcessCollector for LinuxProcessCollector {
    fn collect(&self) -> RawProcessScan {
        scan(Depth::Full)
    }
}

/// Builds the collector Fedora's snapshot service uses.
pub fn collector() -> Arc<dyn ProcessCollector> {
    Arc::new(LinuxProcessCollector::new())
}

// --- the walk --------------------------------------------------------------

#[cfg(target_os = "linux")]
mod sys {
    use std::fs;
    use std::path::Path;

    use crate::metrics::model::Availability;
    use crate::processes::{
        ProcessClass, ProcessInstanceId, ProcessState, RawProcess, RawProcessScan,
    };

    use super::{io, stat, Depth};

    const PROC: &str = "/proc";

    /// The fallback used when `sysconf` cannot answer.
    ///
    /// `USER_HZ` has been 100 on every Linux architecture PULSE targets since
    /// the kernel began exporting `/proc` times in these units, and a wrong
    /// tick rate scales the CPU column rather than breaking it.
    const DEFAULT_CLOCK_TICKS: u64 = 100;
    const DEFAULT_PAGE_SIZE: u64 = 4096;

    /// Clock ticks per second, the unit `/proc/<pid>/stat` reports CPU time in.
    fn clock_ticks() -> u64 {
        // SAFETY: `sysconf` is a pure query with no pointer arguments. A
        // negative return means "unspecified", which is handled.
        let value = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        u64::try_from(value)
            .ok()
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_CLOCK_TICKS)
    }

    /// The size of the pages `/proc/<pid>/stat` counts the resident set in.
    fn page_size() -> u64 {
        // SAFETY: as above.
        let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        u64::try_from(value)
            .ok()
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_PAGE_SIZE)
    }

    /// The effective user PULSE runs as, for classifying processes.
    fn current_uid() -> u32 {
        // SAFETY: `geteuid` takes no arguments, touches no memory and cannot
        // fail. It is one of the few POSIX calls with no error path at all.
        unsafe { libc::geteuid() }
    }

    /// How many logical processors the CPU percentages are normalised against.
    ///
    /// Counted from `/proc/stat`'s per-processor lines — the same file
    /// `cpu.usage.total` is derived from — so the process column and the
    /// system gauge are normalised against the same denominator and can be
    /// compared.
    fn logical_processor_count() -> Option<u32> {
        let content = fs::read_to_string("/proc/stat").ok()?;

        let count = content
            .lines()
            .filter(|line| {
                line.strip_prefix("cpu")
                    .and_then(|rest| rest.chars().next())
                    .is_some_and(|first| first.is_ascii_digit())
            })
            .count();

        u32::try_from(count).ok().filter(|count| *count > 0)
    }

    /// Installed physical memory, for the memory-percentage denominator.
    fn physical_memory_total() -> Option<u64> {
        let content = fs::read_to_string("/proc/meminfo").ok()?;
        crate::platform::linux::memory::parse_meminfo(&content)
            .ok()
            .map(|reading| reading.total())
    }

    /// Turns a failed `/proc` read into the availability that describes it.
    ///
    /// The three cases are genuinely different and the interface says which:
    /// a refusal the user could act on, a process that has exited, and a
    /// kernel that simply does not keep this counter.
    fn availability_for(error: &std::io::Error, what: &str) -> Availability {
        match error.kind() {
            std::io::ErrorKind::PermissionDenied => Availability::permission_denied(format!(
                "{what} belongs to another user and PULSE does not run as root"
            )),
            std::io::ErrorKind::NotFound => {
                Availability::not_detected(format!("{what} does not exist for this process"))
            }
            _ => Availability::temporarily_unavailable(format!("{what} could not be read")),
        }
    }

    /// Walks `/proc` once.
    pub fn scan(depth: Depth) -> RawProcessScan {
        let Ok(entries) = fs::read_dir(PROC) else {
            // `/proc` not mounted. An empty scan, not a panic: every other
            // part of PULSE keeps working.
            return RawProcessScan::default();
        };

        let ticks = clock_ticks();
        let pages = page_size();
        let uid = current_uid();

        let mut processes = Vec::with_capacity(512);

        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };

            if let Some(process) = read_process(pid, depth, ticks, pages, uid) {
                processes.push(process);
            }
        }

        RawProcessScan {
            processes,
            physical_memory_total: physical_memory_total(),
            logical_processor_count: logical_processor_count(),
        }
    }

    /// Reads one process, or `None` when it is already gone.
    fn read_process(
        pid: u32,
        depth: Depth,
        ticks: u64,
        pages: u64,
        uid: u32,
    ) -> Option<RawProcess> {
        let directory = format!("{PROC}/{pid}");

        // The one file whose absence means "this process no longer exists".
        // Everything after it is optional.
        let stat = stat::parse(&fs::read_to_string(format!("{directory}/stat")).ok()?)?;

        // The kernel's own PID, not the directory name: a mismatch would mean
        // `/proc` changed under us mid-read.
        if stat.pid != pid {
            return None;
        }

        let instance = ProcessInstanceId::new(pid, stat.start_ticks);
        let kernel_thread = stat.is_kernel_thread();
        let state = ProcessState::from_linux_char(stat.state);

        let mut process = RawProcess {
            instance,
            parent_pid: Some(stat.parent_pid),
            name: stat.comm.clone(),
            executable_path: None,
            executable_availability: Availability::not_detected("not read"),
            cpu_time_nanos: stat.cpu_time_nanos(ticks),
            cpu_availability: Availability::Available,
            resident_memory_bytes: Some(stat.resident_bytes(pages)),
            memory_availability: Availability::Available,
            thread_count: Some(stat.num_threads),
            thread_availability: Availability::Available,
            io: None,
            io_availability: Availability::not_detected("not read"),
            state,
            state_availability: Availability::Available,
            class: ProcessClass::Unknown,
        };

        if process.cpu_time_nanos.is_none() {
            process.cpu_availability = Availability::temporarily_unavailable(
                "this process reported a CPU time PULSE cannot express",
            );
        }

        if depth == Depth::Counts {
            return Some(process);
        }

        process.class = classify(&directory, kernel_thread, uid);

        if kernel_thread {
            // A kernel thread has no executable and no page cache of its own.
            // Saying so is a fact about it, not a failure to read it.
            let reason = Availability::not_detected("a kernel thread has no executable of its own");
            process.executable_availability = reason.clone();
            process.io_availability = reason;
            return Some(process);
        }

        match fs::read_link(format!("{directory}/exe")) {
            Ok(path) => {
                process.executable_path = Some(path.to_string_lossy().into_owned());
                process.executable_availability = Availability::Available;
            }
            Err(error) => {
                process.executable_availability = availability_for(&error, "the executable link");
            }
        }

        match fs::read_to_string(format!("{directory}/io")) {
            Ok(content) => match io::parse(&content) {
                Some(counters) => {
                    process.io = Some(counters);
                    process.io_availability = Availability::Available;
                }
                None => {
                    process.io_availability = Availability::not_detected(
                        "this kernel keeps no per-process block I/O accounting",
                    );
                }
            },
            Err(error) => {
                process.io_availability = availability_for(&error, "the I/O accounting file");
            }
        }

        Some(process)
    }

    /// Classifies one process.
    ///
    /// Three reliable signals, in order: the kernel's own `PF_KTHREAD` flag,
    /// ownership by root, and ownership by the user PULSE runs as. Anything
    /// else — a process belonging to a third user, or a directory PULSE
    /// cannot stat — is [`ProcessClass::Unknown`] rather than a guess from the
    /// process's name.
    fn classify(directory: &str, kernel_thread: bool, uid: u32) -> ProcessClass {
        if kernel_thread {
            return ProcessClass::SystemProcess;
        }

        let Ok(metadata) = fs::metadata(Path::new(directory)) else {
            return ProcessClass::Unknown;
        };

        use std::os::unix::fs::MetadataExt;
        match metadata.uid() {
            0 => ProcessClass::SystemProcess,
            owner if owner == uid => ProcessClass::UserApplication,
            _ => ProcessClass::Unknown,
        }
    }
}

/// Compiled on non-Linux hosts so the Windows cross-check harness can type
/// check this module. Never reached: only the Linux platform builds it.
#[cfg(not(target_os = "linux"))]
mod sys {
    use crate::processes::RawProcessScan;

    use super::Depth;

    pub fn scan(_depth: Depth) -> RawProcessScan {
        RawProcessScan::default()
    }
}

/// Walks `/proc` at the requested depth.
pub fn scan(depth: Depth) -> RawProcessScan {
    sys::scan(depth)
}

// --- the machine-wide provider ---------------------------------------------

/// Publishes the three machine-wide process metrics on Fedora.
///
/// **Three metrics, whatever the machine runs.** A laptop with 180 processes
/// and a build server with 900 produce the same three catalog entries; the
/// hundreds of rows go through [`ProcessSnapshotService`] instead. See
/// [`wellknown`] for why.
///
/// [`ProcessSnapshotService`]: crate::processes::ProcessSnapshotService
#[derive(Debug)]
pub struct LinuxProcessProvider {
    id: ProviderId,
}

impl LinuxProcessProvider {
    pub fn new() -> Self {
        Self {
            id: ProviderId::new(PROVIDER_ID).expect("provider id must be valid"),
        }
    }
}

impl Default for LinuxProcessProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for LinuxProcessProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError> {
        Ok(wellknown::definitions(&self.id, Availability::Available))
    }

    fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError> {
        // One walk serves all three metrics, and reads only `stat`: the I/O
        // counters and executable links a row needs cost nothing here.
        let counts = crate::processes::counts(&scan(Depth::Counts));

        Ok(requested
            .iter()
            .map(|reference| sample_for(reference, counts))
            .collect())
    }
}

/// Resolves one requested reference against a set of counts.
fn sample_for(reference: &MetricRef, counts: ProcessCounts) -> MetricSample {
    let value = match reference.key.as_str() {
        wellknown::COUNT_TOTAL => Some(f64::from(counts.total)),
        wellknown::COUNT_RUNNING => Some(f64::from(counts.running)),
        wellknown::THREAD_COUNT_TOTAL => Some(f64::from(counts.threads)),
        _ => None,
    };

    match value.and_then(MetricValue::number) {
        Some(value) => MetricSample::available(reference.clone(), value),
        None => MetricSample::unavailable(
            reference.clone(),
            Availability::not_registered(format!(
                "'{}' is not a metric the Linux process provider owns",
                reference.key
            )),
        ),
    }
}

/// Builds the Fedora process provider.
pub fn provider() -> Arc<dyn MetricProvider> {
    Arc::new(LinuxProcessProvider::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    use crate::processes::ProcessClass;

    #[test]
    fn the_provider_declares_exactly_the_three_machine_wide_metrics() {
        let definitions = LinuxProcessProvider::new().describe().expect("describes");

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
    fn an_unknown_reference_is_reported_as_unregistered_not_as_zero() {
        let reference = MetricRef::parse("cpu.usage.total", "cpu:system").expect("valid");
        let sample = sample_for(&reference, ProcessCounts::default());

        assert!(sample.value.is_none());
        assert_eq!(sample.availability.status_str(), "notRegistered");
    }

    #[test]
    fn the_three_keys_resolve_to_their_own_counts() {
        let counts = ProcessCounts {
            total: 342,
            running: 2,
            threads: 1_204,
        };

        let value_of = |key: &str| {
            let reference = MetricRef::parse(key, wellknown::SOURCE).expect("valid");
            match sample_for(&reference, counts).value {
                Some(MetricValue::Number(value)) => value,
                other => panic!("expected a number for {key}, got {other:?}"),
            }
        };

        assert_eq!(value_of(wellknown::COUNT_TOTAL), 342.0);
        assert_eq!(value_of(wellknown::COUNT_RUNNING), 2.0);
        assert_eq!(value_of(wellknown::THREAD_COUNT_TOTAL), 1_204.0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_real_walk_finds_this_test_process_and_stays_consistent() {
        let scan = scan(Depth::Full);

        assert!(
            scan.processes.len() > 1,
            "a Linux machine always runs more than one process"
        );
        assert!(scan.physical_memory_total.is_some_and(|total| total > 0));
        assert!(scan.logical_processor_count.is_some_and(|count| count > 0));

        let me = std::process::id();
        let mine = scan
            .processes
            .iter()
            .find(|process| process.instance.pid == me)
            .expect("the test process must appear in its own scan");

        assert!(mine.cpu_time_nanos.is_some());
        assert!(mine.resident_memory_bytes.is_some_and(|rss| rss > 0));
        assert!(mine.thread_count.is_some_and(|threads| threads >= 1));
        assert!(mine.executable_path.is_some(), "PULSE can read its own exe");
        assert_eq!(mine.class, ProcessClass::UserApplication);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn every_instance_identity_in_a_real_walk_is_unique() {
        // Duplicate identities would make two processes share one baseline.
        let scan = scan(Depth::Full);

        let mut identities: Vec<_> = scan
            .processes
            .iter()
            .map(|process| process.instance)
            .collect();
        let total = identities.len();
        identities.sort_unstable();
        identities.dedup();

        assert_eq!(identities.len(), total);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_counts_only_walk_agrees_with_a_full_one_about_the_machine() {
        let counts = crate::processes::counts(&scan(Depth::Counts));
        let full = crate::processes::counts(&scan(Depth::Full));

        assert!(counts.total > 1);
        assert!(counts.threads >= counts.total);
        // Processes start and exit between the two walks, so this is a
        // tolerance rather than an equality.
        let difference = counts.total.abs_diff(full.total);
        assert!(difference < 50, "the two walks disagreed by {difference}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kernel_threads_are_classified_as_system_processes() {
        let scan = scan(Depth::Full);

        // PID 2 is `kthreadd`, the parent of every kernel thread, on every
        // Linux system PULSE supports.
        if let Some(kthreadd) = scan
            .processes
            .iter()
            .find(|process| process.instance.pid == 2)
        {
            assert_eq!(kthreadd.class, ProcessClass::SystemProcess);
            assert!(kthreadd.executable_path.is_none());
            assert!(kthreadd.io.is_none());
            assert!(
                kthreadd.thread_count.is_some(),
                "a kernel thread still has an inventory row"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_process_that_cannot_be_read_fully_still_appears() {
        let scan = scan(Depth::Full);

        // On a normal desktop, some processes belong to root. Those keep
        // their inventory and lose only what needs their own permissions.
        for process in &scan.processes {
            assert!(!process.name.is_empty() || process.instance.pid > 0);
            if process.io.is_none() {
                assert!(
                    !process.io_availability.is_available(),
                    "a missing counter must carry a reason"
                );
            }
        }
    }
}
