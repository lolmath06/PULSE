//! One tick of history, and when the next one is due.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::clock::Clock;
use super::error::HistoryError;
use super::store::{BatchReceipt, HistoryStore};
use crate::metrics::model::{MetricRef, MetricSample, MetricValue};
use crate::metrics::MetricsEngine;

/// Anything that can sample metrics — the engine in production, a fake in
/// tests.
pub trait SampleSource: Send + Sync {
    fn sample(&self, metrics: &[MetricRef]) -> Vec<MetricSample>;
}

impl SampleSource for MetricsEngine {
    fn sample(&self, metrics: &[MetricRef]) -> Vec<MetricSample> {
        MetricsEngine::sample(self, metrics)
    }
}

/// What one tick did and how long it took.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickReport {
    pub receipt: BatchReceipt,
    /// Samples that came back unavailable and therefore wrote nothing.
    pub unavailable: u32,
    pub sample_duration: Duration,
    pub insert_duration: Duration,
}

/// Samples the historized metrics once and writes them as one batch.
///
/// **Unavailable is not zero.** A metric whose sample is unavailable — GPU
/// telemetry missing, a sensor gone, a rate without its first interval — writes
/// **no row**, so the chart shows a gap rather than a fabricated `0`.
pub struct HistorySampler {
    metrics: Vec<MetricRef>,
    source: Arc<dyn SampleSource>,
    store: Arc<HistoryStore>,
    clock: Arc<dyn Clock>,
}

impl HistorySampler {
    pub fn new(
        metrics: Vec<MetricRef>,
        source: Arc<dyn SampleSource>,
        store: Arc<HistoryStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            metrics,
            source,
            store,
            clock,
        }
    }

    pub fn metrics(&self) -> &[MetricRef] {
        &self.metrics
    }

    pub fn tick(&self) -> Result<TickReport, HistoryError> {
        let began = Instant::now();
        let samples = self.source.sample(&self.metrics);
        let timestamp_ms = self.clock.now_ms();
        let sample_duration = began.elapsed();

        let mut unavailable = 0_u32;
        let values: Vec<(MetricRef, f64)> = samples
            .into_iter()
            .filter_map(
                |sample| match (&sample.value, sample.availability.is_available()) {
                    (Some(MetricValue::Number(value)), true) => Some((sample.metric, *value)),
                    _ => {
                        unavailable += 1;
                        None
                    }
                },
            )
            .collect();

        let began = Instant::now();
        let receipt = self.store.insert_batch(timestamp_ms, &values)?;
        let insert_duration = began.elapsed();

        Ok(TickReport {
            receipt,
            unavailable,
            sample_duration,
            insert_duration,
        })
    }
}

/// When the next tick is due, on the **monotonic** clock.
///
/// Fixed-rate while on time, so the cadence does not drift by the time each
/// tick takes. When a tick is late by a whole interval or more — the machine
/// slept, hibernated, or one tick was unusually slow — the schedule restarts
/// from *now* instead of firing the missed ticks back to back. **Missed
/// samples are never caught up**: a batch of fabricated readings for a period
/// the machine was asleep would be worse than the gap the chart shows.
///
/// Wall-clock steps (NTP, a manual change, DST — which does not even exist in
/// UTC) cannot affect this schedule at all; they only move the timestamps the
/// next batches carry.
#[derive(Debug, Clone, Copy)]
pub struct TickSchedule {
    interval: Duration,
    next: Instant,
}

impl TickSchedule {
    /// The first tick is due at `now`.
    pub fn new(interval: Duration, now: Instant) -> Self {
        Self {
            interval,
            next: now,
        }
    }

    /// Records a tick finishing at `now` and returns how long to wait.
    pub fn after_tick(&mut self, now: Instant) -> Duration {
        self.next += self.interval;
        if now >= self.next {
            self.next = now + self.interval;
        }
        self.next.saturating_duration_since(now)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::history::clock::ManualClock;
    use crate::history::query::{HistoryRange, QueryPlan};
    use crate::history::testing::{cpu_total, memory_percent, metric, TempDir};
    use crate::metrics::model::Availability;

    const T0: i64 = 1_800_000_000_000;

    /// A source whose next answers the test decides, and which counts calls.
    #[derive(Default)]
    struct ScriptedSource {
        next: Mutex<Vec<MetricSample>>,
        calls: Mutex<u32>,
    }

    impl ScriptedSource {
        fn answer(&self, samples: Vec<MetricSample>) {
            *self.next.lock().expect("lock") = samples;
        }
    }

    impl SampleSource for ScriptedSource {
        fn sample(&self, _metrics: &[MetricRef]) -> Vec<MetricSample> {
            *self.calls.lock().expect("lock") += 1;
            self.next.lock().expect("lock").clone()
        }
    }

    fn rig(dir: &TempDir) -> (Arc<ScriptedSource>, Arc<ManualClock>, HistorySampler) {
        let source = Arc::new(ScriptedSource::default());
        let clock = Arc::new(ManualClock::new(T0));
        let store = Arc::new(HistoryStore::open(&dir.database()).expect("open"));
        let sampler = HistorySampler::new(
            vec![cpu_total(), memory_percent()],
            source.clone(),
            store,
            clock.clone(),
        );
        (source, clock, sampler)
    }

    fn cpu_points(sampler: &HistorySampler, now: i64) -> Vec<(i64, f64)> {
        let plan = QueryPlan::new(HistoryRange::SixHours, now, 5_000);
        let plan = QueryPlan { raw: true, ..plan };
        sampler.store.query(&[cpu_total()], &plan).expect("query")[0]
            .points
            .iter()
            .map(|p| (p.t, p.value))
            .collect()
    }

    #[test]
    fn a_tick_writes_available_numbers_and_skips_unavailable_ones() {
        let dir = TempDir::new("tick");
        let (source, _clock, sampler) = rig(&dir);
        source.answer(vec![
            MetricSample::number(cpu_total(), 27.4),
            MetricSample::unavailable(
                memory_percent(),
                Availability::temporarily_unavailable("no delta yet"),
            ),
        ]);

        let report = sampler.tick().expect("tick");

        assert_eq!(report.receipt.rows_written, 1);
        assert_eq!(report.unavailable, 1);
        assert_eq!(report.receipt.timestamp_ms, T0);
        let memory = sampler
            .store
            .query(
                &[memory_percent()],
                &QueryPlan::new(HistoryRange::FifteenMinutes, T0, 5_000),
            )
            .expect("query");
        assert!(
            memory[0].points.is_empty(),
            "unavailable is a gap, not a zero"
        );
    }

    #[test]
    fn non_numeric_samples_write_nothing() {
        let dir = TempDir::new("tick-text");
        let (source, _clock, sampler) = rig(&dir);
        source.answer(vec![MetricSample::available(
            cpu_total(),
            MetricValue::text("busy"),
        )]);

        assert_eq!(sampler.tick().expect("tick").receipt.rows_written, 0);
    }

    #[test]
    fn t0_plus_five_plus_ten_long_gap_and_relaunch() {
        let dir = TempDir::new("timeline");
        let two_hours = 2 * 3_600_000;
        {
            let (source, clock, sampler) = rig(&dir);
            for (advance, value) in [(0, 1.0), (5_000, 2.0), (5_000, 3.0), (two_hours, 4.0)] {
                clock.advance(advance);
                source.answer(vec![MetricSample::number(cpu_total(), value)]);
                sampler.tick().expect("tick");
            }
            assert_eq!(
                *source.calls.lock().expect("lock"),
                4,
                "one sample per tick"
            );
        }

        // Relaunch: a new store and sampler on the same file.
        let (source, clock, sampler) = rig(&dir);
        clock.set(T0 + 10_000 + two_hours + 5_000);
        source.answer(vec![MetricSample::number(cpu_total(), 5.0)]);
        sampler.tick().expect("tick after relaunch");

        let now = clock.now_ms();
        assert_eq!(
            cpu_points(&sampler, now),
            [
                (T0, 1.0),
                (T0 + 5_000, 2.0),
                (T0 + 10_000, 3.0),
                (T0 + 10_000 + two_hours, 4.0),
                (T0 + 15_000 + two_hours, 5.0),
            ],
            "nothing was invented for the two missing hours"
        );
    }

    #[test]
    fn the_sampler_asks_only_for_its_own_metrics() {
        let dir = TempDir::new("refs");
        let (_source, _clock, sampler) = rig(&dir);
        assert_eq!(sampler.metrics(), [cpu_total(), memory_percent()]);
        assert!(!sampler
            .metrics()
            .contains(&metric("process.count.total", "process:1-1")));
    }

    #[test]
    fn an_on_time_schedule_does_not_drift() {
        let start = Instant::now();
        let interval = Duration::from_secs(5);
        let mut schedule = TickSchedule::new(interval, start);

        // Each tick took 200 ms: the wait shrinks so ticks stay 5 s apart.
        let wait = schedule.after_tick(start + Duration::from_millis(200));
        assert_eq!(wait, Duration::from_millis(4_800));
        let wait = schedule.after_tick(start + interval + Duration::from_millis(300));
        assert_eq!(wait, Duration::from_millis(4_700));
    }

    #[test]
    fn a_suspend_is_never_caught_up() {
        let start = Instant::now();
        let interval = Duration::from_secs(5);
        let mut schedule = TickSchedule::new(interval, start);

        // The machine slept for an hour after the first tick.
        let resumed = start + Duration::from_secs(3_600);
        let wait = schedule.after_tick(resumed);
        assert_eq!(
            wait, interval,
            "one normal interval, not 720 catch-up ticks"
        );

        let wait = schedule.after_tick(resumed + interval + Duration::from_millis(10));
        assert_eq!(wait, interval - Duration::from_millis(10));
    }

    #[test]
    fn a_slow_tick_restarts_the_schedule_rather_than_bursting() {
        let start = Instant::now();
        let interval = Duration::from_secs(5);
        let mut schedule = TickSchedule::new(interval, start);

        let wait = schedule.after_tick(start + Duration::from_secs(12));
        assert_eq!(wait, interval);
    }
}
