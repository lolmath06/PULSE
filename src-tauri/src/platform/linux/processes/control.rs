//! The Fedora process inspector and process control backends.
//!
//! ```text
//! inspect   /proc/<pid>/stat     identity, state, parent, start ticks
//!           /proc/<pid>/status   real/saved UID → owner, permission prediction
//!           /proc/<pid>/exe      path, size, mtime, ELF header → architecture
//!           /proc/stat btime     boot time → wall-clock start
//!           getpriority          nice value
//!           sched_getaffinity    CPU mask
//! control   pidfd_open           pins one incarnation
//!           pidfd_send_signal    SIGTERM / SIGKILL / SIGSTOP / SIGCONT
//!           setpriority          per thread, every thread of the process
//!           sched_setaffinity    per thread, every thread of the process
//! ```
//!
//! # pidfd: the process, not the number
//!
//! `pidfd_open(2)` returns a file descriptor that refers to one process for
//! its whole lifetime. PULSE opens it **before** reading the start token, then
//! checks the pidfd has not already exited, so the token it compares is
//! provably that process's. Signals then go through `pidfd_send_signal(2)`:
//! if the process exits and its PID is recycled at any point after the
//! check, the signal fails with `ESRCH` instead of reaching the newcomer.
//!
//! On a kernel without pidfds (before 5.3) PULSE falls back to re-reading the
//! start token immediately before `kill(2)`, and says so in the docs.
//!
//! # Threads
//!
//! Linux nice values and CPU affinities are per **thread**. `renice -p` and
//! `taskset -p` change only the main thread, which leaves a browser's worker
//! threads untouched. PULSE applies the change to every thread listed in
//! `/proc/<pid>/task` at that moment, checking the pidfd is still alive after
//! listing, and reports how many threads accepted.
//!
//! # No privilege, ever
//!
//! Everything runs with the permissions of the user who started PULSE.
//! `EPERM`/`EACCES` are reported as `permissionDenied`. There is no `sudo`,
//! no `pkexec`, no setuid helper and no daemon.

#[cfg(target_os = "linux")]
use std::sync::Arc;

#[cfg(target_os = "linux")]
use crate::processes::control::ProcessControlBackend;
#[cfg(target_os = "linux")]
use crate::processes::inspector::ProcessInspectorBackend;

/// Builds the Fedora control backend.
#[cfg(target_os = "linux")]
pub fn control_backend() -> Arc<dyn ProcessControlBackend> {
    Arc::new(LinuxProcessControl)
}

/// Builds the Fedora inspector backend.
#[cfg(target_os = "linux")]
pub fn inspector_backend() -> Arc<dyn ProcessInspectorBackend> {
    Arc::new(LinuxProcessInspector)
}

/// Controls processes through pidfds and ordinary Unix calls.
#[derive(Debug, Default)]
pub struct LinuxProcessControl;

/// Inspects processes through `/proc`.
#[derive(Debug, Default)]
pub struct LinuxProcessInspector;

/// Strips the suffix the kernel appends to `/proc/<pid>/exe` when the file
/// has been deleted or replaced since the process started.
pub fn strip_deleted_suffix(target: &str) -> (&str, bool) {
    match target.strip_suffix(" (deleted)") {
        Some(path) => (path, true),
        None => (target, false),
    }
}

/// The wall-clock start of a process, in Unix epoch milliseconds.
///
/// `boot_time_s` is `/proc/stat`'s `btime`; `start_ticks` is field 22 of
/// `/proc/<pid>/stat`. `btime` has one-second resolution, so the result is
/// accurate to about a second — which is what a start *time* needs. The
/// identity token is the raw tick count, never this.
pub fn start_time_ms(boot_time_s: u64, start_ticks: u64, ticks_per_second: u64) -> Option<u64> {
    if ticks_per_second == 0 {
        return None;
    }
    let offset_ms = u128::from(start_ticks) * 1_000 / u128::from(ticks_per_second);
    let boot_ms = u128::from(boot_time_s) * 1_000;
    u64::try_from(boot_ms + offset_ms).ok()
}

/// Extracts `btime` from `/proc/stat`.
pub fn parse_boot_time(proc_stat: &str) -> Option<u64> {
    proc_stat
        .lines()
        .find_map(|line| line.strip_prefix("btime "))
        .and_then(|value| value.trim().parse().ok())
}

#[cfg(target_os = "linux")]
mod sys {
    use std::ffi::CStr;
    use std::fs;
    use std::io::Read;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::time::UNIX_EPOCH;

    use crate::metrics::model::Availability;
    use crate::platform::linux::cpu_list::parse_cpu_list;
    use crate::processes::control::{
        Access, ControlError, PinnedProcess, PriorityKind, ProcessAffinity, ProcessControlBackend,
        ProcessNode, ProcessPriority, ResumeReport, SuspendRecord, TerminateMode, ThreadApply,
    };
    use crate::processes::field::Field;
    use crate::processes::inspector::{
        Capability, ExecutableInfo, OpenedExecutable, PlatformAccess, ProcessInspectorBackend,
        ProcessOwner, Provenance, RawDetails,
    };
    use crate::processes::{ProcessClass, ProcessState};

    use super::super::elf::{parse_header, HEADER_PREFIX_LEN};
    use super::super::stat::{self, ProcStat};
    use super::super::status::{may_control, parse_uids, Uids};
    use super::super::{rpm, scan, Depth};
    use super::{
        parse_boot_time, start_time_ms, strip_deleted_suffix, LinuxProcessControl,
        LinuxProcessInspector,
    };

    fn errno() -> i32 {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
    }

    fn read_stat(pid: u32) -> Result<ProcStat, ControlError> {
        let content =
            fs::read_to_string(format!("/proc/{pid}/stat")).map_err(|_| ControlError::Gone)?;
        let parsed = stat::parse(&content).ok_or(ControlError::Gone)?;
        if parsed.pid != pid {
            return Err(ControlError::Gone);
        }
        Ok(parsed)
    }

    fn ticks_per_second() -> u64 {
        // SAFETY: a pure query with no pointer arguments.
        let value = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        u64::try_from(value).ok().filter(|v| *v > 0).unwrap_or(100)
    }

    fn current_euid() -> u32 {
        // SAFETY: `geteuid` has no arguments and no error path.
        unsafe { libc::geteuid() }
    }

    fn uids(pid: u32) -> Option<Uids> {
        parse_uids(&fs::read_to_string(format!("/proc/{pid}/status")).ok()?)
    }

    /// Resolves a UID to its account name through NSS (`getpwuid_r`).
    fn user_name(uid: u32) -> Option<String> {
        let mut buffer = vec![0 as libc::c_char; 16 * 1024];
        // SAFETY: `passwd` is plain data; zeroed is a valid initial value.
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result: *mut libc::passwd = std::ptr::null_mut();

        // SAFETY: every pointer refers to a live local, and the buffer's
        // length is passed. On success `result` points at `entry`, whose
        // string fields point into `buffer`, which outlives the reads below.
        let rc = unsafe {
            libc::getpwuid_r(
                uid,
                &mut entry,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut result,
            )
        };
        if rc != 0 || result.is_null() || entry.pw_name.is_null() {
            return None;
        }

        // SAFETY: checked non-null above; NUL-terminated inside `buffer`.
        let name = unsafe { CStr::from_ptr(entry.pw_name) };
        Some(name.to_string_lossy().into_owned())
    }

    fn online_cpus() -> Vec<u32> {
        if let Ok(content) = fs::read_to_string("/sys/devices/system/cpu/online") {
            if let Ok(ids) = parse_cpu_list(&content) {
                if !ids.is_empty() {
                    return ids.into_iter().map(|id| id.get()).collect();
                }
            }
        }
        // SAFETY: a pure query.
        let count = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_CONF) };
        (0..u32::try_from(count).unwrap_or(1).max(1)).collect()
    }

    fn read_affinity(pid: u32) -> Result<ProcessAffinity, ControlError> {
        // SAFETY: `cpu_set_t` is a plain bit array; zeroed is empty.
        let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
        // SAFETY: `set` is live and its exact size is passed.
        let rc = unsafe {
            libc::sched_getaffinity(
                pid as libc::pid_t,
                std::mem::size_of::<libc::cpu_set_t>(),
                &mut set,
            )
        };
        if rc != 0 {
            return Err(errno_error(errno(), "reading the CPU affinity"));
        }

        let cpus = (0..libc::CPU_SETSIZE as usize)
            // SAFETY: `index` is below CPU_SETSIZE, the set's capacity.
            .filter(|index| unsafe { libc::CPU_ISSET(*index, &set) })
            .map(|index| index as u32)
            .collect();

        Ok(ProcessAffinity {
            cpus,
            available: online_cpus(),
            limitation: None,
        })
    }

    fn read_nice(pid: u32) -> Result<i32, ControlError> {
        // `getpriority` may legitimately return -1, so errno is the only
        // reliable failure signal and must be cleared first.
        // SAFETY: writing the calling thread's own errno.
        unsafe { *libc::__errno_location() = 0 };
        // SAFETY: a pure query taking integers.
        let value = unsafe { libc::getpriority(libc::PRIO_PROCESS, pid as libc::id_t) };
        let error = errno();
        if value == -1 && error != 0 {
            return Err(errno_error(error, "reading the nice value"));
        }
        Ok(value)
    }

    fn errno_error(code: i32, what: &str) -> ControlError {
        match code {
            libc::ESRCH => ControlError::Gone,
            libc::EPERM | libc::EACCES => ControlError::PermissionDenied(format!(
                "The system refused {what}: PULSE runs with your normal permissions and never \
                 elevates."
            )),
            libc::EINVAL => ControlError::Invalid(format!("The system rejected {what}.")),
            other => ControlError::Platform(format!(
                "{what} failed: {}",
                std::io::Error::from_raw_os_error(other)
            )),
        }
    }

    /// The thread IDs of `pid`, from `/proc/<pid>/task`.
    fn threads(pid: u32) -> Vec<u32> {
        let mut tids: Vec<u32> = fs::read_dir(format!("/proc/{pid}/task"))
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        // The main thread first: its failure is the process's failure.
        tids.sort_by_key(|tid| (*tid != pid, *tid));
        if tids.first() != Some(&pid) {
            tids.insert(0, pid);
        }
        tids
    }

    // --- pidfd ------------------------------------------------------------

    fn pidfd_open(pid: u32) -> Result<Option<OwnedFd>, ControlError> {
        // SAFETY: the documented raw syscall with integer arguments. A
        // non-negative return is a new descriptor this function now owns.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
        if fd >= 0 {
            // SAFETY: just returned by the kernel, owned by nobody else.
            return Ok(Some(unsafe { OwnedFd::from_raw_fd(fd as i32) }));
        }
        match errno() {
            libc::ESRCH => Err(ControlError::Gone),
            // Kernel without pidfds: fall back to token re-reads.
            libc::ENOSYS => Ok(None),
            code => Err(errno_error(code, "opening a process descriptor")),
        }
    }

    /// Whether the process a pidfd refers to has exited.
    fn pidfd_exited(fd: &OwnedFd) -> bool {
        let mut poll = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one live `pollfd`, count 1, zero timeout.
        let ready = unsafe { libc::poll(&mut poll, 1, 0) };
        ready > 0 && (poll.revents & libc::POLLIN) != 0
    }

    struct LinuxPinned {
        pid: u32,
        pidfd: Option<OwnedFd>,
        stat: ProcStat,
    }

    impl LinuxPinned {
        fn refuse_kernel_thread(&self) -> Result<(), ControlError> {
            if self.stat.is_kernel_thread() {
                Err(ControlError::Unsupported(
                    "PULSE does not control kernel threads.".to_string(),
                ))
            } else {
                Ok(())
            }
        }

        /// Proves the pinned process still holds the PID right now.
        fn still_alive(&self) -> Result<(), ControlError> {
            match &self.pidfd {
                Some(fd) if pidfd_exited(fd) => Err(ControlError::Gone),
                Some(_) => Ok(()),
                None => match read_stat(self.pid) {
                    Ok(now) if now.start_ticks == self.stat.start_ticks => Ok(()),
                    _ => Err(ControlError::Gone),
                },
            }
        }

        fn signal(&self, signal: libc::c_int, what: &str) -> Result<(), ControlError> {
            self.refuse_kernel_thread()?;

            let rc = match &self.pidfd {
                // SAFETY: a live pidfd, a valid signal number, no siginfo.
                Some(fd) => unsafe {
                    libc::syscall(
                        libc::SYS_pidfd_send_signal,
                        fd.as_raw_fd(),
                        signal,
                        std::ptr::null::<libc::siginfo_t>(),
                        0,
                    )
                },
                None => {
                    self.still_alive()?;
                    // SAFETY: integer arguments only.
                    i64::from(unsafe { libc::kill(self.pid as libc::pid_t, signal) })
                }
            };

            if rc == 0 {
                Ok(())
            } else {
                Err(errno_error(errno(), what))
            }
        }

        /// Applies a per-thread setting to every thread of the process.
        fn per_thread(
            &self,
            what: &str,
            apply: impl Fn(u32) -> i32,
        ) -> Result<ThreadApply, ControlError> {
            self.refuse_kernel_thread()?;

            let tids = threads(self.pid);
            // Listed while alive → these are this process's threads.
            self.still_alive()?;

            let mut result = ThreadApply::default();
            for tid in tids {
                if apply(tid) == 0 {
                    result.applied += 1;
                } else {
                    let code = errno();
                    if tid == self.pid {
                        return Err(errno_error(code, what));
                    }
                    // A worker thread that exited meanwhile is not a refusal.
                    if code != libc::ESRCH {
                        result.failed += 1;
                    }
                }
            }
            Ok(result)
        }
    }

    impl PinnedProcess for LinuxPinned {
        fn start_token(&self) -> u64 {
            self.stat.start_ticks
        }

        fn is_stopped(&self) -> bool {
            ProcessState::from_linux_char(self.stat.state) == ProcessState::Stopped
        }

        fn terminate(&self, mode: TerminateMode) -> Result<(), ControlError> {
            match mode {
                TerminateMode::Graceful => self.signal(libc::SIGTERM, "sending SIGTERM"),
                TerminateMode::Force => self.signal(libc::SIGKILL, "sending SIGKILL"),
            }
        }

        fn suspend(&self) -> Result<SuspendRecord, ControlError> {
            self.signal(libc::SIGSTOP, "sending SIGSTOP")?;
            Ok(SuspendRecord::default())
        }

        fn resume(&self, _record: &SuspendRecord) -> Result<ResumeReport, ControlError> {
            self.signal(libc::SIGCONT, "sending SIGCONT")?;
            Ok(ResumeReport {
                resumed: 1,
                ..ResumeReport::default()
            })
        }

        fn priority(&self) -> Result<ProcessPriority, ControlError> {
            read_nice(self.pid).map(|value| ProcessPriority::Nice { value })
        }

        fn set_priority(&self, priority: ProcessPriority) -> Result<ThreadApply, ControlError> {
            let ProcessPriority::Nice { value } = priority else {
                return Err(ControlError::Invalid("Linux uses nice values.".to_string()));
            };
            self.per_thread("changing the nice value", |tid| {
                // SAFETY: integer arguments only.
                unsafe { libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, value) }
            })
        }

        fn affinity(&self) -> Result<ProcessAffinity, ControlError> {
            read_affinity(self.pid)
        }

        fn set_affinity(&self, cpus: &[u32]) -> Result<ThreadApply, ControlError> {
            let online = online_cpus();
            if let Some(bad) = cpus.iter().find(|cpu| !online.contains(cpu)) {
                return Err(ControlError::Invalid(format!(
                    "CPU {bad} is not an online logical processor."
                )));
            }

            // SAFETY: a plain bit array; zeroed is the empty set.
            let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
            for cpu in cpus {
                if (*cpu as usize) < libc::CPU_SETSIZE as usize {
                    // SAFETY: bounds checked against the set's capacity.
                    unsafe { libc::CPU_SET(*cpu as usize, &mut set) };
                }
            }

            self.per_thread("changing the CPU affinity", |tid| {
                // SAFETY: `set` is live and its exact size is passed.
                unsafe {
                    libc::sched_setaffinity(
                        tid as libc::pid_t,
                        std::mem::size_of::<libc::cpu_set_t>(),
                        &set,
                    )
                }
            })
        }
    }

    impl ProcessControlBackend for LinuxProcessControl {
        fn pin(&self, pid: u32, _access: Access) -> Result<Box<dyn PinnedProcess>, ControlError> {
            let pidfd = pidfd_open(pid)?;
            let stat = read_stat(pid)?;

            // The stat read above belongs to the pidfd's process only if
            // that process had not exited by the time it was read.
            if let Some(fd) = &pidfd {
                if pidfd_exited(fd) {
                    return Err(ControlError::Gone);
                }
            }
            if ProcessState::from_linux_char(stat.state) == ProcessState::Zombie {
                return Err(ControlError::Gone);
            }

            Ok(Box::new(LinuxPinned { pid, pidfd, stat }))
        }

        fn process_nodes(&self) -> Vec<ProcessNode> {
            scan(Depth::Counts)
                .processes
                .into_iter()
                .map(|process| ProcessNode {
                    instance: process.instance,
                    parent_pid: process.parent_pid,
                })
                .collect()
        }

        fn priority_kind(&self) -> PriorityKind {
            PriorityKind::Nice
        }

        fn supports_force_kill(&self) -> bool {
            true
        }
    }

    // --- the inspector ----------------------------------------------------

    fn io_availability(error: &std::io::Error, what: &str) -> Availability {
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

    fn millis(time: std::io::Result<std::time::SystemTime>) -> Field<u64> {
        match time
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        {
            Some(duration) => Field::available(duration.as_millis() as u64),
            None => Field::missing(Availability::not_detected(
                "the file reports no modification time",
            )),
        }
    }

    fn executable(pid: u32, kernel: bool) -> Field<ExecutableInfo> {
        if kernel {
            return Field::missing(Availability::not_detected(
                "a kernel thread has no userspace executable",
            ));
        }

        let link = format!("/proc/{pid}/exe");
        let target = match fs::read_link(&link) {
            Ok(target) => target.to_string_lossy().into_owned(),
            Err(error) => return Field::missing(io_availability(&error, "the executable link")),
        };

        let (path, replaced) = strip_deleted_suffix(&target);
        // Metadata through the link: the file actually running, even if the
        // path on disk now names something else.
        let metadata = fs::metadata(&link);

        Field::available(ExecutableInfo {
            path: path.to_string(),
            file_name: path.rsplit('/').next().unwrap_or(path).to_string(),
            size_bytes: match &metadata {
                Ok(metadata) => Field::available(metadata.len()),
                Err(error) => Field::missing(io_availability(error, "the executable")),
            },
            modified_at: match &metadata {
                Ok(metadata) => millis(metadata.modified()),
                Err(error) => Field::missing(io_availability(error, "the executable")),
            },
            replaced_on_disk: replaced,
        })
    }

    fn architecture(pid: u32, kernel: bool) -> Field<String> {
        if kernel {
            return Field::missing(Availability::not_detected(
                "a kernel thread runs as part of the kernel itself",
            ));
        }

        let mut prefix = [0_u8; HEADER_PREFIX_LEN];
        match fs::File::open(format!("/proc/{pid}/exe"))
            .and_then(|mut file| file.read_exact(&mut prefix))
        {
            Ok(()) => match parse_header(&prefix) {
                Some(header) => Field::available(header.architecture()),
                None => Field::missing(Availability::not_detected(
                    "the executable is not an ELF binary",
                )),
            },
            Err(error) => Field::missing(io_availability(&error, "the executable")),
        }
    }

    fn category(stat: &ProcStat, uids: Option<&Uids>, euid: u32) -> ProcessClass {
        if stat.is_kernel_thread() {
            return ProcessClass::KernelThread;
        }
        match uids.map(|uids| uids.real) {
            Some(0) => ProcessClass::SystemProcess,
            Some(uid) if uid == euid => ProcessClass::UserApplication,
            _ => ProcessClass::Unknown,
        }
    }

    impl ProcessInspectorBackend for LinuxProcessInspector {
        fn inspect(&self, pid: u32) -> Result<RawDetails, ControlError> {
            let stat = read_stat(pid)?;
            let kernel = stat.is_kernel_thread();
            let euid = current_euid();
            let uids = uids(pid);

            let started_at = fs::read_to_string("/proc/stat")
                .ok()
                .and_then(|content| parse_boot_time(&content))
                .and_then(|boot| start_time_ms(boot, stat.start_ticks, ticks_per_second()));

            let owner = match &uids {
                Some(uids) => Field::available(ProcessOwner {
                    id: uids.real.to_string(),
                    name: user_name(uids.real),
                }),
                None => Field::missing(Availability::temporarily_unavailable(
                    "the process status file could not be read",
                )),
            };

            let access = match &uids {
                _ if kernel => PlatformAccess::all(Capability::denied(
                    "Kernel thread — PULSE does not control kernel threads.",
                )),
                Some(uids) if may_control(euid, uids) => PlatformAccess::all(Capability::allowed()),
                Some(uids) => PlatformAccess::all(Capability::denied(format!(
                    "Permission denied: owned by UID {}, and PULSE does not run as root.",
                    uids.real
                ))),
                None => {
                    PlatformAccess::all(Capability::denied("Permission could not be determined."))
                }
            };

            let parent_name = match read_stat(stat.parent_pid) {
                Ok(parent) => Field::available(parent.comm),
                Err(_) if stat.parent_pid == 0 => {
                    Field::missing(Availability::not_detected("started directly by the kernel"))
                }
                Err(_) => Field::missing(Availability::temporarily_unavailable(
                    "the parent exited while being read",
                )),
            };

            let priority = match read_nice(pid) {
                Ok(value) => Field::available(ProcessPriority::Nice { value }),
                Err(ControlError::PermissionDenied(reason)) => {
                    Field::missing(Availability::permission_denied(reason))
                }
                Err(_) => Field::missing(Availability::temporarily_unavailable(
                    "the nice value could not be read",
                )),
            };

            let affinity = match read_affinity(pid) {
                Ok(affinity) => Field::available(affinity),
                Err(ControlError::PermissionDenied(reason)) => {
                    Field::missing(Availability::permission_denied(reason))
                }
                Err(_) => Field::missing(Availability::temporarily_unavailable(
                    "the CPU affinity could not be read",
                )),
            };

            let details = RawDetails {
                start_token: stat.start_ticks,
                parent_pid: Some(stat.parent_pid).filter(|parent| *parent != 0),
                parent_name,
                name: stat.comm.clone(),
                state: ProcessState::from_linux_char(stat.state),
                state_availability: Availability::Available,
                category: category(&stat, uids.as_ref(), euid),
                started_at: match started_at {
                    Some(ms) => Field::available(ms),
                    None => Field::missing(Availability::not_detected(
                        "the boot time could not be read",
                    )),
                },
                executable: executable(pid, kernel),
                owner,
                architecture: architecture(pid, kernel),
                priority,
                affinity,
                version_info: Field::missing(Availability::unsupported(
                    "Linux executables carry no version resource; see the package instead",
                )),
                access,
            };

            // Everything above must describe one incarnation. If the PID
            // changed hands mid-read, the selected process is gone.
            match read_stat(pid) {
                Ok(again) if again.start_ticks == stat.start_ticks => Ok(details),
                _ => Err(ControlError::Gone),
            }
        }

        fn provenance(&self, pid: u32) -> Result<(u64, Provenance), ControlError> {
            let (token, path) = self.executable_path(pid)?;
            Ok((token, rpm::query(&path)))
        }

        fn open_executable(&self, pid: u32) -> Result<OpenedExecutable, ControlError> {
            let before = read_stat(pid)?;
            if before.is_kernel_thread() {
                return Err(ControlError::Unsupported(
                    "A kernel thread has no executable to hash.".to_string(),
                ));
            }

            let file =
                fs::File::open(format!("/proc/{pid}/exe")).map_err(|error| match error.kind() {
                    std::io::ErrorKind::PermissionDenied => ControlError::PermissionDenied(
                        "The executable belongs to another user and PULSE does not run as root."
                            .to_string(),
                    ),
                    std::io::ErrorKind::NotFound => ControlError::Gone,
                    _ => ControlError::Platform(format!("Opening the executable failed: {error}")),
                })?;

            let after = read_stat(pid)?;
            if after.start_ticks != before.start_ticks {
                return Err(ControlError::Gone);
            }

            Ok(OpenedExecutable {
                file,
                start_token: after.start_ticks,
            })
        }

        fn executable_path(&self, pid: u32) -> Result<(u64, String), ControlError> {
            let before = read_stat(pid)?;
            if before.is_kernel_thread() {
                return Err(ControlError::Unsupported(
                    "A kernel thread has no executable.".to_string(),
                ));
            }

            let target =
                fs::read_link(format!("/proc/{pid}/exe")).map_err(|error| match error.kind() {
                    std::io::ErrorKind::PermissionDenied => ControlError::PermissionDenied(
                        "The executable belongs to another user and PULSE does not run as root."
                            .to_string(),
                    ),
                    _ => ControlError::Gone,
                })?;
            let target = target.to_string_lossy().into_owned();
            let (path, replaced) = strip_deleted_suffix(&target);
            if replaced {
                return Err(ControlError::Unsupported(
                    "The executable was deleted or replaced on disk after this process started."
                        .to_string(),
                ));
            }

            let after = read_stat(pid)?;
            if after.start_ticks != before.start_ticks {
                return Err(ControlError::Gone);
            }

            Ok((after.start_ticks, path.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_a_deleted_executable() {
        assert_eq!(
            strip_deleted_suffix("/usr/bin/tool (deleted)"),
            ("/usr/bin/tool", true)
        );
        assert_eq!(
            strip_deleted_suffix("/usr/bin/tool"),
            ("/usr/bin/tool", false)
        );
    }

    #[test]
    fn computes_a_wall_clock_start() {
        // Booted at 1 700 000 000 s; started 12.34 s later at 100 Hz.
        assert_eq!(
            start_time_ms(1_700_000_000, 1_234, 100),
            Some(1_700_000_012_340)
        );
        assert_eq!(start_time_ms(1, 1, 0), None);
    }

    #[test]
    fn parses_the_boot_time() {
        assert_eq!(
            parse_boot_time("cpu  1 2 3\nctxt 99\nbtime 1700000000\nprocesses 5\n"),
            Some(1_700_000_000)
        );
        assert_eq!(parse_boot_time("cpu 1 2 3\n"), None);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod runtime_tests;
