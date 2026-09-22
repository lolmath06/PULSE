//! What a platform collector produces: raw, undifferenced, per-process facts.
//!
//! The split matters. A collector reads **counters**; it never computes a
//! rate, never normalises a percentage and never decides what "CPU usage"
//! means. That arithmetic lives in [`cpu`] and [`io`] and is therefore
//! identical on both platforms — which is the only way
//! `sum(process CPU) ≈ system CPU` can hold on Fedora *and* on Windows.
//!
//! [`cpu`]: super::cpu
//! [`io`]: super::io

use crate::metrics::model::Availability;

use super::identity::ProcessInstanceId;
use super::state::{ProcessClass, ProcessState};

/// Cumulative I/O counters for one process, in bytes since it started.
///
/// **Not the same notion on both platforms.** Linux `read_bytes`/`write_bytes`
/// from `/proc/<pid>/io` count bytes the block layer attributes to the process
/// — storage-backed traffic. Windows `ReadTransferCount`/`WriteTransferCount`
/// count the bytes of every read/write I/O operation the process performs, as
/// Windows accounts them, which is not restricted to a block device. The
/// columns share a name; `docs/metrics/processes.md` §6 spells out the
/// difference. On Linux, `rchar`/`wchar` are deliberately not used: they count
/// every byte passed to `read()`/`write()` — page cache, pipes, sockets,
/// `/proc` — and would report a cached re-read at gigabytes per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RawProcessIo {
    pub read_bytes: u64,
    pub write_bytes: u64,
}

/// One process as a collector found it.
///
/// Every optional field is paired with an availability explaining its absence,
/// because "the kernel refused this" and "this process has no such thing" are
/// different answers and the user is shown which.
#[derive(Debug, Clone, PartialEq)]
pub struct RawProcess {
    pub instance: ProcessInstanceId,
    /// The parent's PID, when the platform reports one. `None` for the root
    /// of the tree and wherever it could not be read.
    pub parent_pid: Option<u32>,
    /// The short name the operating system knows this process by.
    pub name: String,
    /// The executable's path, when readable.
    ///
    /// Not shown prominently: see the privacy section of
    /// `docs/metrics/processes.md`. It is carried because it is the only
    /// trustworthy key for grouping processes into applications.
    pub executable_path: Option<String>,
    /// Why `executable_path` is absent, when it is.
    pub executable_availability: Availability,
    /// Total CPU time consumed since start, in **nanoseconds**.
    ///
    /// Nanoseconds rather than each platform's native unit so the delta
    /// arithmetic is shared: Linux clock ticks and Windows 100 ns FILETIME
    /// units are both converted by their collector.
    pub cpu_time_nanos: Option<u64>,
    pub cpu_availability: Availability,
    /// Physical memory currently resident: Linux RSS, Windows working set.
    pub resident_memory_bytes: Option<u64>,
    pub memory_availability: Availability,
    pub thread_count: Option<u32>,
    pub thread_availability: Availability,
    pub io: Option<RawProcessIo>,
    pub io_availability: Availability,
    pub state: ProcessState,
    /// Why the state is [`ProcessState::Other`], when the platform simply does
    /// not report one. [`Availability::Available`] where the state is real.
    pub state_availability: Availability,
    pub class: ProcessClass,
}

impl RawProcess {
    /// A process known only by its inventory row — nothing else could be read.
    ///
    /// This is the shape a protected Windows process degrades to, and it is
    /// deliberately still a *row*: PID, name, parent and thread count are real
    /// facts, and hiding them because a handle was refused would understate
    /// what is running.
    pub fn inventory_only(
        instance: ProcessInstanceId,
        name: impl Into<String>,
        refused: Availability,
    ) -> Self {
        Self {
            instance,
            parent_pid: None,
            name: name.into(),
            executable_path: None,
            executable_availability: refused.clone(),
            cpu_time_nanos: None,
            cpu_availability: refused.clone(),
            resident_memory_bytes: None,
            memory_availability: refused.clone(),
            thread_count: None,
            thread_availability: refused.clone(),
            io: None,
            io_availability: refused.clone(),
            state: ProcessState::Other,
            state_availability: refused,
            class: ProcessClass::Unknown,
        }
    }
}

/// Everything one pass over the process table produced.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RawProcessScan {
    pub processes: Vec<RawProcess>,
    /// Installed physical memory, for the memory-percentage denominator.
    ///
    /// `None` leaves every process's memory percentage unavailable while
    /// leaving the byte figures intact — a missing denominator costs one
    /// column, not the table.
    pub physical_memory_total: Option<u64>,
    /// How many logical processors the CPU capacity is normalised against.
    ///
    /// `None` leaves every CPU percentage unavailable rather than dividing by
    /// a guess. See [`cpu`](super::cpu) for why this figure is the divisor.
    pub logical_processor_count: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_protected_process_still_produces_a_row() {
        let refused = Availability::permission_denied("OpenProcess was refused");
        let raw =
            RawProcess::inventory_only(ProcessInstanceId::new(4, 0), "System", refused.clone());

        assert_eq!(raw.name, "System");
        assert_eq!(raw.instance.pid, 4);
        for availability in [
            &raw.cpu_availability,
            &raw.memory_availability,
            &raw.io_availability,
            &raw.executable_availability,
        ] {
            assert_eq!(availability, &refused);
        }
        assert!(raw.cpu_time_nanos.is_none());
        assert!(raw.io.is_none());
        assert_eq!(raw.class, ProcessClass::Unknown);
    }

    #[test]
    fn an_empty_scan_is_a_valid_answer_not_a_failure() {
        let scan = RawProcessScan::default();

        assert!(scan.processes.is_empty());
        assert_eq!(scan.physical_memory_total, None);
        assert_eq!(scan.logical_processor_count, None);
    }
}
