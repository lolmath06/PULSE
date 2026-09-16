//! CPU usage: counters, delta arithmetic and the multi-processor baseline.
//!
//! CPU usage is a **rate**, not a reading. Both Linux and Windows expose
//! monotonic counters of time spent busy and idle, so a single absolute read
//! says nothing — usage only exists between two samples.
//!
//! The platform-specific part is small: extract "busy" and "total" tick counts
//! for the machine and for each logical processor. Everything after that — the
//! delta, the division, the clamping and the baseline state machine — lives
//! here and is shared, so it is written once and tested once for both
//! operating systems.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::metrics::model::MetricError;

use super::topology::LogicalId;

/// A snapshot of cumulative CPU time counters.
///
/// Units are whatever the OS uses (jiffies on Linux, 100 ns intervals on
/// Windows) — only the ratio matters, so no conversion is needed. `busy` must
/// be a subset of `total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuCounters {
    /// Cumulative time spent doing work.
    pub busy: u64,
    /// Cumulative time overall, busy and idle together.
    pub total: u64,
}

impl CpuCounters {
    pub const fn new(busy: u64, total: u64) -> Self {
        Self { busy, total }
    }

    /// Whether this snapshot is self-consistent.
    ///
    /// `busy > total` means the counters were misread; publishing a usage
    /// derived from them would produce a value above 100%.
    pub const fn is_consistent(&self) -> bool {
        self.busy <= self.total
    }
}

/// One read of every CPU counter the platform could obtain.
///
/// Built from a **single** system read: on Linux one `/proc/stat` read yields
/// the aggregate line and every `cpuN` line at once, and on Windows one call
/// per processor group does the same. Sampling the aggregate and the
/// per-processor figures at different instants would make them disagree.
///
/// Both halves are optional and independent, which is what lets
/// `cpu.usage.total` keep working when the per-processor source fails, and
/// vice versa.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CpuSnapshot {
    /// Aggregate counters for the machine, when available.
    pub total: Option<CpuCounters>,
    /// Counters per logical processor, keyed by ordinal.
    pub logical: BTreeMap<LogicalId, CpuCounters>,
}

impl CpuSnapshot {
    /// An empty snapshot.
    pub fn new() -> Self {
        Self::default()
    }

    /// A snapshot carrying only aggregate counters.
    pub fn total_only(total: CpuCounters) -> Self {
        Self {
            total: Some(total),
            logical: BTreeMap::new(),
        }
    }

    pub fn with_total(mut self, total: CpuCounters) -> Self {
        self.total = Some(total);
        self
    }

    /// Records one logical processor's counters.
    pub fn insert_logical(&mut self, id: LogicalId, counters: CpuCounters) {
        self.logical.insert(id, counters);
    }

    /// Whether this snapshot carries nothing at all.
    pub fn is_empty(&self) -> bool {
        self.total.is_none() && self.logical.is_empty()
    }
}

/// What a sampling attempt produced for one processor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CpuUsage {
    /// A usable percentage, already clamped to 0–100.
    Ready(f64),
    /// No usable delta yet. The baseline has been (re)stored, so the next
    /// request should succeed.
    ///
    /// This is deliberately **not** `0%`: reporting an unmeasured CPU as idle
    /// is a lie the user cannot distinguish from a genuinely idle machine.
    NeedsAnotherSample(NeedsAnotherSample),
}

/// Why no usage could be computed yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeedsAnotherSample {
    /// Nothing to compare against — the first reading, or a logical processor
    /// that has just appeared (CPU hotplug, or one brought back online).
    NoBaseline,
    /// The counters did not advance between the two reads. Common on an idle
    /// logical processor sampled twice in quick succession.
    NoElapsedTime,
    /// The counters went backwards, so the previous baseline is meaningless.
    /// Happens across suspend/resume and on some virtualised clocks.
    CountersWentBackwards,
    /// The counters contradicted themselves (`busy` above `total`). The
    /// reading is discarded rather than published as a percentage above 100.
    InconsistentCounters,
}

impl NeedsAnotherSample {
    /// A short explanation for the UI.
    pub const fn reason(self) -> &'static str {
        match self {
            NeedsAnotherSample::NoBaseline => {
                "CPU usage is measured between two samples; waiting for the next one"
            }
            NeedsAnotherSample::NoElapsedTime => {
                "no CPU time elapsed since the previous sample; waiting for the next one"
            }
            NeedsAnotherSample::CountersWentBackwards => {
                "CPU counters were reset (suspend/resume); baseline restarted"
            }
            NeedsAnotherSample::InconsistentCounters => {
                "CPU counters were inconsistent; baseline restarted"
            }
        }
    }
}

/// The usage computed for every processor in one snapshot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuUsageReport {
    /// Aggregate usage, absent when the snapshot carried no aggregate.
    pub total: Option<CpuUsage>,
    /// Usage per logical processor, keyed by ordinal.
    pub logical: BTreeMap<LogicalId, CpuUsage>,
}

impl CpuUsageReport {
    /// Usage for one logical processor, if the snapshot covered it.
    pub fn logical(&self, id: LogicalId) -> Option<CpuUsage> {
        self.logical.get(&id).copied()
    }
}

/// Converts a counter delta into a percentage.
///
/// Returns `None` when no time elapsed. The result is clamped to 0–100: a
/// counter that advances irregularly must never publish 103% to a gauge
/// widget.
pub fn usage_percent(busy_delta: u64, total_delta: u64) -> Option<f64> {
    if total_delta == 0 {
        return None;
    }

    let percent = (busy_delta as f64 / total_delta as f64) * 100.0;

    if !percent.is_finite() {
        return None;
    }

    Some(percent.clamp(0.0, 100.0))
}

/// Compares one processor's counters against its baseline.
///
/// Pure, so every branch of the state machine is testable without a lock or a
/// map. `previous` is `None` for a processor seen for the first time.
fn compare(previous: Option<CpuCounters>, current: CpuCounters) -> CpuUsage {
    if !current.is_consistent() {
        return CpuUsage::NeedsAnotherSample(NeedsAnotherSample::InconsistentCounters);
    }

    let Some(previous) = previous else {
        return CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoBaseline);
    };

    if current.total < previous.total || current.busy < previous.busy {
        return CpuUsage::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards);
    }

    match usage_percent(current.busy - previous.busy, current.total - previous.total) {
        Some(percent) => CpuUsage::Ready(percent),
        None => CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoElapsedTime),
    }
}

/// The baselines held between two requests.
#[derive(Debug, Default)]
struct Baselines {
    total: Option<CpuCounters>,
    logical: BTreeMap<LogicalId, CpuCounters>,
}

/// Holds the previous CPU counters so usage can be derived without blocking.
///
/// PULSE has no sampler thread yet, and a `sleep(100ms)` inside a command
/// would freeze the UI thread for every request. Instead the provider captures
/// a baseline when it is built, and each request compares against the previous
/// one. The very first request after startup may therefore report
/// [`CpuUsage::NeedsAnotherSample`] — which the UI shows honestly rather than
/// as 0%.
///
/// # One lock, not one per processor
///
/// The aggregate and every logical processor live behind a **single**
/// `Mutex<Baselines>`. A mutex per CPU would be 32 or 128 locks acquired in
/// sequence on every request, for state that is only ever written as one
/// consistent set derived from one system read — more machinery, more ways to
/// interleave, and no contention removed, since a request touches all of them
/// anyway.
///
/// # Machines whose CPU list changes
///
/// A logical processor that disappears between two reads (offlined, or
/// hot-unplugged) has its baseline dropped, so a CPU that comes back later
/// starts from `NoBaseline` rather than differencing against counters from
/// before it left. A processor that appears reports `NoBaseline` once and
/// works from the next request on.
#[derive(Debug, Default)]
pub struct CpuUsageTracker {
    baselines: Mutex<Baselines>,
}

impl CpuUsageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores an initial baseline, discarding any previous one.
    ///
    /// Errors are swallowed on purpose: failing to prime is not fatal, it just
    /// means the first sample needs a second request.
    pub fn prime(&self, snapshot: &CpuSnapshot) {
        if let Ok(mut guard) = self.baselines.lock() {
            if let Some(total) = snapshot.total {
                guard.total = Some(total);
            }
            guard.logical = snapshot.logical.clone();
        }
    }

    /// Computes usage against the stored baselines, which then advance.
    ///
    /// Never panics. A poisoned mutex — only possible if another thread
    /// panicked while holding it — is reported as a structured internal error
    /// rather than unwrapped.
    ///
    /// Every outcome, including the failure paths, leaves a usable baseline
    /// behind: that is what makes the *next* request succeed.
    pub fn update(&self, snapshot: &CpuSnapshot) -> Result<CpuUsageReport, MetricError> {
        let mut guard = self.baselines.lock().map_err(|_| {
            MetricError::internal("CPU baseline lock was poisoned by a panicking thread")
        })?;

        let total = snapshot.total.map(|current| {
            let outcome = compare(guard.total, current);
            guard.total = Some(current);
            outcome
        });

        let logical: BTreeMap<LogicalId, CpuUsage> = snapshot
            .logical
            .iter()
            .map(|(&id, &current)| (id, compare(guard.logical.get(&id).copied(), current)))
            .collect();

        // Replacing rather than merging is what drops the baselines of
        // processors that are no longer present.
        if !snapshot.logical.is_empty() {
            guard.logical = snapshot.logical.clone();
        }

        Ok(CpuUsageReport { total, logical })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logical(ordinal: u32) -> LogicalId {
        LogicalId::new(ordinal)
    }

    fn snapshot(total: CpuCounters, logical_counters: &[(u32, CpuCounters)]) -> CpuSnapshot {
        let mut snapshot = CpuSnapshot::total_only(total);
        for &(ordinal, counters) in logical_counters {
            snapshot.insert_logical(logical(ordinal), counters);
        }
        snapshot
    }

    // --- arithmetic -------------------------------------------------------

    #[test]
    fn usage_percent_computes_the_expected_ratio() {
        assert_eq!(usage_percent(25, 100), Some(25.0));
        assert_eq!(usage_percent(0, 100), Some(0.0));
        assert_eq!(usage_percent(100, 100), Some(100.0));
        assert_eq!(
            usage_percent(1, 3).map(|p| (p * 100.0).round()),
            Some(3333.0)
        );
    }

    #[test]
    fn usage_percent_refuses_a_zero_delta_rather_than_dividing() {
        assert_eq!(usage_percent(0, 0), None);
        assert_eq!(usage_percent(5, 0), None);
    }

    #[test]
    fn usage_percent_clamps_out_of_range_input() {
        // Should not happen with consistent counters, but a gauge must never
        // be handed 150%.
        assert_eq!(usage_percent(150, 100), Some(100.0));
    }

    #[test]
    fn counters_know_whether_they_are_consistent() {
        assert!(CpuCounters::new(30, 100).is_consistent());
        assert!(CpuCounters::new(100, 100).is_consistent());
        assert!(!CpuCounters::new(101, 100).is_consistent());
    }

    // --- the pure state machine ------------------------------------------

    #[test]
    fn every_branch_of_the_comparison_is_distinguishable() {
        let base = CpuCounters::new(100, 400);

        assert_eq!(
            compare(None, base),
            CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoBaseline)
        );
        assert_eq!(
            compare(Some(base), base),
            CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoElapsedTime)
        );
        assert_eq!(
            compare(Some(base), CpuCounters::new(50, 200)),
            CpuUsage::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards)
        );
        assert_eq!(
            compare(Some(base), CpuCounters::new(900, 500)),
            CpuUsage::NeedsAnotherSample(NeedsAnotherSample::InconsistentCounters)
        );
        assert_eq!(
            compare(Some(base), CpuCounters::new(150, 500)),
            CpuUsage::Ready(50.0)
        );
    }

    #[test]
    fn busy_going_backwards_alone_is_also_caught() {
        assert_eq!(
            compare(
                Some(CpuCounters::new(500, 1000)),
                CpuCounters::new(400, 1100)
            ),
            CpuUsage::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards)
        );
    }

    #[test]
    fn each_reason_carries_its_own_explanation() {
        let reasons = [
            NeedsAnotherSample::NoBaseline,
            NeedsAnotherSample::NoElapsedTime,
            NeedsAnotherSample::CountersWentBackwards,
            NeedsAnotherSample::InconsistentCounters,
        ];

        let mut texts: Vec<&str> = reasons.iter().map(|r| r.reason()).collect();
        texts.sort_unstable();
        texts.dedup();

        assert_eq!(texts.len(), reasons.len(), "reasons must stay distinct");
        assert!(texts.iter().all(|text| !text.is_empty()));
    }

    // --- aggregate behaviour, unchanged from Phase 2 ----------------------

    #[test]
    fn the_first_sample_waits_instead_of_reporting_zero_percent() {
        let tracker = CpuUsageTracker::new();

        let report = tracker
            .update(&CpuSnapshot::total_only(CpuCounters::new(100, 400)))
            .expect("no error");

        assert_eq!(
            report.total,
            Some(CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoBaseline))
        );
    }

    #[test]
    fn the_second_sample_produces_usage() {
        let tracker = CpuUsageTracker::new();

        tracker.prime(&CpuSnapshot::total_only(CpuCounters::new(100, 400)));
        let report = tracker
            .update(&CpuSnapshot::total_only(CpuCounters::new(150, 500)))
            .expect("no error");

        // 50 busy ticks out of 100 elapsed ticks.
        assert_eq!(report.total, Some(CpuUsage::Ready(50.0)));
    }

    #[test]
    fn consecutive_samples_each_measure_their_own_interval() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(&CpuSnapshot::total_only(CpuCounters::new(0, 0)));

        assert_eq!(
            tracker
                .update(&CpuSnapshot::total_only(CpuCounters::new(10, 100)))
                .expect("ok")
                .total,
            Some(CpuUsage::Ready(10.0))
        );
        // Second interval is busier; the first interval must not dilute it.
        assert_eq!(
            tracker
                .update(&CpuSnapshot::total_only(CpuCounters::new(100, 200)))
                .expect("ok")
                .total,
            Some(CpuUsage::Ready(90.0))
        );
    }

    #[test]
    fn a_snapshot_without_an_aggregate_leaves_the_aggregate_baseline_alone() {
        // Windows reads the aggregate and the per-processor figures through
        // two different APIs; one failing must not corrupt the other.
        let tracker = CpuUsageTracker::new();
        tracker.prime(&CpuSnapshot::total_only(CpuCounters::new(100, 400)));

        let mut per_logical_only = CpuSnapshot::new();
        per_logical_only.insert_logical(logical(0), CpuCounters::new(10, 40));
        let report = tracker.update(&per_logical_only).expect("ok");
        assert_eq!(report.total, None);

        // The aggregate baseline survived, so it still measures the full
        // interval since it was primed.
        let report = tracker
            .update(&CpuSnapshot::total_only(CpuCounters::new(150, 500)))
            .expect("ok");
        assert_eq!(report.total, Some(CpuUsage::Ready(50.0)));
    }

    // --- per-logical behaviour -------------------------------------------

    #[test]
    fn every_logical_processor_gets_its_own_baseline() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(&snapshot(
            CpuCounters::new(0, 0),
            &[
                (0, CpuCounters::new(0, 0)),
                (1, CpuCounters::new(0, 0)),
                (2, CpuCounters::new(0, 0)),
            ],
        ));

        let report = tracker
            .update(&snapshot(
                CpuCounters::new(110, 300),
                &[
                    (0, CpuCounters::new(100, 100)),
                    (1, CpuCounters::new(10, 100)),
                    (2, CpuCounters::new(0, 100)),
                ],
            ))
            .expect("ok");

        // Each processor measured independently, not smeared into the average.
        assert_eq!(report.logical(logical(0)), Some(CpuUsage::Ready(100.0)));
        assert_eq!(report.logical(logical(1)), Some(CpuUsage::Ready(10.0)));
        assert_eq!(report.logical(logical(2)), Some(CpuUsage::Ready(0.0)));
        // 110 busy of 300 elapsed.
        assert!(matches!(report.total, Some(CpuUsage::Ready(_))));
    }

    #[test]
    fn the_aggregate_and_the_per_processor_figures_are_independent() {
        // A busy machine whose work sits on one processor: the aggregate is
        // low, one processor is pinned. Both must be reported as measured.
        let tracker = CpuUsageTracker::new();
        tracker.prime(&snapshot(
            CpuCounters::new(0, 0),
            &[(0, CpuCounters::new(0, 0)), (1, CpuCounters::new(0, 0))],
        ));

        let report = tracker
            .update(&snapshot(
                CpuCounters::new(100, 200),
                &[
                    (0, CpuCounters::new(100, 100)),
                    (1, CpuCounters::new(0, 100)),
                ],
            ))
            .expect("ok");

        assert_eq!(report.total, Some(CpuUsage::Ready(50.0)));
        assert_eq!(report.logical(logical(0)), Some(CpuUsage::Ready(100.0)));
        assert_eq!(report.logical(logical(1)), Some(CpuUsage::Ready(0.0)));
    }

    #[test]
    fn non_contiguous_ordinals_are_tracked_correctly() {
        let tracker = CpuUsageTracker::new();
        let ids = [0_u32, 1, 2, 3, 8, 9, 10, 11];

        tracker.prime(&snapshot(
            CpuCounters::new(0, 0),
            &ids.map(|id| (id, CpuCounters::new(0, 0))),
        ));

        let report = tracker
            .update(&snapshot(
                CpuCounters::new(0, 0),
                &ids.map(|id| (id, CpuCounters::new(u64::from(id), 100))),
            ))
            .expect("ok");

        for id in ids {
            assert_eq!(
                report.logical(logical(id)),
                Some(CpuUsage::Ready(f64::from(id))),
                "ordinal {id} was mis-tracked"
            );
        }
        assert!(report.logical(logical(4)).is_none());
    }

    #[test]
    fn a_newly_appeared_processor_waits_rather_than_reporting_zero() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(&snapshot(
            CpuCounters::new(0, 0),
            &[(0, CpuCounters::new(0, 0))],
        ));

        // cpu1 was brought online between the two reads.
        let report = tracker
            .update(&snapshot(
                CpuCounters::new(20, 200),
                &[
                    (0, CpuCounters::new(10, 100)),
                    (1, CpuCounters::new(10, 100)),
                ],
            ))
            .expect("ok");

        assert_eq!(report.logical(logical(0)), Some(CpuUsage::Ready(10.0)));
        assert_eq!(
            report.logical(logical(1)),
            Some(CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoBaseline)),
            "a CPU seen for the first time has nothing to difference against"
        );

        // From the next request on it measures normally.
        let report = tracker
            .update(&snapshot(
                CpuCounters::new(40, 400),
                &[
                    (0, CpuCounters::new(20, 200)),
                    (1, CpuCounters::new(60, 200)),
                ],
            ))
            .expect("ok");
        assert_eq!(report.logical(logical(1)), Some(CpuUsage::Ready(50.0)));
    }

    #[test]
    fn a_processor_that_disappears_loses_its_stale_baseline() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(&snapshot(
            CpuCounters::new(0, 0),
            &[
                (0, CpuCounters::new(0, 0)),
                (1, CpuCounters::new(1_000, 2_000)),
            ],
        ));

        // cpu1 goes offline: it is simply absent from the report.
        let report = tracker
            .update(&snapshot(
                CpuCounters::new(10, 100),
                &[(0, CpuCounters::new(10, 100))],
            ))
            .expect("ok");
        assert!(report.logical(logical(1)).is_none());

        // It comes back, with counters reset by the offline/online cycle.
        // Differencing against the pre-offline baseline would have produced a
        // bogus figure; instead it starts cleanly.
        let report = tracker
            .update(&snapshot(
                CpuCounters::new(20, 200),
                &[(0, CpuCounters::new(20, 200)), (1, CpuCounters::new(5, 50))],
            ))
            .expect("ok");
        assert_eq!(
            report.logical(logical(1)),
            Some(CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoBaseline))
        );
    }

    #[test]
    fn an_idle_processor_waits_rather_than_reporting_idle() {
        let tracker = CpuUsageTracker::new();
        let counters = CpuCounters::new(500, 1_000);
        tracker.prime(&snapshot(
            CpuCounters::new(0, 0),
            &[(0, counters), (1, counters)],
        ));

        let report = tracker
            .update(&snapshot(
                CpuCounters::new(10, 100),
                &[(0, CpuCounters::new(550, 1_100)), (1, counters)],
            ))
            .expect("ok");

        assert_eq!(report.logical(logical(0)), Some(CpuUsage::Ready(50.0)));
        assert_eq!(
            report.logical(logical(1)),
            Some(CpuUsage::NeedsAnotherSample(
                NeedsAnotherSample::NoElapsedTime
            )),
            "an unmeasurable processor is never reported as 0%"
        );
    }

    #[test]
    fn one_processors_counters_rewinding_does_not_affect_the_others() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(&snapshot(
            CpuCounters::new(0, 0),
            &[
                (0, CpuCounters::new(0, 0)),
                (1, CpuCounters::new(5_000, 10_000)),
            ],
        ));

        let report = tracker
            .update(&snapshot(
                CpuCounters::new(10, 100),
                &[
                    (0, CpuCounters::new(25, 100)),
                    (1, CpuCounters::new(10, 100)),
                ],
            ))
            .expect("ok");

        assert_eq!(report.logical(logical(0)), Some(CpuUsage::Ready(25.0)));
        assert_eq!(
            report.logical(logical(1)),
            Some(CpuUsage::NeedsAnotherSample(
                NeedsAnotherSample::CountersWentBackwards
            ))
        );

        // The rewound reading became the new baseline for that processor only.
        let report = tracker
            .update(&snapshot(
                CpuCounters::new(20, 200),
                &[
                    (0, CpuCounters::new(50, 200)),
                    (1, CpuCounters::new(60, 200)),
                ],
            ))
            .expect("ok");
        assert_eq!(report.logical(logical(1)), Some(CpuUsage::Ready(50.0)));
    }

    #[test]
    fn inconsistent_counters_are_discarded_not_published_as_over_one_hundred() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(&snapshot(
            CpuCounters::new(0, 0),
            &[(0, CpuCounters::new(0, 0))],
        ));

        let report = tracker
            .update(&snapshot(
                CpuCounters::new(50, 100),
                &[(0, CpuCounters::new(500, 100))],
            ))
            .expect("ok");

        assert_eq!(
            report.logical(logical(0)),
            Some(CpuUsage::NeedsAnotherSample(
                NeedsAnotherSample::InconsistentCounters
            ))
        );
        // And the rest of the machine is unaffected.
        assert!(matches!(report.total, Some(CpuUsage::Ready(_))));
    }

    #[test]
    fn every_failure_path_stores_a_usable_baseline() {
        // The promise behind "Refresh makes it work": each outcome leaves the
        // tracker able to answer next time.
        for first in [
            CpuCounters::new(100, 400),
            CpuCounters::new(0, 0),
            CpuCounters::new(999, 999),
        ] {
            let tracker = CpuUsageTracker::new();
            tracker
                .update(&snapshot(first, &[(0, first)]))
                .expect("first read");

            let second = CpuCounters::new(first.busy + 10, first.total + 100);
            let report = tracker
                .update(&snapshot(second, &[(0, second)]))
                .expect("second read");

            assert!(matches!(report.total, Some(CpuUsage::Ready(_))));
            assert!(matches!(
                report.logical(logical(0)),
                Some(CpuUsage::Ready(_))
            ));
        }
    }

    #[test]
    fn an_empty_snapshot_reports_nothing_and_breaks_nothing() {
        let tracker = CpuUsageTracker::new();
        let report = tracker.update(&CpuSnapshot::new()).expect("ok");

        assert_eq!(report.total, None);
        assert!(report.logical.is_empty());
    }

    #[test]
    fn a_report_is_ordered_numerically_by_ordinal() {
        // BTreeMap over LogicalId, so iteration order is 2 before 10.
        let tracker = CpuUsageTracker::new();
        let counters = CpuCounters::new(0, 0);
        let mut snapshot = CpuSnapshot::new();
        for ordinal in [10, 2, 1, 11, 0] {
            snapshot.insert_logical(logical(ordinal), counters);
        }

        let report = tracker.update(&snapshot).expect("ok");
        let ordinals: Vec<u32> = report.logical.keys().map(|id| id.get()).collect();

        assert_eq!(ordinals, [0, 1, 2, 10, 11]);
    }

    #[test]
    fn the_tracker_is_usable_from_several_threads() {
        use std::sync::Arc;

        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CpuUsageTracker>();

        let tracker = Arc::new(CpuUsageTracker::new());
        let handles: Vec<_> = (1..=8_u64)
            .map(|i| {
                let tracker = Arc::clone(&tracker);
                std::thread::spawn(move || {
                    let counters = CpuCounters::new(i * 10, i * 100);
                    tracker.update(&snapshot(counters, &[(0, counters), (1, counters)]))
                })
            })
            .collect();

        for handle in handles {
            // Whatever the interleaving, no panic and no error.
            assert!(handle.join().expect("thread did not panic").is_ok());
        }
    }
}
