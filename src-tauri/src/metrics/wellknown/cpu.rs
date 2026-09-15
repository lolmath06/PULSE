//! The `cpu.usage.total` metric: declaration and delta arithmetic.
//!
//! CPU usage is a **rate**, not a reading. Both Linux and Windows expose
//! monotonic counters of time spent busy and idle, so a single absolute read
//! says nothing — usage only exists between two samples.
//!
//! The platform-specific part is small: extract "busy" and "total" tick counts
//! from the native counters. Everything after that — the delta, the division,
//! the clamping and the baseline state machine — lives here and is shared, so
//! it is written once and tested once for both operating systems.

use std::sync::Mutex;

use crate::metrics::model::{
    MetricCategory, MetricDefinition, MetricDefinitionBuilder, MetricError, MetricErrorCode,
    MetricKey, MetricKind, MetricRef, MetricUnit, ProviderId, SourceId,
};

/// `cpu.usage.total` — aggregate CPU utilisation across all cores.
pub const USAGE_TOTAL: &str = "cpu.usage.total";

/// The logical CPU source.
///
/// `cpu:system` is a *logical* identifier for "this machine's CPU as a whole",
/// not a device path. It is therefore stable by construction and identical on
/// every platform. Per-core and per-package sources will come later with their
/// own, hardware-derived identifiers.
pub const SOURCE: &str = "cpu:system";

/// Builds the `cpu.usage.total` reference.
pub fn usage_total_ref() -> MetricRef {
    MetricRef::new(
        MetricKey::new(USAGE_TOTAL).expect("well-known CPU key must be valid"),
        SourceId::new(SOURCE).expect("well-known CPU source must be valid"),
    )
}

/// Declares every CPU metric PULSE ships, attributed to `provider`.
///
/// Both the Linux and Windows CPU providers call this, so their declarations
/// are identical apart from `providerId`. A test asserts exactly that.
pub fn definitions(provider: &ProviderId) -> Vec<MetricDefinition> {
    vec![MetricDefinitionBuilder::new(
        usage_total_ref(),
        provider.clone(),
        MetricCategory::Cpu,
        MetricUnit::Percent,
        MetricKind::Gauge,
    )
    .display_name("CPU usage")
    .source_label("System CPU")
    .description(
        "Share of CPU time spent doing work, averaged over all cores since the previous sample.",
    )
    .build()]
}

/// A snapshot of the host's cumulative CPU time counters.
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

/// What a sampling attempt produced.
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
    /// Nothing to compare against — the first reading of this provider.
    NoBaseline,
    /// The counters did not advance between the two reads.
    NoElapsedTime,
    /// The counters went backwards, so the previous baseline is meaningless.
    /// Happens across suspend/resume and on some virtualised clocks.
    CountersWentBackwards,
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
        }
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

/// Holds the previous CPU counters so usage can be derived without blocking.
///
/// PULSE has no sampler thread yet, and a `sleep(100ms)` inside a command
/// would freeze the UI thread for every request. Instead the provider captures
/// a baseline when it is built, and each request compares against the previous
/// one. The very first request after startup may therefore report
/// [`CpuUsage::NeedsAnotherSample`] — which the UI shows honestly rather than
/// as 0%.
#[derive(Debug, Default)]
pub struct CpuUsageTracker {
    previous: Mutex<Option<CpuCounters>>,
}

impl CpuUsageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores an initial baseline, discarding any previous one.
    ///
    /// Errors are swallowed on purpose: failing to prime is not fatal, it just
    /// means the first sample needs a second request.
    pub fn prime(&self, counters: CpuCounters) {
        if let Ok(mut guard) = self.previous.lock() {
            *guard = Some(counters);
        }
    }

    /// Computes usage against the stored baseline and becomes the new baseline.
    ///
    /// Never panics. A poisoned mutex — only possible if another thread
    /// panicked while holding it — is reported as a structured internal error
    /// rather than unwrapped.
    pub fn update(&self, current: CpuCounters) -> Result<CpuUsage, MetricError> {
        if !current.is_consistent() {
            return Err(MetricError::new(
                MetricErrorCode::Parse,
                format!(
                    "inconsistent CPU counters: busy ({}) exceeds total ({})",
                    current.busy, current.total
                ),
            )
            .with_recoverable(true));
        }

        let mut guard = self.previous.lock().map_err(|_| {
            MetricError::internal("CPU baseline lock was poisoned by a panicking thread")
        })?;

        let outcome = match *guard {
            None => CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoBaseline),
            Some(previous) => {
                if current.total < previous.total || current.busy < previous.busy {
                    CpuUsage::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards)
                } else {
                    let total_delta = current.total - previous.total;
                    let busy_delta = current.busy - previous.busy;

                    match usage_percent(busy_delta, total_delta) {
                        Some(percent) => CpuUsage::Ready(percent),
                        None => CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoElapsedTime),
                    }
                }
            }
        };

        // Always advance the baseline, including on the failure paths: that is
        // what makes the *next* request succeed.
        *guard = Some(current);

        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_well_known_reference_is_valid_and_stable() {
        let reference = usage_total_ref();

        assert_eq!(reference.key.as_str(), "cpu.usage.total");
        assert_eq!(reference.source_id.as_str(), "cpu:system");
        assert_eq!(reference.to_string(), "cpu.usage.total@cpu:system");
    }

    #[test]
    fn definitions_describe_a_percent_gauge() {
        let provider = ProviderId::new("linux.cpu").expect("valid");
        let definitions = definitions(&provider);

        assert_eq!(definitions.len(), 1);
        let definition = &definitions[0];
        assert_eq!(definition.unit, MetricUnit::Percent);
        assert_eq!(definition.kind, MetricKind::Gauge);
        assert_eq!(definition.category, MetricCategory::Cpu);
        assert_eq!(definition.provider_id, provider);
        // No OS name in user-facing text: the metric is conceptually identical.
        assert!(!definition.display_name.to_lowercase().contains("linux"));
        assert!(!definition.source_label.to_lowercase().contains("linux"));
    }

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

    #[test]
    fn the_first_sample_waits_instead_of_reporting_zero_percent() {
        let tracker = CpuUsageTracker::new();

        let outcome = tracker
            .update(CpuCounters::new(100, 400))
            .expect("no error");

        assert_eq!(
            outcome,
            CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoBaseline)
        );
    }

    #[test]
    fn the_second_sample_produces_usage() {
        let tracker = CpuUsageTracker::new();

        tracker.prime(CpuCounters::new(100, 400));
        let outcome = tracker
            .update(CpuCounters::new(150, 500))
            .expect("no error");

        // 50 busy ticks out of 100 elapsed ticks.
        assert_eq!(outcome, CpuUsage::Ready(50.0));
    }

    #[test]
    fn consecutive_samples_each_measure_their_own_interval() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(CpuCounters::new(0, 0));

        assert_eq!(
            tracker.update(CpuCounters::new(10, 100)).expect("ok"),
            CpuUsage::Ready(10.0)
        );
        // Second interval is busier; the first interval must not dilute it.
        assert_eq!(
            tracker.update(CpuCounters::new(100, 200)).expect("ok"),
            CpuUsage::Ready(90.0)
        );
    }

    #[test]
    fn identical_counters_wait_rather_than_report_idle() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(CpuCounters::new(100, 400));

        assert_eq!(
            tracker.update(CpuCounters::new(100, 400)).expect("ok"),
            CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoElapsedTime)
        );
    }

    #[test]
    fn counters_going_backwards_reset_the_baseline() {
        // Observed across suspend/resume and on some virtualised clocks.
        let tracker = CpuUsageTracker::new();
        tracker.prime(CpuCounters::new(500, 1000));

        assert_eq!(
            tracker.update(CpuCounters::new(100, 400)).expect("ok"),
            CpuUsage::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards)
        );

        // The rewound reading became the new baseline, so the next one works.
        assert_eq!(
            tracker.update(CpuCounters::new(150, 500)).expect("ok"),
            CpuUsage::Ready(50.0)
        );
    }

    #[test]
    fn busy_going_backwards_alone_is_also_caught() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(CpuCounters::new(500, 1000));

        assert_eq!(
            tracker.update(CpuCounters::new(400, 1100)).expect("ok"),
            CpuUsage::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards)
        );
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
            tracker.update(first).expect("first read");
            let second = CpuCounters::new(first.busy + 10, first.total + 100);

            assert!(matches!(
                tracker.update(second).expect("second read"),
                CpuUsage::Ready(_)
            ));
        }
    }

    #[test]
    fn inconsistent_counters_are_a_structured_error_not_a_bogus_percentage() {
        let tracker = CpuUsageTracker::new();

        let error = tracker
            .update(CpuCounters::new(500, 100))
            .expect_err("busy above total must be rejected");

        assert_eq!(error.code, MetricErrorCode::Parse);
        assert!(error.recoverable);
    }

    #[test]
    fn the_tracker_is_usable_from_several_threads() {
        use std::sync::Arc;

        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CpuUsageTracker>();

        let tracker = Arc::new(CpuUsageTracker::new());
        let handles: Vec<_> = (1..=8)
            .map(|i| {
                let tracker = Arc::clone(&tracker);
                std::thread::spawn(move || tracker.update(CpuCounters::new(i * 10, i * 100)))
            })
            .collect();

        for handle in handles {
            // Whatever the interleaving, no panic and no error.
            assert!(handle.join().expect("thread did not panic").is_ok());
        }
    }
}
