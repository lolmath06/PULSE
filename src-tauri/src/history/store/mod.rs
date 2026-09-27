//! The SQLite file behind history.
//!
//! # Connections
//!
//! Two connections to one file, each behind its own short-lived mutex:
//!
//! - the **writer** — the scheduler's batches and the retention pass;
//! - the **reader** — the UI's queries, opened read-only.
//!
//! In WAL mode a reader never waits for the writer and the writer never waits
//! for a reader, so a chart loading a week of history does not delay the next
//! batch, and a batch being written does not stall a chart. Each mutex is held
//! for one statement group only — never across a sleep, never across an IPC
//! call.
//!
//! # SQLite settings
//!
//! | Setting              | Value     | Why                                                      |
//! | -------------------- | --------- | -------------------------------------------------------- |
//! | `journal_mode`       | `WAL`     | readers and the writer do not block each other          |
//! | `synchronous`        | `NORMAL`  | durable at every checkpoint; one fsync per checkpoint rather than per batch |
//! | `busy_timeout`       | 2 000 ms  | a rare lock wait is retried instead of failing a batch   |
//! | `journal_size_limit` | 16 MiB    | the WAL file is truncated back after checkpoints          |
//! | `foreign_keys`       | `ON`      | a sample can only reference an existing series           |
//!
//! `NORMAL` in WAL mode can lose the last few batches on a power cut — never
//! corrupt the file. For a monitoring history that trade is right: an fsync
//! every five seconds would be pure write amplification.
//!
//! # Privacy
//!
//! A row is a series number, a timestamp and a number. The series table holds
//! the metric key and a **digest** of the source identifier, not the
//! identifier itself: some sources are derived from a MAC address or a drive
//! serial, and history has no need to store either. See
//! `docs/history/storage.md`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::error::HistoryError;
use super::query::{HistoryPoint, HistorySeries, LatestSample, QueryPlan};
use super::retention::RetentionPolicy;
use super::schema::{self, SCHEMA_VERSION};
use super::selection;
use crate::metrics::model::{MetricRef, SourceId};

/// How long a statement waits on a lock before giving up.
pub const BUSY_TIMEOUT: Duration = Duration::from_millis(2_000);

/// The WAL file is truncated back to this after a checkpoint.
pub const JOURNAL_SIZE_LIMIT_BYTES: i64 = 16 * 1024 * 1024;

/// The digest history stores in place of a source identifier.
///
/// `kind:` followed by 64 bits of SHA-256 over a fixed, versioned prefix and
/// the full identifier. The kind stays readable (`cpu:…`, `network:…`) so the
/// file can still be inspected by family; the instance — which may be derived
/// from a MAC address or a serial number — does not appear.
///
/// This is pseudonymisation, not anonymisation: someone holding both the file
/// and a candidate identifier can confirm a match. It keeps identifiers out of
/// the file; it does not make them unguessable.
pub fn source_digest(source: &SourceId) -> String {
    let text = source.as_str();
    let kind = text.split(':').next().unwrap_or("source");

    let mut hasher = Sha256::new();
    hasher.update(b"pulse.history.source.v1\0");
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();

    let mut hex = String::with_capacity(16);
    for byte in &digest[..8] {
        hex.push_str(&format!("{byte:02x}"));
    }
    format!("{kind}:{hex}")
}

/// What one batch wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchReceipt {
    pub batch_id: i64,
    pub timestamp_ms: i64,
    /// Rows actually inserted.
    pub rows_written: u32,
    /// Values dropped before insertion because they were NaN or infinite.
    pub skipped_non_finite: u32,
    /// Values refused because their series may not be historized.
    pub skipped_not_historizable: u32,
}

/// What one retention pass did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionReport {
    /// Aggregate buckets created or merged into.
    pub aggregates_written: u64,
    /// Raw rows removed after their bucket was written.
    pub raw_rows_deleted: u64,
    /// Aggregates removed for being older than the retention window.
    pub aggregates_deleted: u64,
    pub batches_deleted: u64,
}

/// A description of the file, for diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStats {
    pub path: String,
    pub schema_version: u32,
    pub journal_mode: String,
    /// `0` OFF, `1` NORMAL, `2` FULL, `3` EXTRA — SQLite's own numbering.
    pub synchronous: i64,
    pub busy_timeout_ms: u64,
    /// The main file plus its WAL, as the filesystem reports them.
    pub file_bytes: u64,
    pub wal_bytes: u64,
    pub page_size: i64,
    pub page_count: i64,
    pub freelist_count: i64,
    pub series_count: i64,
    pub raw_rows: i64,
    pub aggregate_rows: i64,
    pub batch_count: i64,
    pub average_rows_per_batch: Option<f64>,
    pub first_batch_ms: Option<i64>,
    pub last_batch_ms: Option<i64>,
}

struct Writer {
    connection: Connection,
    /// Series numbers already known to exist. Only ever filled after a commit,
    /// so a rolled-back batch cannot leave a phantom id behind.
    series: HashMap<MetricRef, i64>,
}

struct Reader {
    connection: Connection,
    /// Found series only: a series the writer creates later must still be
    /// found, so an absence is never cached.
    series: HashMap<MetricRef, i64>,
}

/// The history database.
pub struct HistoryStore {
    path: PathBuf,
    journal_mode: String,
    writer: Mutex<Writer>,
    reader: Mutex<Reader>,
}

impl std::fmt::Debug for HistoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryStore")
            .field("path", &self.path)
            .field("journal_mode", &self.journal_mode)
            .finish()
    }
}

/// Recovers a poisoned lock: every critical section here is a self-contained
/// SQLite call whose failure is already a `Result`, so the data behind the
/// mutex is still consistent after a panic elsewhere.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn open_error(path: &Path, error: impl std::fmt::Display) -> HistoryError {
    HistoryError::Open {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

impl HistoryStore {
    /// Opens (creating if needed) the database at `path` and migrates it.
    ///
    /// A file written by a newer PULSE is refused **before** anything is
    /// written to it — not even a pragma that would change its header.
    pub fn open(path: &Path) -> Result<Self, HistoryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| open_error(parent, error))?;
        }

        let mut writer = Connection::open(path).map_err(|error| open_error(path, error))?;
        writer.busy_timeout(BUSY_TIMEOUT)?;

        if let Some(found) = schema::read_version(&writer)? {
            if found > SCHEMA_VERSION {
                return Err(HistoryError::FutureSchema {
                    found,
                    supported: SCHEMA_VERSION,
                });
            }
        }

        let journal_mode: String =
            writer.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        writer.execute_batch(&format!(
            "PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA journal_size_limit = {JOURNAL_SIZE_LIMIT_BYTES};"
        ))?;
        schema::migrate(&mut writer)?;

        let reader = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(|error| open_error(path, error))?;
        reader.busy_timeout(BUSY_TIMEOUT)?;

        Ok(Self {
            path: path.to_path_buf(),
            journal_mode: journal_mode.to_ascii_lowercase(),
            writer: Mutex::new(Writer {
                connection: writer,
                series: HashMap::new(),
            }),
            reader: Mutex::new(Reader {
                connection: reader,
                series: HashMap::new(),
            }),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The journal mode SQLite actually accepted — `wal` everywhere PULSE
    /// runs, but reported rather than assumed.
    pub fn journal_mode(&self) -> &str {
        &self.journal_mode
    }

    /// Writes one batch: every value recorded at `timestamp_ms`, in **one**
    /// transaction.
    ///
    /// - Non-finite values are dropped, never stored as a number or a NULL.
    /// - A reference that may not be historized (a per-process series) is
    ///   refused here too, whatever the caller selected.
    /// - A second value for the same series at the same instant is ignored,
    ///   so replaying a batch cannot duplicate it.
    ///
    /// Either every row of the batch is written or none is.
    pub fn insert_batch(
        &self,
        timestamp_ms: i64,
        values: &[(MetricRef, f64)],
    ) -> Result<BatchReceipt, HistoryError> {
        let mut guard = lock(&self.writer);
        let Writer { connection, series } = &mut *guard;

        let mut skipped_non_finite = 0_u32;
        let mut skipped_not_historizable = 0_u32;
        let mut created: Vec<(MetricRef, i64)> = Vec::new();
        let mut rows_written = 0_u32;

        let transaction = connection.transaction()?;
        {
            let mut insert_series = transaction.prepare_cached(
                "INSERT OR IGNORE INTO metric_series (metric_key, source_digest) VALUES (?1, ?2)",
            )?;
            let mut find_series = transaction.prepare_cached(
                "SELECT series_id FROM metric_series WHERE metric_key = ?1 AND source_digest = ?2",
            )?;
            let mut insert_sample = transaction.prepare_cached(
                "INSERT OR IGNORE INTO raw_metric_samples (series_id, timestamp_ms, value)
                 VALUES (?1, ?2, ?3)",
            )?;

            for (metric, value) in values {
                if !value.is_finite() {
                    skipped_non_finite += 1;
                    continue;
                }
                if !selection::is_historizable(metric) {
                    skipped_not_historizable += 1;
                    continue;
                }

                let id = match series.get(metric) {
                    Some(id) => *id,
                    None => {
                        let digest = source_digest(&metric.source_id);
                        insert_series.execute(params![metric.key.as_str(), digest])?;
                        let id: i64 = find_series
                            .query_row(params![metric.key.as_str(), digest], |row| row.get(0))?;
                        created.push((metric.clone(), id));
                        id
                    }
                };

                let inserted = insert_sample.execute(params![id, timestamp_ms, value])?;
                rows_written += inserted as u32;
            }
        }

        transaction.execute(
            "INSERT INTO sample_batches (timestamp_ms, row_count) VALUES (?1, ?2)",
            params![timestamp_ms, rows_written],
        )?;
        let batch_id = transaction.last_insert_rowid();
        transaction.commit()?;

        series.extend(created);

        Ok(BatchReceipt {
            batch_id,
            timestamp_ms,
            rows_written,
            skipped_non_finite,
            skipped_not_historizable,
        })
    }

    /// Answers a request, one series per metric, in request order.
    ///
    /// A metric with no recorded series yields an empty series, not an error.
    /// A row that cannot be read (wrong type, non-finite) is skipped and
    /// counted in `skipped_rows`; it never fails the request or panics.
    pub fn query(
        &self,
        metrics: &[MetricRef],
        plan: &QueryPlan,
    ) -> Result<Vec<HistorySeries>, HistoryError> {
        let mut guard = lock(&self.reader);
        let Reader { connection, series } = &mut *guard;

        let mut answer = Vec::with_capacity(metrics.len());
        for metric in metrics {
            let id = match series.get(metric) {
                Some(id) => Some(*id),
                None => {
                    let found: Option<i64> = connection
                        .prepare_cached(
                            "SELECT series_id FROM metric_series
                             WHERE metric_key = ?1 AND source_digest = ?2",
                        )?
                        .query_row(
                            params![metric.key.as_str(), source_digest(&metric.source_id)],
                            |row| row.get(0),
                        )
                        .optional()?;
                    if let Some(id) = found {
                        series.insert(metric.clone(), id);
                    }
                    found
                }
            };

            let Some(id) = id else {
                answer.push(HistorySeries {
                    metric: metric.clone(),
                    points: Vec::new(),
                    latest: None,
                    skipped_rows: 0,
                });
                continue;
            };

            let (points, skipped_rows) = if plan.raw {
                read_raw(connection, id, plan)?
            } else {
                read_bucketed(connection, id, plan)?
            };
            let latest = read_latest(connection, id, plan)?;

            answer.push(HistorySeries {
                metric: metric.clone(),
                points,
                latest,
                skipped_rows,
            });
        }

        Ok(answer)
    }

    /// Applies `policy` as of `now_ms`: folds old raw rows into aggregates,
    /// then drops what is past the retention window.
    ///
    /// Each series is compacted in **its own transaction**, and within it the
    /// raw rows are deleted only after their aggregate was written. A failure
    /// rolls that series back untouched and stops the pass; the next pass
    /// resumes where it failed.
    pub fn compact(
        &self,
        now_ms: i64,
        policy: &RetentionPolicy,
    ) -> Result<CompactionReport, HistoryError> {
        let raw_cutoff = policy.raw_cutoff(now_ms);
        let aggregate_cutoff = policy.aggregate_cutoff(now_ms);
        let mut report = CompactionReport::default();

        let mut guard = lock(&self.writer);
        let connection = &mut guard.connection;

        let ids: Vec<i64> = {
            let mut statement =
                connection.prepare("SELECT series_id FROM metric_series ORDER BY series_id")?;
            let ids = statement
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<i64>, _>>()?;
            ids
        };

        for id in ids {
            let transaction = connection.transaction()?;
            let written = transaction.execute(
                "INSERT INTO aggregate_metric_samples
                     (series_id, bucket_start_ms, bucket_ms, min_value, max_value, avg_value, sample_count)
                 SELECT series_id, (timestamp_ms / ?3) * ?3 AS bucket, ?3,
                        MIN(value), MAX(value), AVG(value), COUNT(*)
                 FROM raw_metric_samples
                 WHERE series_id = ?1 AND timestamp_ms < ?2
                   AND typeof(value) IN ('real', 'integer')
                 GROUP BY bucket
                 ON CONFLICT (series_id, bucket_start_ms) DO UPDATE SET
                     min_value = MIN(min_value, excluded.min_value),
                     max_value = MAX(max_value, excluded.max_value),
                     avg_value = (avg_value * sample_count + excluded.avg_value * excluded.sample_count)
                                 / (sample_count + excluded.sample_count),
                     sample_count = sample_count + excluded.sample_count",
                params![id, raw_cutoff, policy.bucket_ms],
            )?;
            let deleted = transaction.execute(
                "DELETE FROM raw_metric_samples WHERE series_id = ?1 AND timestamp_ms < ?2",
                params![id, raw_cutoff],
            )?;
            let expired = transaction.execute(
                "DELETE FROM aggregate_metric_samples WHERE series_id = ?1 AND bucket_start_ms < ?2",
                params![id, aggregate_cutoff],
            )?;
            transaction.commit()?;

            report.aggregates_written += written as u64;
            report.raw_rows_deleted += deleted as u64;
            report.aggregates_deleted += expired as u64;
        }

        report.batches_deleted = connection.execute(
            "DELETE FROM sample_batches WHERE timestamp_ms < ?1",
            params![raw_cutoff],
        )? as u64;

        Ok(report)
    }

    /// Folds the WAL back into the main file and truncates it.
    ///
    /// Called once on shutdown, so a closed PULSE leaves one self-contained
    /// file. Not required for safety: an un-checkpointed WAL is replayed the
    /// next time the database opens.
    pub fn checkpoint(&self) -> Result<(), HistoryError> {
        let guard = lock(&self.writer);
        guard
            .connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }

    /// Describes the file. Counts every row, so it is meant for diagnostics,
    /// not for every tick.
    pub fn stats(&self) -> Result<DatabaseStats, HistoryError> {
        let guard = lock(&self.reader);
        let connection = &guard.connection;

        let pragma = |name: &str| -> Result<i64, HistoryError> {
            Ok(connection.query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))?)
        };
        let count = |table: &str| -> Result<i64, HistoryError> {
            Ok(
                connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })?,
            )
        };

        let (average_rows_per_batch, first_batch_ms, last_batch_ms): (
            Option<f64>,
            Option<i64>,
            Option<i64>,
        ) = connection.query_row(
            "SELECT AVG(row_count), MIN(timestamp_ms), MAX(timestamp_ms) FROM sample_batches",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        let size_of = |path: &Path| std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let mut wal = self.path.clone().into_os_string();
        wal.push("-wal");

        Ok(DatabaseStats {
            path: self.path.display().to_string(),
            schema_version: schema::read_version(connection)?.unwrap_or(0),
            journal_mode: self.journal_mode.clone(),
            synchronous: lock(&self.writer).connection.query_row(
                "PRAGMA synchronous",
                [],
                |row| row.get(0),
            )?,
            busy_timeout_ms: BUSY_TIMEOUT.as_millis() as u64,
            file_bytes: size_of(&self.path),
            wal_bytes: size_of(Path::new(&wal)),
            page_size: pragma("page_size")?,
            page_count: pragma("page_count")?,
            freelist_count: pragma("freelist_count")?,
            series_count: count("metric_series")?,
            raw_rows: count("raw_metric_samples")?,
            aggregate_rows: count("aggregate_metric_samples")?,
            batch_count: count("sample_batches")?,
            average_rows_per_batch,
            first_batch_ms,
            last_batch_ms,
        })
    }
}

/// Reads one value column defensively: a wrong type or a non-finite number is
/// `None`, which the caller counts as a skipped row.
fn finite(value: rusqlite::Result<f64>) -> Option<f64> {
    value.ok().filter(|number| number.is_finite())
}

fn read_raw(
    connection: &Connection,
    series_id: i64,
    plan: &QueryPlan,
) -> Result<(Vec<HistoryPoint>, u32), HistoryError> {
    let mut statement = connection.prepare_cached(
        "SELECT timestamp_ms, value FROM raw_metric_samples
         WHERE series_id = ?1 AND timestamp_ms >= ?2 AND timestamp_ms <= ?3
         ORDER BY timestamp_ms",
    )?;
    let mut rows = statement.query(params![series_id, plan.from_ms, plan.to_ms])?;

    let mut points = Vec::new();
    let mut skipped = 0_u32;
    while let Some(row) = rows.next()? {
        match (row.get::<_, i64>(0).ok(), finite(row.get(1))) {
            (Some(t), Some(value)) => points.push(HistoryPoint::raw(t, value)),
            _ => skipped += 1,
        }
    }
    Ok((points, skipped))
}

#[derive(Clone, Copy)]
struct Bucket {
    min: f64,
    max: f64,
    sum: f64,
    count: u64,
}

impl Bucket {
    fn merge(&mut self, min: f64, max: f64, sum: f64, count: u64) {
        self.min = self.min.min(min);
        self.max = self.max.max(max);
        self.sum += sum;
        self.count += count;
    }
}

/// Buckets raw rows and stored aggregates together into `plan.bucket_ms`.
///
/// Both halves are read row by row rather than aggregated in SQL so that a
/// corrupt row is *skipped and counted* instead of being silently coerced to
/// zero by `SUM`.
fn read_bucketed(
    connection: &Connection,
    series_id: i64,
    plan: &QueryPlan,
) -> Result<(Vec<HistoryPoint>, u32), HistoryError> {
    let bucket_of = |t: i64| t - t.rem_euclid(plan.bucket_ms);
    let mut buckets: BTreeMap<i64, Bucket> = BTreeMap::new();
    let mut skipped = 0_u32;

    let mut add = |t: i64, min: f64, max: f64, sum: f64, count: u64| {
        buckets
            .entry(bucket_of(t))
            .and_modify(|bucket| bucket.merge(min, max, sum, count))
            .or_insert(Bucket {
                min,
                max,
                sum,
                count,
            });
    };

    {
        let mut statement = connection.prepare_cached(
            "SELECT bucket_start_ms, min_value, max_value, avg_value, sample_count
             FROM aggregate_metric_samples
             WHERE series_id = ?1 AND bucket_start_ms >= ?2 AND bucket_start_ms <= ?3
             ORDER BY bucket_start_ms",
        )?;
        let mut rows = statement.query(params![series_id, plan.from_ms, plan.to_ms])?;
        while let Some(row) = rows.next()? {
            let t = row.get::<_, i64>(0).ok();
            let min = finite(row.get(1));
            let max = finite(row.get(2));
            let avg = finite(row.get(3));
            let count = row.get::<_, i64>(4).ok().filter(|count| *count > 0);
            match (t, min, max, avg, count) {
                (Some(t), Some(min), Some(max), Some(avg), Some(count)) if min <= max => {
                    add(t, min, max, avg * count as f64, count as u64)
                }
                _ => skipped += 1,
            }
        }
    }

    {
        let mut statement = connection.prepare_cached(
            "SELECT timestamp_ms, value FROM raw_metric_samples
             WHERE series_id = ?1 AND timestamp_ms >= ?2 AND timestamp_ms <= ?3
             ORDER BY timestamp_ms",
        )?;
        let mut rows = statement.query(params![series_id, plan.from_ms, plan.to_ms])?;
        while let Some(row) = rows.next()? {
            match (row.get::<_, i64>(0).ok(), finite(row.get(1))) {
                (Some(t), Some(value)) => add(t, value, value, value, 1),
                _ => skipped += 1,
            }
        }
    }

    let points = buckets
        .into_iter()
        .map(|(t, bucket)| HistoryPoint {
            t,
            value: bucket.sum / bucket.count as f64,
            min: Some(bucket.min),
            max: Some(bucket.max),
            count: Some(u32::try_from(bucket.count).unwrap_or(u32::MAX)),
        })
        .collect();
    Ok((points, skipped))
}

fn read_latest(
    connection: &Connection,
    series_id: i64,
    plan: &QueryPlan,
) -> Result<Option<LatestSample>, HistoryError> {
    let mut statement = connection.prepare_cached(
        "SELECT timestamp_ms, value FROM raw_metric_samples
         WHERE series_id = ?1 AND timestamp_ms >= ?2 AND timestamp_ms <= ?3
         ORDER BY timestamp_ms DESC LIMIT 1",
    )?;
    let latest = statement
        .query_row(params![series_id, plan.from_ms, plan.to_ms], |row| {
            Ok((row.get::<_, i64>(0).ok(), finite(row.get(1))))
        })
        .optional()?;

    Ok(match latest {
        Some((Some(t), Some(value))) => Some(LatestSample { t, value }),
        _ => None,
    })
}

#[cfg(test)]
mod tests;
