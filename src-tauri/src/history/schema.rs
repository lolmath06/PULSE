//! The history database schema and its migrations.
//!
//! # Versioning
//!
//! The version lives in `schema_metadata` under `schema_version`. On open:
//!
//! - **no `schema_metadata` table** — a new, empty file: every migration runs;
//! - **an older version** — only the missing migrations run, each in its own
//!   transaction together with the version bump, so a crash can never leave a
//!   half-migrated file claiming a version it does not have;
//! - **a newer version** — [`HistoryError::FutureSchema`]. The file was written
//!   by a later PULSE and is **left untouched**: never migrated backwards,
//!   never truncated, never deleted. History is then unavailable and the live
//!   monitor carries on.
//!
//! # Shape
//!
//! Metadata such as a metric's unit or display name is **never** repeated per
//! row: it belongs to the metric catalog. A sample row is three numbers —
//! series, timestamp, value — and the series table maps a series number to its
//! `(metric key, source digest)` identity once.

use rusqlite::{Connection, OptionalExtension};

use super::error::HistoryError;

/// The schema this build writes and reads.
pub const SCHEMA_VERSION: u32 = 1;

/// Version 1: series, batches, raw samples and one-minute aggregates.
///
/// `WITHOUT ROWID` with a `(series_id, timestamp)` primary key makes the
/// primary key *be* the index every query and every compaction walks, stores
/// each row once instead of twice, and makes a duplicate sample for the same
/// series at the same instant impossible by construction.
const V1: &str = "
CREATE TABLE schema_metadata (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE metric_series (
    series_id     INTEGER PRIMARY KEY,
    metric_key    TEXT NOT NULL,
    source_digest TEXT NOT NULL,
    UNIQUE (metric_key, source_digest)
);

CREATE TABLE sample_batches (
    batch_id     INTEGER PRIMARY KEY,
    timestamp_ms INTEGER NOT NULL,
    row_count    INTEGER NOT NULL
);
CREATE INDEX sample_batches_by_time ON sample_batches (timestamp_ms);

CREATE TABLE raw_metric_samples (
    series_id    INTEGER NOT NULL REFERENCES metric_series (series_id),
    timestamp_ms INTEGER NOT NULL,
    value        REAL NOT NULL,
    PRIMARY KEY (series_id, timestamp_ms)
) WITHOUT ROWID;

CREATE TABLE aggregate_metric_samples (
    series_id       INTEGER NOT NULL REFERENCES metric_series (series_id),
    bucket_start_ms INTEGER NOT NULL,
    bucket_ms       INTEGER NOT NULL,
    min_value       REAL NOT NULL,
    max_value       REAL NOT NULL,
    avg_value       REAL NOT NULL,
    sample_count    INTEGER NOT NULL,
    PRIMARY KEY (series_id, bucket_start_ms)
) WITHOUT ROWID;
";

/// Every migration, in order. `(version it produces, SQL)`.
const MIGRATIONS: &[(u32, &str)] = &[(1, V1)];

/// Reads the stored schema version, or `None` for a file PULSE never wrote to.
pub fn read_version(connection: &Connection) -> Result<Option<u32>, HistoryError> {
    let has_metadata: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_metadata')",
        [],
        |row| row.get(0),
    )?;
    if !has_metadata {
        return Ok(None);
    }

    let stored: Option<String> = connection
        .query_row(
            "SELECT value FROM schema_metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    match stored {
        None => Ok(None),
        Some(text) => text
            .trim()
            .parse::<u32>()
            .map(Some)
            .map_err(|_| HistoryError::InvalidSchema(format!("schema_version is '{text}'"))),
    }
}

/// Brings the database up to [`SCHEMA_VERSION`], or refuses a newer one.
///
/// Returns the version the file is at afterwards.
pub fn migrate(connection: &mut Connection) -> Result<u32, HistoryError> {
    let current = read_version(connection)?.unwrap_or(0);

    if current > SCHEMA_VERSION {
        return Err(HistoryError::FutureSchema {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }

    for (version, sql) in MIGRATIONS {
        if *version <= current {
            continue;
        }

        let transaction = connection.transaction()?;
        transaction.execute_batch(sql)?;
        transaction.execute(
            "INSERT INTO schema_metadata (key, value) VALUES ('schema_version', ?1)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [version.to_string()],
        )?;
        transaction.commit()?;
    }

    Ok(SCHEMA_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables(connection: &Connection) -> Vec<String> {
        let mut statement = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("prepare");
        let names = statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows");
        names
    }

    #[test]
    fn an_empty_database_has_no_version() {
        let connection = Connection::open_in_memory().expect("open");
        assert_eq!(read_version(&connection).expect("read"), None);
    }

    #[test]
    fn migrating_an_empty_database_creates_version_one() {
        let mut connection = Connection::open_in_memory().expect("open");

        assert_eq!(migrate(&mut connection).expect("migrate"), 1);
        assert_eq!(read_version(&connection).expect("read"), Some(1));
        assert_eq!(
            tables(&connection),
            [
                "aggregate_metric_samples",
                "metric_series",
                "raw_metric_samples",
                "sample_batches",
                "schema_metadata",
            ]
        );
    }

    #[test]
    fn migrating_twice_is_a_no_op() {
        let mut connection = Connection::open_in_memory().expect("open");
        migrate(&mut connection).expect("first");
        migrate(&mut connection).expect("second");

        assert_eq!(read_version(&connection).expect("read"), Some(1));
    }

    #[test]
    fn a_future_schema_is_refused_and_left_untouched() {
        let mut connection = Connection::open_in_memory().expect("open");
        migrate(&mut connection).expect("migrate");
        connection
            .execute(
                "UPDATE schema_metadata SET value = '99' WHERE key = 'schema_version'",
                [],
            )
            .expect("bump");
        connection
            .execute("CREATE TABLE from_the_future (x INTEGER)", [])
            .expect("future table");

        let error = migrate(&mut connection).expect_err("must refuse");

        assert_eq!(
            error,
            HistoryError::FutureSchema {
                found: 99,
                supported: 1
            }
        );
        assert_eq!(read_version(&connection).expect("read"), Some(99));
        assert!(tables(&connection).contains(&"from_the_future".to_string()));
    }

    #[test]
    fn a_garbled_version_is_an_error_not_a_panic() {
        let mut connection = Connection::open_in_memory().expect("open");
        migrate(&mut connection).expect("migrate");
        connection
            .execute(
                "UPDATE schema_metadata SET value = 'banana' WHERE key = 'schema_version'",
                [],
            )
            .expect("garble");

        assert!(matches!(
            migrate(&mut connection),
            Err(HistoryError::InvalidSchema(_))
        ));
    }
}
