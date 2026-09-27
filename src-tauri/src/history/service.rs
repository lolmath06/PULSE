//! The one history scheduler.
//!
//! [`HistoryService`] owns **the only timer that writes history**: a single
//! background thread that ticks at the configured cadence and runs retention
//! once an hour. Nothing else persists samples — not the Refresh buttons, not a
//! chart, not a provider. The UI learns about new data from one lightweight
//! event per batch and re-queries only what it shows.
//!
//! If the database cannot be opened, the service is built **unavailable** with
//! the reason, no thread is started, and the rest of PULSE carries on live.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::clock::Clock;
use super::error::HistoryError;
use super::query::{HistoryRange, HistoryResponse, QueryPlan};
use super::retention::RetentionPolicy;
use super::sampler::{HistorySampler, SampleSource, TickReport, TickSchedule};
use super::schema::SCHEMA_VERSION;
use super::store::{CompactionReport, DatabaseStats, HistoryStore};
use super::Cadence;
use crate::metrics::model::MetricRef;

/// The most metrics one history request may name.
///
/// Generous — the per-processor view of a 128-thread machine fits — but it
/// keeps a malformed request from asking for the whole catalog at once.
pub const MAX_METRICS_PER_QUERY: usize = 256;

/// How many recent tick timings the status keeps.
const TIMING_WINDOW: usize = 120;

/// The payload of the `history-sample-recorded` event.
///
/// Deliberately tiny: which batch, when, how many rows. The UI re-queries the
/// series it displays; the catalog and the values themselves never travel
/// with the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRecorded {
    pub batch_id: i64,
    pub timestamp_ms: i64,
    pub row_count: u32,
}

/// Where the service reports each batch. Tauri's event emitter in the app, a
/// channel in tests.
pub trait HistoryEventSink: Send + Sync {
    fn batch_recorded(&self, event: &BatchRecorded);
}

/// A sink that discards events.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoEvents;

impl HistoryEventSink for NoEvents {
    fn batch_recorded(&self, _event: &BatchRecorded) {}
}

/// Whether history is being recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum HistoryState {
    Recording,
    Unavailable { reason: String },
}

/// Median and maximum of recent tick timings, in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingSummary {
    pub ticks: usize,
    pub sample_median_us: u64,
    pub sample_max_us: u64,
    pub insert_median_us: u64,
    pub insert_max_us: u64,
}

/// The last retention pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionRun {
    pub at_ms: i64,
    pub duration_ms: u64,
    pub report: CompactionReport,
}

/// A snapshot of the history subsystem, for the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStatus {
    #[serde(flatten)]
    pub state: HistoryState,
    pub database_path: Option<String>,
    pub schema_version: u32,
    pub cadence_ms: i64,
    pub historized_metric_count: usize,
    pub batches_this_session: u64,
    pub last_batch: Option<BatchRecorded>,
    pub last_error: Option<String>,
    pub timings: Option<TimingSummary>,
    pub last_compaction: Option<CompactionRun>,
    /// Row counts and file sizes — only when asked for, since counting rows
    /// scans the tables.
    pub database: Option<DatabaseStats>,
}

/// How the service records.
#[derive(Debug, Clone)]
pub struct HistoryConfig {
    pub database: PathBuf,
    pub cadence: Cadence,
    pub retention: RetentionPolicy,
}

impl HistoryConfig {
    pub fn new(database: PathBuf) -> Self {
        Self {
            database,
            cadence: super::DEFAULT_CADENCE,
            retention: RetentionPolicy::DEFAULT,
        }
    }
}

#[derive(Default)]
struct Runtime {
    batches: u64,
    last_batch: Option<BatchRecorded>,
    last_error: Option<String>,
    sample_us: VecDeque<u64>,
    insert_us: VecDeque<u64>,
    last_compaction: Option<CompactionRun>,
}

impl Runtime {
    fn record(&mut self, report: &TickReport) {
        self.batches += 1;
        self.last_batch = Some(BatchRecorded {
            batch_id: report.receipt.batch_id,
            timestamp_ms: report.receipt.timestamp_ms,
            row_count: report.receipt.rows_written,
        });
        self.last_error = None;
        for (window, duration) in [
            (&mut self.sample_us, report.sample_duration),
            (&mut self.insert_us, report.insert_duration),
        ] {
            if window.len() == TIMING_WINDOW {
                window.pop_front();
            }
            window.push_back(u64::try_from(duration.as_micros()).unwrap_or(u64::MAX));
        }
    }

    fn timings(&self) -> Option<TimingSummary> {
        fn median_and_max(window: &VecDeque<u64>) -> (u64, u64) {
            let mut sorted: Vec<u64> = window.iter().copied().collect();
            sorted.sort_unstable();
            (
                sorted.get(sorted.len() / 2).copied().unwrap_or(0),
                sorted.last().copied().unwrap_or(0),
            )
        }

        if self.sample_us.is_empty() {
            return None;
        }
        let (sample_median_us, sample_max_us) = median_and_max(&self.sample_us);
        let (insert_median_us, insert_max_us) = median_and_max(&self.insert_us);
        Some(TimingSummary {
            ticks: self.sample_us.len(),
            sample_median_us,
            sample_max_us,
            insert_median_us,
            insert_max_us,
        })
    }
}

struct Worker {
    stop: mpsc::Sender<()>,
    handle: JoinHandle<()>,
}

/// The history subsystem: store, scheduler thread and query entry point.
pub struct HistoryService {
    store: Option<Arc<HistoryStore>>,
    unavailable: Option<String>,
    cadence: Cadence,
    historized: usize,
    clock: Arc<dyn Clock>,
    runtime: Arc<Mutex<Runtime>>,
    worker: Mutex<Option<Worker>>,
}

impl std::fmt::Debug for HistoryService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryService")
            .field("store", &self.store)
            .field("unavailable", &self.unavailable)
            .field("historized", &self.historized)
            .finish()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl HistoryService {
    /// A service that records nothing, and says why.
    pub fn unavailable(
        reason: impl Into<String>,
        historized: usize,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            store: None,
            unavailable: Some(reason.into()),
            cadence: super::DEFAULT_CADENCE,
            historized,
            clock,
            runtime: Arc::new(Mutex::new(Runtime::default())),
            worker: Mutex::new(None),
        }
    }

    /// Opens the database and starts the scheduler thread.
    ///
    /// The first batch is taken immediately, after one retention pass; then
    /// one every `config.cadence`. A database that cannot be opened yields an
    /// unavailable service rather than an error: PULSE starts either way.
    pub fn start(
        config: HistoryConfig,
        metrics: Vec<MetricRef>,
        source: Arc<dyn SampleSource>,
        clock: Arc<dyn Clock>,
        events: Arc<dyn HistoryEventSink>,
    ) -> Self {
        let historized = metrics.len();
        let store = match HistoryStore::open(&config.database) {
            Ok(store) => Arc::new(store),
            Err(error) => {
                eprintln!("PULSE: history unavailable: {error}");
                return Self::unavailable(error.to_string(), historized, clock);
            }
        };

        let runtime = Arc::new(Mutex::new(Runtime::default()));
        let (stop, stopped) = mpsc::channel::<()>();
        let sampler = HistorySampler::new(metrics, source, Arc::clone(&store), Arc::clone(&clock));
        let thread_store = Arc::clone(&store);
        let thread_clock = Arc::clone(&clock);
        let thread_runtime = Arc::clone(&runtime);
        let cadence = config.cadence;
        let retention = config.retention;

        let spawned = std::thread::Builder::new()
            .name("pulse-history".into())
            .spawn(move || {
                run(
                    &sampler,
                    &thread_store,
                    thread_clock.as_ref(),
                    &thread_runtime,
                    events.as_ref(),
                    cadence,
                    &retention,
                    &stopped,
                )
            });

        let worker = match spawned {
            Ok(handle) => Some(Worker { stop, handle }),
            Err(error) => {
                eprintln!("PULSE: history scheduler could not start: {error}");
                return Self::unavailable(
                    format!("the history scheduler could not start: {error}"),
                    historized,
                    clock,
                );
            }
        };

        Self {
            store: Some(store),
            unavailable: None,
            cadence,
            historized,
            clock,
            runtime,
            worker: Mutex::new(worker),
        }
    }

    pub fn is_recording(&self) -> bool {
        self.store.is_some()
    }

    /// Answers one request with a bounded number of points per series.
    pub fn query(
        &self,
        metrics: &[MetricRef],
        range: HistoryRange,
    ) -> Result<HistoryResponse, HistoryError> {
        let Some(store) = &self.store else {
            return Err(HistoryError::Open {
                path: PathBuf::new(),
                message: self
                    .unavailable
                    .clone()
                    .unwrap_or_else(|| "history is unavailable".into()),
            });
        };
        if metrics.len() > MAX_METRICS_PER_QUERY {
            return Err(HistoryError::InvalidRequest(format!(
                "{} metrics requested, at most {MAX_METRICS_PER_QUERY} are allowed",
                metrics.len()
            )));
        }

        let plan = QueryPlan::new(range, self.clock.now_ms(), self.cadence.millis());
        let series = store.query(metrics, &plan)?;
        Ok(HistoryResponse {
            range,
            plan,
            series,
        })
    }

    /// The subsystem's state. `include_database` adds row counts and sizes,
    /// which scan the tables.
    pub fn status(&self, include_database: bool) -> HistoryStatus {
        let runtime = lock(&self.runtime);
        let (database, stats_error) = match (&self.store, include_database) {
            (Some(store), true) => match store.stats() {
                Ok(stats) => (Some(stats), None),
                Err(error) => (None, Some(error.to_string())),
            },
            _ => (None, None),
        };

        HistoryStatus {
            state: match &self.unavailable {
                Some(reason) => HistoryState::Unavailable {
                    reason: reason.clone(),
                },
                None => HistoryState::Recording,
            },
            database_path: self
                .store
                .as_ref()
                .map(|store| store.path().display().to_string()),
            schema_version: SCHEMA_VERSION,
            cadence_ms: self.cadence.millis(),
            historized_metric_count: self.historized,
            batches_this_session: runtime.batches,
            last_batch: runtime.last_batch,
            last_error: runtime.last_error.clone().or(stats_error),
            timings: runtime.timings(),
            last_compaction: runtime.last_compaction,
            database,
        }
    }

    /// Stops the scheduler and waits for its current tick to finish.
    ///
    /// Idempotent. Called on application exit so the last batch is committed
    /// and the WAL is checkpointed when the store is dropped.
    pub fn shutdown(&self) {
        if let Some(worker) = lock(&self.worker).take() {
            let _ = worker.stop.send(());
            let _ = worker.handle.join();
            if let Some(store) = &self.store {
                if let Err(error) = store.checkpoint() {
                    eprintln!("PULSE: history checkpoint on exit failed: {error}");
                }
            }
            eprintln!("PULSE: history recorder stopped");
        }
    }
}

impl Drop for HistoryService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    sampler: &HistorySampler,
    store: &HistoryStore,
    clock: &dyn Clock,
    runtime: &Mutex<Runtime>,
    events: &dyn HistoryEventSink,
    cadence: Cadence,
    retention: &RetentionPolicy,
    stopped: &mpsc::Receiver<()>,
) {
    let mut schedule = TickSchedule::new(cadence.duration(), Instant::now());
    // Retention runs first, at startup, then once per `retention.interval`.
    let mut next_compaction = Instant::now();

    loop {
        if Instant::now() >= next_compaction {
            let began = Instant::now();
            let at_ms = clock.now_ms();
            match store.compact(at_ms, retention) {
                Ok(report) => {
                    lock(runtime).last_compaction = Some(CompactionRun {
                        at_ms,
                        duration_ms: u64::try_from(began.elapsed().as_millis()).unwrap_or(0),
                        report,
                    });
                }
                Err(error) => {
                    eprintln!("PULSE: history retention failed: {error}");
                    lock(runtime).last_error = Some(error.to_string());
                }
            }
            next_compaction = Instant::now() + retention.interval;
        }

        match sampler.tick() {
            Ok(report) => {
                let event = {
                    let mut runtime = lock(runtime);
                    runtime.record(&report);
                    runtime.last_batch
                };
                if let Some(event) = event {
                    events.batch_recorded(&event);
                }
            }
            Err(error) => {
                eprintln!("PULSE: history batch failed: {error}");
                lock(runtime).last_error = Some(error.to_string());
            }
        }

        let wait = schedule.after_tick(Instant::now());
        match stopped.recv_timeout(wait) {
            Err(RecvTimeoutError::Timeout) => continue,
            // A stop request, or the service was dropped.
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::history::clock::{ManualClock, SystemClock};
    use crate::history::testing::{cpu_total, memory_percent, TempDir};
    use crate::metrics::model::MetricSample;

    struct Fixed;

    impl SampleSource for Fixed {
        fn sample(&self, metrics: &[MetricRef]) -> Vec<MetricSample> {
            metrics
                .iter()
                .map(|metric| MetricSample::number(metric.clone(), 42.0))
                .collect()
        }
    }

    /// Forwards events to a channel the test can wait on.
    struct Channel(Mutex<mpsc::Sender<BatchRecorded>>);

    impl HistoryEventSink for Channel {
        fn batch_recorded(&self, event: &BatchRecorded) {
            let _ = lock(&self.0).send(*event);
        }
    }

    #[test]
    fn a_database_that_cannot_open_leaves_an_honest_unavailable_service() {
        let dir = TempDir::new("service-fail");
        // A directory where the file should be: SQLite cannot open it.
        std::fs::create_dir_all(dir.database()).expect("block the path");

        let service = HistoryService::start(
            HistoryConfig::new(dir.database()),
            vec![cpu_total()],
            Arc::new(Fixed),
            Arc::new(SystemClock),
            Arc::new(NoEvents),
        );

        assert!(!service.is_recording());
        let status = service.status(true);
        assert!(matches!(status.state, HistoryState::Unavailable { .. }));
        assert_eq!(status.historized_metric_count, 1);
        assert!(service
            .query(&[cpu_total()], HistoryRange::FifteenMinutes)
            .is_err());
    }

    #[test]
    fn the_scheduler_records_immediately_emits_an_event_and_stops_promptly() {
        let dir = TempDir::new("service-run");
        let (sender, received) = mpsc::channel();
        let clock = Arc::new(ManualClock::new(1_800_000_000_000));

        let service = HistoryService::start(
            HistoryConfig::new(dir.database()),
            vec![cpu_total(), memory_percent()],
            Arc::new(Fixed),
            clock,
            Arc::new(Channel(Mutex::new(sender))),
        );

        let event = received
            .recv_timeout(Duration::from_secs(5))
            .expect("the first batch is immediate");
        assert_eq!(event.row_count, 2);
        assert_eq!(event.timestamp_ms, 1_800_000_000_000);

        // Stopping interrupts the 5-second wait instead of sitting it out.
        let began = Instant::now();
        service.shutdown();
        assert!(began.elapsed() < Duration::from_secs(2));

        let status = service.status(true);
        assert_eq!(status.state, HistoryState::Recording);
        assert_eq!(status.batches_this_session, 1);
        assert_eq!(status.cadence_ms, 5_000);
        assert!(status.timings.is_some());
        assert!(status.last_compaction.is_some(), "retention ran at startup");
        let database = status.database.expect("stats");
        assert_eq!(database.raw_rows, 2);
        assert_eq!(database.batch_count, 1);

        let response = service
            .query(&[cpu_total()], HistoryRange::FifteenMinutes)
            .expect("query");
        assert_eq!(response.series[0].points.len(), 1);
        assert_eq!(response.plan.gap_threshold_ms, 15_000);
    }

    #[test]
    fn oversized_requests_are_refused() {
        let dir = TempDir::new("service-cap");
        let service = HistoryService::start(
            HistoryConfig::new(dir.database()),
            Vec::new(),
            Arc::new(Fixed),
            Arc::new(SystemClock),
            Arc::new(NoEvents),
        );
        let many = vec![cpu_total(); MAX_METRICS_PER_QUERY + 1];

        assert!(matches!(
            service.query(&many, HistoryRange::OneHour),
            Err(HistoryError::InvalidRequest(_))
        ));
    }

    #[test]
    fn live_sampling_never_writes_history() {
        // The Refresh buttons call the engine directly. Sampling the same
        // source many times outside the scheduler must not add a single row.
        let dir = TempDir::new("service-refresh");
        let (sender, received) = mpsc::channel();
        let source = Arc::new(Fixed);
        let service = HistoryService::start(
            HistoryConfig::new(dir.database()),
            vec![cpu_total()],
            source.clone(),
            Arc::new(ManualClock::new(1_800_000_000_000)),
            Arc::new(Channel(Mutex::new(sender))),
        );
        received
            .recv_timeout(Duration::from_secs(5))
            .expect("first batch");

        for _ in 0..25 {
            let _ = source.sample(&[cpu_total()]);
        }

        let database = service.status(true).database.expect("stats");
        assert_eq!(database.batch_count, 1);
        assert_eq!(database.raw_rows, 1);
    }
}
