//! [`ProcessSnapshotService`] — the high-cardinality half of PULSE's process
//! support.
//!
//! ```text
//!                      ┌── MetricsEngine ────────────────────────────┐
//!   low cardinality    │  linux.processes / windows.processes        │
//!   stable references  │    process.count.total                      │
//!                      │    process.count.running                    │
//!                      │    process.thread.count.total               │
//!                      └─────────────────────────────────────────────┘
//!
//!                      ┌── ProcessSnapshotService ───────────────────┐
//!   high cardinality   │  several hundred rows, renewed every        │
//!   ephemeral rows     │  refresh, requested through                 │
//!                      │  get_process_snapshot and then discarded    │
//!                      └─────────────────────────────────────────────┘
//! ```
//!
//! The two halves are separate because they have opposite lifetimes. A metric
//! reference is a promise: a dashboard saves `cpu.usage.total@cpu:system`
//! today and resolves it in six months. A process is the opposite of that —
//! `process:1234-9001` is meaningless the moment the process exits, and most
//! of them exit within seconds. Putting one in the other's container would
//! either fill the catalog with thousands of definitions that can never
//! resolve again, or force the snapshot through `sample_metrics()`, a call
//! shaped for tens of values rather than thousands.
//!
//! This service therefore owns its own state — the rate baselines — and its
//! own command, and touches the engine not at all.

use std::sync::Arc;
use std::time::Instant;

use crate::metrics::model::{now_ms, Availability};

use super::application::{aggregate, application_key};
use super::collector::ProcessCollector;
use super::field::Field;
use super::rates::{CounterSnapshot, ProcessCounters, ProcessRateTracker, Rate};
use super::raw::{RawProcess, RawProcessScan};
use super::snapshot::{ProcessCounts, ProcessEntry, ProcessSnapshot};

/// Builds process snapshots on demand.
///
/// Holds **numbers only** between refreshes: the rate baselines keyed by
/// process incarnation. No file descriptors, no Windows `HANDLE`s, nothing
/// whose lifetime could outlive the process it refers to. A machine with
/// four hundred processes keeps four hundred small records, and every one of
/// them is dropped the moment its process is no longer in a scan.
#[derive(Debug)]
pub struct ProcessSnapshotService {
    collector: Option<Arc<dyn ProcessCollector>>,
    tracker: ProcessRateTracker,
    /// Origin of the monotonic clock rates are measured against. Monotonic so
    /// an NTP correction between refreshes cannot become a CPU spike.
    origin: Instant,
}

impl ProcessSnapshotService {
    /// Builds the service.
    ///
    /// **Deliberately takes no baseline at startup.** Priming here would make
    /// the very first snapshot show rates, and PULSE would then be dividing by
    /// an interval the user never asked for — a hidden sample taken at launch.
    /// The first snapshot establishes the baseline and says so; the second,
    /// which is the user's first *Refresh*, carries real rates. That is one
    /// fewer walk of the process table at startup and one fewer number whose
    /// provenance the user cannot see.
    pub fn new(collector: Option<Arc<dyn ProcessCollector>>) -> Self {
        Self {
            collector,
            tracker: ProcessRateTracker::new(),
            origin: Instant::now(),
        }
    }

    /// A service for a platform with no process collector.
    pub fn unsupported() -> Self {
        Self::new(None)
    }

    /// Walks the process table once and derives everything from it.
    pub fn snapshot(&self) -> ProcessSnapshot {
        let Some(collector) = &self.collector else {
            return ProcessSnapshot::unsupported(
                "PULSE has no process collector for this platform",
            );
        };

        let started = Instant::now();
        let scan = collector.collect();
        let rates = self
            .tracker
            .update(&self.counters(&scan), scan.logical_processor_count);

        let mut processes: Vec<ProcessEntry> = scan
            .processes
            .iter()
            .map(|raw| entry(raw, rates.get(&raw.instance), scan.physical_memory_total))
            .collect();

        sort_processes(&mut processes);

        let counts = counts(&scan);
        let applications = aggregate(&processes);

        ProcessSnapshot {
            taken_at: now_ms(),
            duration_ms: started.elapsed().as_millis() as u64,
            counts,
            processes,
            applications,
            unsupported_reason: None,
        }
    }

    /// The counters this scan carries, stamped on the monotonic clock.
    fn counters(&self, scan: &RawProcessScan) -> CounterSnapshot {
        let mut snapshot =
            CounterSnapshot::new(Instant::now().duration_since(self.origin).as_nanos());

        for raw in &scan.processes {
            snapshot.insert(
                raw.instance,
                ProcessCounters {
                    cpu_time_nanos: raw.cpu_time_nanos,
                    io: raw.io,
                },
            );
        }

        snapshot
    }
}

/// The machine-wide counts, derived from the same pass the rows are.
///
/// Thread counts that could not be read contribute nothing rather than a
/// guess, so the total is an understatement on a machine full of protected
/// processes — which is the honest direction to be wrong in.
pub fn counts(scan: &RawProcessScan) -> ProcessCounts {
    ProcessCounts {
        total: scan.processes.len() as u32,
        running: scan
            .processes
            .iter()
            .filter(|raw| raw.state.is_running())
            .count() as u32,
        threads: scan
            .processes
            .iter()
            .filter_map(|raw| raw.thread_count)
            .fold(0_u32, |total, threads| total.saturating_add(threads)),
    }
}

/// Orders the process table: busiest first, then alphabetically, then by PID.
///
/// The tie-break exists so the table does not tremble. Several hundred
/// processes sit at exactly `0 %`, and without a deterministic order behind
/// the CPU column they would reshuffle on every *Refresh*, making the list
/// unreadable precisely when nothing is happening.
pub fn sort_processes(processes: &mut [ProcessEntry]) {
    processes.sort_by(|left, right| {
        right
            .cpu_percent
            .value
            .unwrap_or(-1.0)
            .partial_cmp(&left.cpu_percent.value.unwrap_or(-1.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.pid.cmp(&right.pid))
    });
}

/// Turns one raw process plus its derived rates into a row.
fn entry(
    raw: &RawProcess,
    rates: super::rates::ProcessRates,
    memory_total: Option<u64>,
) -> ProcessEntry {
    let (key, _) = application_key(raw.executable_path.as_deref(), &raw.name);

    let resident = match raw.resident_memory_bytes {
        Some(bytes) => Field::available(bytes as f64),
        None => Field::missing(raw.memory_availability.clone()),
    };

    ProcessEntry {
        instance_id: raw.instance.canonical_string(),
        pid: raw.instance.pid,
        parent_pid: raw.parent_pid,
        name: raw.name.clone(),
        executable_path: match &raw.executable_path {
            Some(path) => Field::available(path.clone()),
            None => Field::missing(raw.executable_availability.clone()),
        },
        state: raw.state,
        state_availability: raw.state_availability.clone(),
        classification: raw.class,
        cpu_percent: rate_field(
            raw.cpu_time_nanos.is_some(),
            rates.cpu_percent,
            &raw.cpu_availability,
        ),
        memory_percent: memory_percent(
            raw.resident_memory_bytes,
            memory_total,
            &raw.memory_availability,
        ),
        resident_memory_bytes: resident,
        thread_count: match raw.thread_count {
            Some(threads) => Field::available(f64::from(threads)),
            None => Field::missing(raw.thread_availability.clone()),
        },
        read_bytes_per_second: rate_field(
            raw.io.is_some(),
            rates.read_bytes_per_second,
            &raw.io_availability,
        ),
        write_bytes_per_second: rate_field(
            raw.io.is_some(),
            rates.write_bytes_per_second,
            &raw.io_availability,
        ),
        application_key: key,
    }
}

/// Resolves a rate into a field.
///
/// Three distinct outcomes, kept distinct: the counter was refused (the
/// platform's reason), the counter exists but has no baseline yet (waiting),
/// or a real rate — including a real zero.
fn rate_field(counter_present: bool, rate: Rate, refusal: &Availability) -> Field<f64> {
    if !counter_present {
        return Field::missing(refusal.clone());
    }

    match rate {
        Rate::Ready(value) => Field::available(value),
        Rate::NeedsAnotherSample => Field::waiting_for_another_sample(),
    }
}

/// Resident memory as a share of installed physical memory.
///
/// Checked throughout: a zero or missing denominator yields no measurement
/// rather than `NaN` or `Infinity`, and the result is clamped to 100 % because
/// a process cannot be more resident than the machine has memory.
fn memory_percent(resident: Option<u64>, total: Option<u64>, refusal: &Availability) -> Field<f64> {
    let Some(resident) = resident else {
        return Field::missing(refusal.clone());
    };

    let Some(total) = total.filter(|total| *total > 0) else {
        return Field::missing(Availability::not_detected(
            "installed physical memory is unknown, so a share of it cannot be computed",
        ));
    };

    let percent = (resident as f64) / (total as f64) * 100.0;

    if percent.is_finite() && percent >= 0.0 {
        Field::available(percent.min(100.0))
    } else {
        Field::missing(Availability::not_detected(
            "the memory share did not compute to a real number",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processes::identity::ProcessInstanceId;
    use crate::processes::raw::RawProcessIo;
    use crate::processes::state::{ProcessClass, ProcessState};
    use std::sync::Mutex;

    /// A collector that replays scripted scans, so the whole service is
    /// testable without a real process table.
    #[derive(Debug)]
    struct ScriptedCollector {
        scans: Mutex<Vec<RawProcessScan>>,
        calls: Mutex<usize>,
    }

    impl ScriptedCollector {
        fn new(scans: Vec<RawProcessScan>) -> Arc<Self> {
            Arc::new(Self {
                scans: Mutex::new(scans),
                calls: Mutex::new(0),
            })
        }

        fn calls(&self) -> usize {
            *self.calls.lock().expect("lock")
        }
    }

    impl ProcessCollector for ScriptedCollector {
        fn collect(&self) -> RawProcessScan {
            *self.calls.lock().expect("lock") += 1;

            let mut scans = self.scans.lock().expect("lock");
            if scans.len() > 1 {
                scans.remove(0)
            } else {
                scans.first().cloned().unwrap_or_default()
            }
        }
    }

    fn raw(pid: u32, token: u64, name: &str, cpu_nanos: u64, rss: u64) -> RawProcess {
        RawProcess {
            instance: ProcessInstanceId::new(pid, token),
            parent_pid: Some(1),
            name: name.to_string(),
            executable_path: Some(format!("/usr/bin/{name}")),
            executable_availability: Availability::Available,
            cpu_time_nanos: Some(cpu_nanos),
            cpu_availability: Availability::Available,
            resident_memory_bytes: Some(rss),
            memory_availability: Availability::Available,
            thread_count: Some(4),
            thread_availability: Availability::Available,
            io: Some(RawProcessIo {
                read_bytes: 0,
                write_bytes: 0,
            }),
            io_availability: Availability::Available,
            state: ProcessState::SleepingOrWaiting,
            state_availability: Availability::Available,
            class: ProcessClass::UserApplication,
        }
    }

    fn scan(processes: Vec<RawProcess>) -> RawProcessScan {
        RawProcessScan {
            processes,
            physical_memory_total: Some(32 * 1024 * 1024 * 1024),
            logical_processor_count: Some(32),
        }
    }

    #[test]
    fn a_platform_without_a_collector_returns_an_explained_empty_snapshot() {
        let snapshot = ProcessSnapshotService::unsupported().snapshot();

        assert!(snapshot.processes.is_empty());
        assert!(snapshot.applications.is_empty());
        assert!(snapshot.unsupported_reason.is_some());
        assert_eq!(snapshot.counts.total, 0);
    }

    #[test]
    fn the_first_snapshot_shows_memory_and_threads_but_waits_for_rates() {
        let collector = ScriptedCollector::new(vec![scan(vec![raw(10, 1, "firefox", 0, 1024)])]);
        let service = ProcessSnapshotService::new(Some(collector));
        let snapshot = service.snapshot();

        let process = &snapshot.processes[0];
        // Snapshot values: available immediately.
        assert_eq!(process.resident_memory_bytes.value, Some(1024.0));
        assert_eq!(process.thread_count.value, Some(4.0));
        assert_eq!(process.pid, 10);
        assert_eq!(process.instance_id, "process:10-1");

        // Rates: no interval exists yet, and none is invented.
        for field in [
            &process.cpu_percent,
            &process.read_bytes_per_second,
            &process.write_bytes_per_second,
        ] {
            assert_eq!(field.value, None);
            assert_eq!(field.availability.status_str(), "temporarilyUnavailable");
        }
    }

    #[test]
    fn construction_does_not_walk_the_process_table() {
        // No hidden sample at launch: the first walk is the first snapshot
        // the user asked for.
        let collector = ScriptedCollector::new(vec![scan(vec![raw(10, 1, "firefox", 0, 1024)])]);
        let service =
            ProcessSnapshotService::new(Some(collector.clone() as Arc<dyn ProcessCollector>));

        assert_eq!(collector.calls(), 0);
        service.snapshot();
        assert_eq!(collector.calls(), 1);
    }

    #[test]
    fn the_second_snapshot_reports_real_cpu_against_the_whole_machine() {
        let collector = ScriptedCollector::new(vec![
            scan(vec![raw(10, 1, "firefox", 0, 1024)]),
            scan(vec![raw(10, 1, "firefox", 10_000_000_000, 1024)]),
        ]);
        let service = ProcessSnapshotService::new(Some(collector));

        service.snapshot();
        let snapshot = service.snapshot();

        let cpu = snapshot.processes[0]
            .cpu_percent
            .value
            .expect("a baseline exists by now");
        // Ten seconds of CPU over a sub-millisecond interval saturates the
        // machine; the clamp is what keeps it from being a four-digit number.
        assert!((0.0..=100.0).contains(&cpu), "got {cpu}");
    }

    #[test]
    fn memory_percent_is_resident_over_installed() {
        let total = 32 * 1024 * 1024 * 1024_u64;
        let field = memory_percent(Some(total / 4), Some(total), &Availability::Available);
        assert_eq!(field.value, Some(25.0));
    }

    #[test]
    fn memory_percent_refuses_to_divide_by_an_unknown_or_zero_total() {
        for total in [None, Some(0)] {
            let field = memory_percent(Some(1024), total, &Availability::Available);
            assert_eq!(field.value, None);
            assert_eq!(field.availability.status_str(), "notDetected");
        }
    }

    #[test]
    fn memory_percent_carries_the_platforms_reason_when_memory_was_refused() {
        let refusal = Availability::permission_denied("the handle was refused");
        let field = memory_percent(None, Some(1024), &refusal);
        assert_eq!(field.availability, refusal);
    }

    #[test]
    fn memory_percent_is_never_negative_nan_or_above_one_hundred() {
        let field = memory_percent(Some(u64::MAX), Some(1), &Availability::Available);
        assert_eq!(field.value, Some(100.0));
    }

    #[test]
    fn a_refused_counter_keeps_its_reason_rather_than_becoming_waiting() {
        let refusal = Availability::permission_denied("/proc/7/io is not readable");
        let field = rate_field(false, Rate::NeedsAnotherSample, &refusal);

        assert_eq!(field.value, None);
        assert_eq!(field.availability, refusal);
    }

    #[test]
    fn a_present_counter_without_a_baseline_waits() {
        let field = rate_field(true, Rate::NeedsAnotherSample, &Availability::Available);
        assert_eq!(field.availability.status_str(), "temporarilyUnavailable");
    }

    #[test]
    fn a_measured_zero_stays_zero() {
        let field = rate_field(true, Rate::Ready(0.0), &Availability::Available);
        assert_eq!(field.value, Some(0.0));
    }

    #[test]
    fn counts_come_from_the_same_pass_as_the_rows() {
        let mut running = raw(10, 1, "yes", 0, 1024);
        running.state = ProcessState::Running;
        let mut zombie = raw(11, 2, "defunct", 0, 0);
        zombie.state = ProcessState::Zombie;
        let mut unreadable = raw(12, 3, "guarded", 0, 0);
        unreadable.thread_count = None;

        let counts = counts(&scan(vec![running, zombie, unreadable]));

        assert_eq!(counts.total, 3);
        assert_eq!(counts.running, 1);
        assert_eq!(counts.threads, 8, "the unreadable one contributes nothing");
    }

    #[test]
    fn a_process_that_vanishes_between_snapshots_simply_disappears() {
        let collector = ScriptedCollector::new(vec![
            scan(vec![raw(10, 1, "yes", 0, 1024), raw(11, 2, "bash", 0, 512)]),
            scan(vec![raw(11, 2, "bash", 0, 512)]),
        ]);
        let service = ProcessSnapshotService::new(Some(collector));

        assert_eq!(service.snapshot().counts.total, 2);
        let after = service.snapshot();
        assert_eq!(after.counts.total, 1);
        assert_eq!(after.processes[0].name, "bash");
    }

    #[test]
    fn the_table_is_sorted_by_cpu_then_name_then_pid() {
        let mut processes = vec![
            ProcessEntry {
                cpu_percent: Field::available(1.0),
                name: "zeta".into(),
                pid: 5,
                ..sample_entry()
            },
            ProcessEntry {
                cpu_percent: Field::available(1.0),
                name: "alpha".into(),
                pid: 9,
                ..sample_entry()
            },
            ProcessEntry {
                cpu_percent: Field::available(1.0),
                name: "alpha".into(),
                pid: 2,
                ..sample_entry()
            },
            ProcessEntry {
                cpu_percent: Field::available(9.0),
                name: "busy".into(),
                pid: 7,
                ..sample_entry()
            },
            ProcessEntry {
                cpu_percent: Field::waiting_for_another_sample(),
                name: "new".into(),
                pid: 8,
                ..sample_entry()
            },
        ];

        sort_processes(&mut processes);

        let order: Vec<(&str, u32)> = processes
            .iter()
            .map(|process| (process.name.as_str(), process.pid))
            .collect();
        assert_eq!(
            order,
            [
                ("busy", 7),
                ("alpha", 2),
                ("alpha", 9),
                ("zeta", 5),
                ("new", 8)
            ]
        );
    }

    fn sample_entry() -> ProcessEntry {
        ProcessEntry {
            instance_id: "process:1-1".into(),
            pid: 1,
            parent_pid: None,
            name: "init".into(),
            executable_path: Field::available("/usr/lib/systemd/systemd".into()),
            state: ProcessState::SleepingOrWaiting,
            state_availability: Availability::Available,
            classification: ProcessClass::SystemProcess,
            cpu_percent: Field::available(0.0),
            resident_memory_bytes: Field::available(0.0),
            memory_percent: Field::available(0.0),
            thread_count: Field::available(1.0),
            read_bytes_per_second: Field::available(0.0),
            write_bytes_per_second: Field::available(0.0),
            application_key: "exe:/usr/lib/systemd/systemd".into(),
        }
    }

    #[test]
    fn applications_are_built_from_the_same_rows_the_table_shows() {
        let collector = ScriptedCollector::new(vec![scan(vec![
            raw(10, 1, "firefox", 0, 1024),
            raw(11, 2, "firefox", 0, 2048),
            raw(12, 3, "bash", 0, 256),
        ])]);
        let snapshot = ProcessSnapshotService::new(Some(collector)).snapshot();

        assert_eq!(snapshot.processes.len(), 3);
        assert_eq!(snapshot.applications.len(), 2);
        let firefox = snapshot
            .applications
            .iter()
            .find(|app| app.display_name == "firefox")
            .expect("grouped");
        assert_eq!(firefox.process_count, 2);
        assert_eq!(firefox.resident_memory_bytes.value, Some(3072.0));
    }

    #[test]
    fn hundreds_of_processes_produce_one_snapshot_without_per_process_calls() {
        let processes: Vec<RawProcess> = (1..=500)
            .map(|pid| raw(pid, u64::from(pid), &format!("worker{pid}"), 0, 1024))
            .collect();
        let collector = ScriptedCollector::new(vec![scan(processes)]);

        let snapshot = ProcessSnapshotService::new(Some(collector)).snapshot();

        assert_eq!(snapshot.counts.total, 500);
        assert_eq!(snapshot.processes.len(), 500);
        assert_eq!(
            snapshot.applications.len(),
            500,
            "each has its own path here; grouping is by identity, not by name"
        );
    }

    #[test]
    fn a_reused_pid_is_a_new_row_with_no_inherited_rates() {
        let collector = ScriptedCollector::new(vec![
            scan(vec![raw(1234, 100, "firefox", 812_000_000_000, 4096)]),
            scan(vec![raw(1234, 200, "cargo", 200_000_000, 2048)]),
        ]);
        let service = ProcessSnapshotService::new(Some(collector));

        service.snapshot();
        let snapshot = service.snapshot();

        let process = &snapshot.processes[0];
        assert_eq!(process.name, "cargo");
        assert_eq!(process.instance_id, "process:1234-200");
        assert_eq!(
            process.cpu_percent.value, None,
            "the new incarnation must not inherit firefox's CPU baseline"
        );
        assert_eq!(
            process.cpu_percent.availability.status_str(),
            "temporarilyUnavailable"
        );
    }
}
