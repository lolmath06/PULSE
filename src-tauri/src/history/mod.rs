//! Persistent metric history.
//!
//! ```text
//! MetricsEngine ──sample──► HistorySampler ──batch──► HistoryStore (SQLite)
//!       ▲                        ▲                         │
//!       │                  one scheduler thread            │ bounded queries
//!       │                  (HistoryService)                ▼
//!  Refresh buttons                                  HistoryQuery ──► UI charts
//!  (live only, never persisted)
//! ```
//!
//! Four pieces, each with one job:
//!
//! - [`HistoryStore`] — the SQLite file: schema, migrations, one transaction per
//!   batch, bounded queries, compaction. Knows nothing about scheduling.
//! - [`HistorySampler`] — one tick: sample the historized metrics once, keep the
//!   finite numbers, write them as one batch. Knows nothing about threads.
//! - [`HistoryService`] — the **only** scheduler: one background thread that
//!   ticks at the configured cadence, runs retention at a low frequency, and
//!   reports each batch to an event sink.
//! - [`query`] — range → resolution planning and the response the UI receives.
//!
//! The module is deliberately **tauri-free** — the event sink is a trait and the
//! database path is an argument — so the Windows harness type checks all of it
//! and every test drives it with an injected clock instead of real seconds.
//!
//! See `docs/history/architecture.md`, `storage.md` and `retention.md`.

pub mod clock;
pub mod error;
pub mod query;
pub mod retention;
pub mod sampler;
pub mod schema;
pub mod selection;
pub mod service;
pub mod store;

pub use clock::{Clock, ManualClock, SystemClock};
pub use error::HistoryError;
pub use query::{HistoryPoint, HistoryRange, HistoryResponse, HistorySeries, QueryPlan};
pub use retention::RetentionPolicy;
pub use sampler::{HistorySampler, SampleSource, TickReport, TickSchedule};
pub use service::{
    BatchRecorded, HistoryEventSink, HistoryService, HistoryState, HistoryStatus, NoEvents,
};
pub use store::{BatchReceipt, CompactionReport, DatabaseStats, HistoryStore};

use std::time::Duration;

/// How often the scheduler records one batch, by default.
///
/// Five seconds: fine enough to see a spike that lasted a few seconds, coarse
/// enough that a day of raw history for a hundred metrics stays in the tens of
/// megabytes. See `docs/history/storage.md` for the arithmetic.
pub const DEFAULT_CADENCE: Cadence = Cadence::FiveSeconds;

/// The cadences the architecture supports.
///
/// Only [`DEFAULT_CADENCE`] is used in Phase 10 — there is no settings screen
/// yet — but every piece below takes the cadence as a value, so a later setting
/// changes a constant rather than code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    OneSecond,
    FiveSeconds,
    TenSeconds,
    ThirtySeconds,
}

impl Cadence {
    pub const ALL: [Cadence; 4] = [
        Cadence::OneSecond,
        Cadence::FiveSeconds,
        Cadence::TenSeconds,
        Cadence::ThirtySeconds,
    ];

    /// The interval between two batches, in milliseconds.
    pub const fn millis(self) -> i64 {
        match self {
            Cadence::OneSecond => 1_000,
            Cadence::FiveSeconds => 5_000,
            Cadence::TenSeconds => 10_000,
            Cadence::ThirtySeconds => 30_000,
        }
    }

    pub const fn duration(self) -> Duration {
        Duration::from_millis(self.millis() as u64)
    }
}

/// The database file name inside the application's local data directory.
pub const DATABASE_FILE_NAME: &str = "history.sqlite3";

/// Where the history database lives, given the platform's local data directory.
///
/// The directory itself comes from Tauri's path resolver — `$XDG_DATA_HOME`
/// (usually `~/.local/share/dev.pulse.app`) on Fedora, `%LOCALAPPDATA%\dev.pulse.app`
/// on Windows — and is never guessed from the working directory or a home path.
pub fn database_path(local_data_dir: &std::path::Path) -> std::path::PathBuf {
    local_data_dir.join(DATABASE_FILE_NAME)
}

#[cfg(test)]
pub(crate) mod testing;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_cadence_is_five_seconds() {
        assert_eq!(DEFAULT_CADENCE.millis(), 5_000);
        assert_eq!(DEFAULT_CADENCE.duration(), Duration::from_secs(5));
    }

    #[test]
    fn every_supported_cadence_is_distinct_and_ascending() {
        let millis: Vec<i64> = Cadence::ALL.iter().map(|c| c.millis()).collect();
        assert_eq!(millis, [1_000, 5_000, 10_000, 30_000]);
    }

    #[test]
    fn the_database_lives_in_the_directory_it_is_given() {
        let base = std::path::Path::new("base-dir");
        assert_eq!(database_path(base), base.join("history.sqlite3"));
    }
}
