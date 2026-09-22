//! Turning cumulative per-process counters into rates.
//!
//! # A rate exists between two samples, never within one
//!
//! `/proc/<pid>/io` and `GetProcessIoCounters` report totals since the process
//! started; `utime + stime` and `GetProcessTimes` report CPU time since the
//! process started. None of them is a rate. The first snapshot therefore has
//! nothing to divide and says so — `waiting for another sample` — rather than
//! reporting `0 %` and `0 B/s`, which a user reads as *measured, and idle*.
//!
//! From the second snapshot on, a genuine zero is a genuine measurement and is
//! shown as zero.
//!
//! # CPU: one convention, both platforms
//!
//! ```text
//!               delta CPU time
//! percent =  ────────────────────────────  × 100
//!            delta wall time × logical CPUs
//! ```
//!
//! **0–100 % is a share of the machine's entire CPU capacity.** On a
//! 32-thread machine, one thread pinning one logical processor is `3.125 %`,
//! not `100 %`. This is the `Windows` convention rather than the `top`
//! convention, and it is chosen because it is the only one under which
//!
//! ```text
//! sum(process CPU) ≈ cpu.usage.total
//! ```
//!
//! holds — which is what makes the process list *explain* the system gauge
//! instead of contradicting it. Reporting `3200 %` on Fedora and `100 %` on
//! Windows for the same workload, as most monitors do, would make the column
//! meaningless across the two platforms PULSE treats as equals.
//!
//! # What is guarded against
//!
//! | Situation | Answer |
//! |---|---|
//! | no baseline (first sample, or new process) | waiting |
//! | zero elapsed time (two refreshes in one instant) | waiting, baseline kept |
//! | counter moved backwards (reset, suspend/resume) | waiting, baseline replaced |
//! | process vanished | no entry; baseline pruned |
//! | PID reused | different [`ProcessInstanceId`], so no baseline |
//! | absurd delta | clamped to the machine's capacity |
//!
//! Nothing here can produce `NaN`, `Infinity` or a negative rate: every result
//! goes through [`finite_non_negative`].

use std::collections::BTreeMap;
use std::sync::Mutex;

use super::identity::ProcessInstanceId;
use super::raw::RawProcessIo;

/// The largest CPU share one process can be reported at.
///
/// A process genuinely cannot exceed the whole machine. A larger figure means
/// the counters disagreed with the clock — a suspended-then-resumed process,
/// a coarse tick granularity over a very short interval — and clamping is
/// honest where a 4 000 % spike would not be.
const MAX_CPU_PERCENT: f64 = 100.0;

/// One process's cumulative counters at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcessCounters {
    /// Total CPU time since start, in nanoseconds.
    pub cpu_time_nanos: Option<u64>,
    /// Total storage bytes read and written since start.
    pub io: Option<RawProcessIo>,
}

/// Every process's counters for one refresh, at one monotonic instant.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CounterSnapshot {
    /// Nanoseconds since the tracker's monotonic origin.
    ///
    /// Monotonic rather than wall-clock: an NTP step between two refreshes
    /// must not become a CPU spike or a negative interval.
    pub at_nanos: u128,
    pub entries: BTreeMap<ProcessInstanceId, ProcessCounters>,
}

impl CounterSnapshot {
    pub fn new(at_nanos: u128) -> Self {
        Self {
            at_nanos,
            entries: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, instance: ProcessInstanceId, counters: ProcessCounters) {
        self.entries.insert(instance, counters);
    }
}

/// A derived rate, or the honest statement that one cannot exist yet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rate {
    Ready(f64),
    NeedsAnotherSample,
}

impl Rate {
    pub fn value(self) -> Option<f64> {
        match self {
            Rate::Ready(value) => Some(value),
            Rate::NeedsAnotherSample => None,
        }
    }
}

/// The three rates one process yields per refresh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessRates {
    pub cpu_percent: Rate,
    pub read_bytes_per_second: Rate,
    pub write_bytes_per_second: Rate,
}

impl Default for ProcessRates {
    fn default() -> Self {
        Self {
            cpu_percent: Rate::NeedsAnotherSample,
            read_bytes_per_second: Rate::NeedsAnotherSample,
            write_bytes_per_second: Rate::NeedsAnotherSample,
        }
    }
}

/// Rejects anything that is not a real, non-negative measurement.
///
/// `NaN` and `Infinity` serialise to JSON `null` and would reach the interface
/// as an empty cell indistinguishable from a refused permission. They are
/// turned into "no measurement" here instead, at the one place every rate
/// passes through.
fn finite_non_negative(value: f64) -> Option<f64> {
    (value.is_finite() && value >= 0.0).then_some(value)
}

/// The share of the **whole machine's** CPU capacity a process used.
///
/// Returns `None` where no honest answer exists: no elapsed time, no logical
/// processors, or arithmetic that did not stay finite.
pub fn cpu_percent(delta_cpu_nanos: u64, elapsed_nanos: u128, logical: u32) -> Option<f64> {
    if elapsed_nanos == 0 || logical == 0 {
        return None;
    }

    let capacity_nanos = (elapsed_nanos as f64) * f64::from(logical);
    let percent = (delta_cpu_nanos as f64) / capacity_nanos * 100.0;

    finite_non_negative(percent).map(|percent| percent.min(MAX_CPU_PERCENT))
}

/// A per-second rate from a byte delta and an interval.
pub fn per_second(delta: u64, elapsed_nanos: u128) -> Option<f64> {
    if elapsed_nanos == 0 {
        return None;
    }

    const NANOS_PER_SECOND: f64 = 1_000_000_000.0;
    let seconds = (elapsed_nanos as f64) / NANOS_PER_SECOND;

    finite_non_negative((delta as f64) / seconds)
}

/// A counter's delta, or `None` when it cannot be differenced.
///
/// `None` means *the baseline is not usable*: either there is none, or the
/// counter moved backwards. A backwards counter is never clamped to zero —
/// that would report "idle" for a process that is anything but.
fn delta(previous: Option<u64>, current: Option<u64>) -> Option<u64> {
    match (previous, current) {
        (Some(previous), Some(current)) => current.checked_sub(previous),
        _ => None,
    }
}

/// Remembers each process incarnation's counters between refreshes.
///
/// Keyed on [`ProcessInstanceId`] — PID **and** start token — so a recycled
/// PID starts from no baseline instead of inheriting its predecessor's. See
/// [`identity`](super::identity).
///
/// Holds numbers only: no file descriptors, no Windows `HANDLE`s, nothing
/// whose lifetime outlives a refresh.
#[derive(Debug, Default)]
pub struct ProcessRateTracker {
    baselines: Mutex<BTreeMap<ProcessInstanceId, Baseline>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Baseline {
    at_nanos: u128,
    counters: ProcessCounters,
}

impl ProcessRateTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a snapshot without deriving anything from it.
    ///
    /// Used at construction so that the user's first *Refresh* already has a
    /// real interval to divide by, while the first snapshot still reports
    /// "waiting" honestly rather than inventing a zero.
    pub fn prime(&self, snapshot: &CounterSnapshot) {
        let mut baselines = self.lock();
        baselines.clear();
        for (instance, counters) in &snapshot.entries {
            baselines.insert(
                *instance,
                Baseline {
                    at_nanos: snapshot.at_nanos,
                    counters: *counters,
                },
            );
        }
    }

    /// Differences a snapshot against the stored baselines and replaces them.
    ///
    /// Processes absent from `snapshot` have their baselines dropped: a
    /// machine that has run for hours must not accumulate an entry for every
    /// short-lived `sh` it ever spawned.
    pub fn update(&self, snapshot: &CounterSnapshot, logical: Option<u32>) -> Rates {
        let mut baselines = self.lock();
        let mut rates = BTreeMap::new();

        for (instance, counters) in &snapshot.entries {
            let previous = baselines.get(instance).copied();
            rates.insert(
                *instance,
                resolve(previous, *counters, snapshot.at_nanos, logical),
            );
        }

        // Replace wholesale rather than merge: an instance that vanished is
        // gone, and one that reappears is by definition a different
        // incarnation with a different key.
        *baselines = snapshot
            .entries
            .iter()
            .map(|(instance, counters)| {
                (
                    *instance,
                    Baseline {
                        at_nanos: snapshot.at_nanos,
                        counters: *counters,
                    },
                )
            })
            .collect();

        Rates(rates)
    }

    /// How many baselines are currently held. Used by tests to prove pruning.
    pub fn tracked(&self) -> usize {
        self.lock().len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<ProcessInstanceId, Baseline>> {
        // A panic inside a tracker update would otherwise poison the lock and
        // disable the process list for the rest of the session. Recovering the
        // data is strictly better: worst case one refresh reports "waiting".
        self.baselines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Derives one process's rates from its baseline.
fn resolve(
    previous: Option<Baseline>,
    current: ProcessCounters,
    at_nanos: u128,
    logical: Option<u32>,
) -> ProcessRates {
    let Some(previous) = previous else {
        // A process first seen this refresh — including every process on the
        // very first snapshot.
        return ProcessRates::default();
    };

    // Two refreshes inside one clock tick. Dividing would be a division by
    // zero; the baseline is deliberately left in place by the caller's
    // wholesale replacement, which writes the same instant back.
    let Some(elapsed) = at_nanos.checked_sub(previous.at_nanos).filter(|e| *e > 0) else {
        return ProcessRates::default();
    };

    let cpu_percent = delta(previous.counters.cpu_time_nanos, current.cpu_time_nanos)
        .and_then(|delta| logical.and_then(|logical| cpu_percent(delta, elapsed, logical)))
        .map_or(Rate::NeedsAnotherSample, Rate::Ready);

    let (read, write) = match (previous.counters.io, current.io) {
        (Some(previous), Some(current)) => (
            delta(Some(previous.read_bytes), Some(current.read_bytes))
                .and_then(|delta| per_second(delta, elapsed)),
            delta(Some(previous.write_bytes), Some(current.write_bytes))
                .and_then(|delta| per_second(delta, elapsed)),
        ),
        _ => (None, None),
    };

    ProcessRates {
        cpu_percent,
        read_bytes_per_second: read.map_or(Rate::NeedsAnotherSample, Rate::Ready),
        write_bytes_per_second: write.map_or(Rate::NeedsAnotherSample, Rate::Ready),
    }
}

/// Every process's rates for one refresh.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rates(BTreeMap<ProcessInstanceId, ProcessRates>);

impl Rates {
    /// One process's rates, or "waiting" for an instance this refresh did not
    /// produce counters for.
    pub fn get(&self, instance: &ProcessInstanceId) -> ProcessRates {
        self.0.get(instance).copied().unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: u128 = 1_000_000_000;

    fn counters(cpu_nanos: u64, read: u64, write: u64) -> ProcessCounters {
        ProcessCounters {
            cpu_time_nanos: Some(cpu_nanos),
            io: Some(RawProcessIo {
                read_bytes: read,
                write_bytes: write,
            }),
        }
    }

    fn snapshot(at: u128, entries: &[(ProcessInstanceId, ProcessCounters)]) -> CounterSnapshot {
        let mut snapshot = CounterSnapshot::new(at);
        for (instance, counters) in entries {
            snapshot.insert(*instance, *counters);
        }
        snapshot
    }

    // --- the CPU convention ----------------------------------------------

    #[test]
    fn one_saturated_logical_processor_is_one_thirty_second_of_a_32_thread_machine() {
        // A full second of CPU time over a full second of wall time on a
        // 32-thread machine: 100 / 32 = 3.125 %, not 100 %.
        let percent = cpu_percent(SECOND as u64, SECOND, 32).expect("measurable");
        assert!((percent - 3.125).abs() < 1e-9, "got {percent}");
    }

    #[test]
    fn saturating_every_logical_processor_is_one_hundred_percent() {
        let percent = cpu_percent((32 * SECOND) as u64, SECOND, 32).expect("measurable");
        assert!((percent - 100.0).abs() < 1e-9, "got {percent}");
    }

    #[test]
    fn the_convention_is_independent_of_the_interval_length() {
        // Same workload measured over 250 ms and over 8 s: same percentage.
        let short = cpu_percent((SECOND / 4) as u64, SECOND / 4, 8).expect("measurable");
        let long = cpu_percent((8 * SECOND) as u64, 8 * SECOND, 8).expect("measurable");
        assert!((short - long).abs() < 1e-9);
        assert!((short - 12.5).abs() < 1e-9, "got {short}");
    }

    #[test]
    fn an_idle_process_reports_a_true_zero() {
        assert_eq!(cpu_percent(0, SECOND, 16), Some(0.0));
        assert_eq!(per_second(0, SECOND), Some(0.0));
    }

    #[test]
    fn an_absurd_counter_is_clamped_to_the_machines_capacity() {
        // A suspended-then-resumed process, or a counter that jumped: a
        // 4 000 % row would be worse than a clamped one.
        let percent = cpu_percent(u64::MAX, SECOND, 8).expect("measurable");
        assert_eq!(percent, 100.0);
    }

    #[test]
    fn no_elapsed_time_and_no_processors_yield_no_measurement() {
        assert_eq!(cpu_percent(SECOND as u64, 0, 8), None);
        assert_eq!(cpu_percent(SECOND as u64, SECOND, 0), None);
        assert_eq!(per_second(1024, 0), None);
    }

    #[test]
    fn rates_are_never_nan_infinite_or_negative() {
        for (delta, elapsed, logical) in [
            (0_u64, 0_u128, 0_u32),
            (u64::MAX, 1, 1),
            (u64::MAX, u128::MAX, u32::MAX),
            (0, u128::MAX, 1),
        ] {
            if let Some(percent) = cpu_percent(delta, elapsed, logical) {
                assert!(percent.is_finite() && (0.0..=100.0).contains(&percent));
            }
            if let Some(rate) = per_second(delta, elapsed) {
                assert!(rate.is_finite() && rate >= 0.0);
            }
        }
    }

    #[test]
    fn bytes_per_second_divides_by_the_real_interval() {
        assert_eq!(per_second(4096, SECOND), Some(4096.0));
        assert_eq!(per_second(4096, SECOND / 2), Some(8192.0));
        assert_eq!(per_second(4096, 4 * SECOND), Some(1024.0));
    }

    // --- the tracker ------------------------------------------------------

    #[test]
    fn the_first_snapshot_waits_rather_than_reporting_zero() {
        let tracker = ProcessRateTracker::new();
        let pid = ProcessInstanceId::new(1234, 7);

        let rates = tracker.update(&snapshot(SECOND, &[(pid, counters(0, 0, 0))]), Some(8));

        assert_eq!(rates.get(&pid).cpu_percent, Rate::NeedsAnotherSample);
        assert_eq!(
            rates.get(&pid).read_bytes_per_second,
            Rate::NeedsAnotherSample
        );
        assert_eq!(
            rates.get(&pid).write_bytes_per_second,
            Rate::NeedsAnotherSample
        );
    }

    #[test]
    fn the_second_snapshot_produces_real_rates() {
        let tracker = ProcessRateTracker::new();
        let pid = ProcessInstanceId::new(1234, 7);

        tracker.update(&snapshot(SECOND, &[(pid, counters(0, 0, 0))]), Some(8));
        let rates = tracker.update(
            &snapshot(2 * SECOND, &[(pid, counters(SECOND as u64, 8192, 4096))]),
            Some(8),
        );

        let resolved = rates.get(&pid);
        assert_eq!(resolved.cpu_percent, Rate::Ready(12.5));
        assert_eq!(resolved.read_bytes_per_second, Rate::Ready(8192.0));
        assert_eq!(resolved.write_bytes_per_second, Rate::Ready(4096.0));
    }

    #[test]
    fn after_a_baseline_genuine_inactivity_is_reported_as_zero() {
        let tracker = ProcessRateTracker::new();
        let pid = ProcessInstanceId::new(1234, 7);

        tracker.update(&snapshot(SECOND, &[(pid, counters(500, 10, 20))]), Some(8));
        let rates = tracker.update(
            &snapshot(2 * SECOND, &[(pid, counters(500, 10, 20))]),
            Some(8),
        );

        let resolved = rates.get(&pid);
        assert_eq!(resolved.cpu_percent, Rate::Ready(0.0));
        assert_eq!(resolved.read_bytes_per_second, Rate::Ready(0.0));
        assert_eq!(resolved.write_bytes_per_second, Rate::Ready(0.0));
    }

    #[test]
    fn a_reused_pid_never_inherits_its_predecessors_baseline() {
        // The scenario this whole identity scheme exists for: PID 1234 was
        // firefox with 812 s of CPU time; it is now a freshly started cargo.
        let tracker = ProcessRateTracker::new();
        let firefox = ProcessInstanceId::new(1234, 100);
        let cargo = ProcessInstanceId::new(1234, 200);

        tracker.update(
            &snapshot(
                SECOND,
                &[(firefox, counters(812 * SECOND as u64, 5_000_000, 900_000))],
            ),
            Some(8),
        );

        let rates = tracker.update(
            &snapshot(2 * SECOND, &[(cargo, counters(200_000_000, 4_096, 0))]),
            Some(8),
        );

        let resolved = rates.get(&cargo);
        assert_eq!(
            resolved.cpu_percent,
            Rate::NeedsAnotherSample,
            "the new incarnation must start from no baseline"
        );
        assert_eq!(
            resolved.read_bytes_per_second,
            Rate::NeedsAnotherSample,
            "and must not inherit an I/O baseline either"
        );
        assert_eq!(tracker.tracked(), 1, "firefox's baseline must be dropped");
    }

    #[test]
    fn a_counter_that_moved_backwards_waits_instead_of_spiking_or_zeroing() {
        let tracker = ProcessRateTracker::new();
        let pid = ProcessInstanceId::new(1234, 7);

        tracker.update(
            &snapshot(SECOND, &[(pid, counters(9 * SECOND as u64, 1_000, 2_000))]),
            Some(8),
        );
        let rates = tracker.update(&snapshot(2 * SECOND, &[(pid, counters(3, 1, 2))]), Some(8));

        let resolved = rates.get(&pid);
        assert_eq!(resolved.cpu_percent, Rate::NeedsAnotherSample);
        assert_eq!(resolved.read_bytes_per_second, Rate::NeedsAnotherSample);

        // And the next interval recovers from the new baseline.
        let rates = tracker.update(
            &snapshot(3 * SECOND, &[(pid, counters(SECOND as u64 + 3, 4_097, 2))]),
            Some(8),
        );
        assert_eq!(rates.get(&pid).cpu_percent, Rate::Ready(12.5));
        assert_eq!(rates.get(&pid).read_bytes_per_second, Rate::Ready(4096.0));
    }

    #[test]
    fn two_refreshes_in_the_same_instant_wait_rather_than_divide_by_zero() {
        let tracker = ProcessRateTracker::new();
        let pid = ProcessInstanceId::new(1234, 7);

        tracker.update(&snapshot(SECOND, &[(pid, counters(0, 0, 0))]), Some(8));
        let rates = tracker.update(
            &snapshot(SECOND, &[(pid, counters(SECOND as u64, 10, 10))]),
            Some(8),
        );

        assert_eq!(rates.get(&pid).cpu_percent, Rate::NeedsAnotherSample);
    }

    #[test]
    fn a_vanished_process_is_pruned_and_costs_nothing_else() {
        let tracker = ProcessRateTracker::new();
        let short_lived = ProcessInstanceId::new(999, 1);
        let survivor = ProcessInstanceId::new(1000, 2);

        tracker.update(
            &snapshot(
                SECOND,
                &[
                    (short_lived, counters(0, 0, 0)),
                    (survivor, counters(0, 0, 0)),
                ],
            ),
            Some(8),
        );
        assert_eq!(tracker.tracked(), 2);

        let rates = tracker.update(
            &snapshot(2 * SECOND, &[(survivor, counters(SECOND as u64, 0, 0))]),
            Some(8),
        );

        assert_eq!(tracker.tracked(), 1);
        assert_eq!(rates.get(&survivor).cpu_percent, Rate::Ready(12.5));
        assert_eq!(
            rates.get(&short_lived).cpu_percent,
            Rate::NeedsAnotherSample,
            "a process that is gone has no rate, and asking for it must not panic"
        );
    }

    #[test]
    fn an_unknown_processor_count_leaves_cpu_unmeasured_but_keeps_io() {
        let tracker = ProcessRateTracker::new();
        let pid = ProcessInstanceId::new(1234, 7);

        tracker.update(&snapshot(SECOND, &[(pid, counters(0, 0, 0))]), None);
        let rates = tracker.update(
            &snapshot(2 * SECOND, &[(pid, counters(SECOND as u64, 2048, 0))]),
            None,
        );

        assert_eq!(rates.get(&pid).cpu_percent, Rate::NeedsAnotherSample);
        assert_eq!(rates.get(&pid).read_bytes_per_second, Rate::Ready(2048.0));
    }

    #[test]
    fn a_process_without_io_counters_still_reports_cpu() {
        let tracker = ProcessRateTracker::new();
        let pid = ProcessInstanceId::new(1234, 7);
        let without_io = ProcessCounters {
            cpu_time_nanos: Some(0),
            io: None,
        };

        tracker.update(&snapshot(SECOND, &[(pid, without_io)]), Some(8));
        let rates = tracker.update(
            &snapshot(
                2 * SECOND,
                &[(
                    pid,
                    ProcessCounters {
                        cpu_time_nanos: Some(SECOND as u64),
                        io: None,
                    },
                )],
            ),
            Some(8),
        );

        assert_eq!(rates.get(&pid).cpu_percent, Rate::Ready(12.5));
        assert_eq!(
            rates.get(&pid).read_bytes_per_second,
            Rate::NeedsAnotherSample
        );
    }

    #[test]
    fn priming_gives_the_first_refresh_a_real_interval() {
        let tracker = ProcessRateTracker::new();
        let pid = ProcessInstanceId::new(1234, 7);

        tracker.prime(&snapshot(0, &[(pid, counters(0, 0, 0))]));
        let rates = tracker.update(
            &snapshot(SECOND, &[(pid, counters(SECOND as u64, 0, 0))]),
            Some(8),
        );

        assert_eq!(rates.get(&pid).cpu_percent, Rate::Ready(12.5));
    }

    #[test]
    fn a_huge_counter_over_a_huge_interval_stays_finite() {
        let tracker = ProcessRateTracker::new();
        let pid = ProcessInstanceId::new(1234, 7);

        tracker.update(&snapshot(0, &[(pid, counters(0, 0, 0))]), Some(1));
        let rates = tracker.update(
            &snapshot(u128::MAX, &[(pid, counters(u64::MAX, u64::MAX, u64::MAX))]),
            Some(1),
        );

        let resolved = rates.get(&pid);
        for rate in [
            resolved.cpu_percent,
            resolved.read_bytes_per_second,
            resolved.write_bytes_per_second,
        ] {
            if let Rate::Ready(value) = rate {
                assert!(value.is_finite() && value >= 0.0, "got {value}");
            }
        }
    }

    #[test]
    fn hundreds_of_processes_are_tracked_without_bookkeeping_growth() {
        let tracker = ProcessRateTracker::new();
        let entries: Vec<_> = (1..=600)
            .map(|pid| {
                (
                    ProcessInstanceId::new(pid, u64::from(pid)),
                    counters(0, 0, 0),
                )
            })
            .collect();

        tracker.update(&snapshot(SECOND, &entries), Some(16));
        assert_eq!(tracker.tracked(), 600);

        // A churn of entirely new incarnations must not accumulate.
        let replacements: Vec<_> = (1..=600)
            .map(|pid| {
                (
                    ProcessInstanceId::new(pid, u64::from(pid) + 10_000),
                    counters(0, 0, 0),
                )
            })
            .collect();
        tracker.update(&snapshot(2 * SECOND, &replacements), Some(16));
        assert_eq!(tracker.tracked(), 600);
    }

    #[test]
    fn a_rate_exposes_its_value_only_when_it_has_one() {
        assert_eq!(Rate::Ready(3.5).value(), Some(3.5));
        assert_eq!(Rate::NeedsAnotherSample.value(), None);
    }
}
