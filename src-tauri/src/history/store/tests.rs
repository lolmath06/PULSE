use rusqlite::Connection;

use super::*;
use crate::history::query::{HistoryRange, QueryPlan};
use crate::history::testing::{cpu_total, memory_percent, metric, TempDir};

const T0: i64 = 1_800_000_000_000;
const CADENCE: i64 = 5_000;

fn raw_plan(from_ms: i64, to_ms: i64) -> QueryPlan {
    QueryPlan {
        from_ms,
        to_ms,
        bucket_ms: CADENCE,
        raw: true,
        gap_threshold_ms: 3 * CADENCE,
    }
}

fn bucketed_plan(from_ms: i64, to_ms: i64, bucket_ms: i64) -> QueryPlan {
    QueryPlan {
        from_ms,
        to_ms,
        bucket_ms,
        raw: false,
        gap_threshold_ms: 3 * bucket_ms,
    }
}

fn values(store: &HistoryStore, metric: &MetricRef, plan: &QueryPlan) -> Vec<(i64, f64)> {
    store
        .query(std::slice::from_ref(metric), plan)
        .expect("query")[0]
        .points
        .iter()
        .map(|point| (point.t, point.value))
        .collect()
}

fn side_connection(dir: &TempDir) -> Connection {
    let connection = Connection::open(dir.database()).expect("side connection");
    connection.busy_timeout(BUSY_TIMEOUT).expect("timeout");
    connection
}

#[test]
fn an_empty_database_answers_with_empty_series() {
    let dir = TempDir::new("empty");
    let store = HistoryStore::open(&dir.database()).expect("open");

    let series = store
        .query(&[cpu_total()], &raw_plan(T0 - 60_000, T0))
        .expect("query");

    assert_eq!(series.len(), 1);
    assert!(series[0].points.is_empty());
    assert_eq!(series[0].latest, None);
    assert_eq!(series[0].metric, cpu_total());
}

#[test]
fn opening_creates_the_file_its_directory_and_schema_v1_in_wal_mode() {
    let dir = TempDir::new("create");
    let nested = dir.path().join("a").join("b").join("history.sqlite3");

    let store = HistoryStore::open(&nested).expect("open");

    assert!(nested.exists());
    assert_eq!(store.journal_mode(), "wal");
    let stats = store.stats().expect("stats");
    assert_eq!(stats.schema_version, 1);
    assert_eq!(stats.journal_mode, "wal");
    assert_eq!(stats.synchronous, 1, "NORMAL");
    assert_eq!(stats.raw_rows, 0);
}

#[test]
fn one_batch_writes_every_metric_in_one_transaction() {
    let dir = TempDir::new("batch");
    let store = HistoryStore::open(&dir.database()).expect("open");

    let receipt = store
        .insert_batch(T0, &[(cpu_total(), 12.5), (memory_percent(), 40.0)])
        .expect("insert");

    assert_eq!(receipt.rows_written, 2);
    assert_eq!(receipt.timestamp_ms, T0);
    let stats = store.stats().expect("stats");
    assert_eq!(stats.raw_rows, 2);
    assert_eq!(stats.batch_count, 1);
    assert_eq!(stats.series_count, 2);
    assert_eq!(stats.average_rows_per_batch, Some(2.0));
}

#[test]
fn queries_return_points_in_time_order_per_metric() {
    let dir = TempDir::new("order");
    let store = HistoryStore::open(&dir.database()).expect("open");

    // Written out of order on purpose.
    for (offset, cpu) in [(10_000, 3.0), (0, 1.0), (5_000, 2.0)] {
        store
            .insert_batch(
                T0 + offset,
                &[(cpu_total(), cpu), (memory_percent(), cpu * 10.0)],
            )
            .expect("insert");
    }

    let plan = raw_plan(T0, T0 + 10_000);
    assert_eq!(
        values(&store, &cpu_total(), &plan),
        [(T0, 1.0), (T0 + 5_000, 2.0), (T0 + 10_000, 3.0)]
    );
    assert_eq!(
        values(&store, &memory_percent(), &plan),
        [(T0, 10.0), (T0 + 5_000, 20.0), (T0 + 10_000, 30.0)]
    );

    let series = store
        .query(&[memory_percent(), cpu_total()], &plan)
        .expect("query");
    assert_eq!(series[0].metric, memory_percent(), "request order is kept");
    assert_eq!(series[1].latest.map(|l| l.value), Some(3.0));
}

#[test]
fn the_window_bounds_are_honoured() {
    let dir = TempDir::new("window");
    let store = HistoryStore::open(&dir.database()).expect("open");
    for step in 0..10 {
        store
            .insert_batch(T0 + step * CADENCE, &[(cpu_total(), step as f64)])
            .expect("insert");
    }

    let plan = raw_plan(T0 + 2 * CADENCE, T0 + 4 * CADENCE);
    let got: Vec<f64> = values(&store, &cpu_total(), &plan)
        .into_iter()
        .map(|(_, v)| v)
        .collect();
    assert_eq!(got, [2.0, 3.0, 4.0]);
}

#[test]
fn replaying_a_batch_does_not_duplicate_it() {
    let dir = TempDir::new("dupe");
    let store = HistoryStore::open(&dir.database()).expect("open");

    store
        .insert_batch(T0, &[(cpu_total(), 1.0)])
        .expect("first");
    let replay = store
        .insert_batch(T0, &[(cpu_total(), 99.0)])
        .expect("replay");

    assert_eq!(replay.rows_written, 0);
    assert_eq!(values(&store, &cpu_total(), &raw_plan(T0, T0)), [(T0, 1.0)]);
}

#[test]
fn only_finite_values_are_written() {
    let dir = TempDir::new("finite");
    let store = HistoryStore::open(&dir.database()).expect("open");

    let receipt = store
        .insert_batch(
            T0,
            &[
                (cpu_total(), f64::NAN),
                (memory_percent(), f64::INFINITY),
                (metric("memory.used", "memory:system"), f64::NEG_INFINITY),
                (metric("memory.available", "memory:system"), 0.0),
            ],
        )
        .expect("insert");

    assert_eq!(receipt.rows_written, 1, "zero is a real reading");
    assert_eq!(receipt.skipped_non_finite, 3);
    assert!(values(&store, &cpu_total(), &raw_plan(T0, T0)).is_empty());
}

#[test]
fn a_per_process_series_is_refused_even_when_passed_directly() {
    let dir = TempDir::new("pid");
    let store = HistoryStore::open(&dir.database()).expect("open");

    let receipt = store
        .insert_batch(
            T0,
            &[
                (metric("process.count.total", "process:system"), 412.0),
                (metric("process.count.total", "process:12345-9001"), 1.0),
                (metric("process.cpu.usage", "process:12345-9001"), 50.0),
            ],
        )
        .expect("insert");

    assert_eq!(receipt.rows_written, 1);
    assert_eq!(receipt.skipped_not_historizable, 2);
    let series_count: i64 = side_connection(&dir)
        .query_row("SELECT COUNT(*) FROM metric_series", [], |row| row.get(0))
        .expect("count");
    assert_eq!(series_count, 1);
}

#[test]
fn history_survives_closing_and_reopening_the_database() {
    let dir = TempDir::new("reopen");
    {
        let store = HistoryStore::open(&dir.database()).expect("open");
        store
            .insert_batch(T0, &[(cpu_total(), 7.0)])
            .expect("insert");
    }

    let store = HistoryStore::open(&dir.database()).expect("reopen");
    store
        .insert_batch(T0 + CADENCE, &[(cpu_total(), 8.0)])
        .expect("continue");

    assert_eq!(
        values(&store, &cpu_total(), &raw_plan(T0, T0 + CADENCE)),
        [(T0, 7.0), (T0 + CADENCE, 8.0)]
    );
    assert_eq!(
        store.stats().expect("stats").series_count,
        1,
        "series reused"
    );
}

#[test]
fn a_failed_batch_is_rolled_back_entirely() {
    let dir = TempDir::new("rollback");
    let store = HistoryStore::open(&dir.database()).expect("open");
    side_connection(&dir)
        .execute_batch(
            "CREATE TRIGGER poison BEFORE INSERT ON raw_metric_samples
             WHEN NEW.value = 666 BEGIN SELECT RAISE(ABORT, 'poisoned'); END;",
        )
        .expect("trigger");

    let result = store.insert_batch(T0, &[(cpu_total(), 1.0), (memory_percent(), 666.0)]);

    assert!(result.is_err());
    let stats = store.stats().expect("stats");
    assert_eq!(stats.raw_rows, 0, "the first row was rolled back too");
    assert_eq!(stats.batch_count, 0);
    assert_eq!(stats.series_count, 0, "no series survived the rollback");

    // And the writer's cache was not poisoned by the rolled-back ids.
    side_connection(&dir)
        .execute_batch("DROP TRIGGER poison")
        .expect("drop trigger");
    let receipt = store
        .insert_batch(T0, &[(cpu_total(), 1.0), (memory_percent(), 2.0)])
        .expect("retry");
    assert_eq!(receipt.rows_written, 2);
}

#[test]
fn a_future_schema_is_refused_and_the_file_is_not_modified() {
    let dir = TempDir::new("future");
    {
        let store = HistoryStore::open(&dir.database()).expect("open");
        store
            .insert_batch(T0, &[(cpu_total(), 1.0)])
            .expect("insert");
    }
    side_connection(&dir)
        .execute(
            "UPDATE schema_metadata SET value = '2' WHERE key = 'schema_version'",
            [],
        )
        .expect("bump");
    let before = std::fs::read(dir.database()).expect("read");

    let error = HistoryStore::open(&dir.database()).expect_err("must refuse");

    assert_eq!(
        error,
        HistoryError::FutureSchema {
            found: 2,
            supported: 1
        }
    );
    assert!(dir.database().exists(), "never deleted");
    assert_eq!(std::fs::read(dir.database()).expect("read"), before);
}

#[test]
fn a_file_that_is_not_a_database_is_an_error_and_is_left_alone() {
    let dir = TempDir::new("garbage");
    std::fs::write(
        dir.database(),
        b"definitely not sqlite, just some bytes....",
    )
    .expect("write");

    assert!(HistoryStore::open(&dir.database()).is_err());
    assert_eq!(
        std::fs::read(dir.database()).expect("read"),
        b"definitely not sqlite, just some bytes...."
    );
}

#[test]
fn a_corrupt_row_is_skipped_and_counted_not_fatal() {
    let dir = TempDir::new("corrupt");
    let store = HistoryStore::open(&dir.database()).expect("open");
    store
        .insert_batch(T0, &[(cpu_total(), 1.0)])
        .expect("insert");
    store
        .insert_batch(T0 + 2 * CADENCE, &[(cpu_total(), 3.0)])
        .expect("insert");
    side_connection(&dir)
        .execute(
            "INSERT INTO raw_metric_samples (series_id, timestamp_ms, value)
             SELECT series_id, ?1, 'garbage' FROM metric_series",
            [T0 + CADENCE],
        )
        .expect("corrupt");

    for plan in [
        raw_plan(T0, T0 + 2 * CADENCE),
        bucketed_plan(T0, T0 + 2 * CADENCE, 5_000),
    ] {
        let series = &store.query(&[cpu_total()], &plan).expect("query")[0];
        assert_eq!(series.points.len(), 2, "plan {plan:?}");
        assert_eq!(series.skipped_rows, 1);
    }
}

#[test]
fn bucketed_queries_keep_min_max_average_and_count() {
    let dir = TempDir::new("bucket");
    let store = HistoryStore::open(&dir.database()).expect("open");
    // One 30-second bucket: a brief spike to 100 among low readings.
    for (step, value) in [10.0, 12.0, 100.0, 14.0, 9.0, 11.0].iter().enumerate() {
        store
            .insert_batch(T0 + step as i64 * CADENCE, &[(cpu_total(), *value)])
            .expect("insert");
    }

    let series = &store
        .query(&[cpu_total()], &bucketed_plan(T0, T0 + 29_999, 30_000))
        .expect("query")[0];

    assert_eq!(series.points.len(), 1);
    let point = series.points[0];
    assert_eq!(point.t, T0 - T0.rem_euclid(30_000));
    assert_eq!(point.min, Some(9.0));
    assert_eq!(point.max, Some(100.0), "the spike survives");
    assert_eq!(point.count, Some(6));
    assert!((point.value - 156.0 / 6.0).abs() < 1e-9);
    assert_eq!(
        series.latest.map(|l| l.value),
        Some(11.0),
        "latest is exact"
    );
}

#[test]
fn gaps_are_left_empty_never_filled() {
    let dir = TempDir::new("gaps");
    let store = HistoryStore::open(&dir.database()).expect("open");
    let two_hours = 2 * 3_600_000;
    let times = [
        T0,
        T0 + 5_000,
        T0 + 10_000,
        T0 + 10_000 + two_hours,
        T0 + 15_000 + two_hours,
    ];
    for t in times {
        store
            .insert_batch(t, &[(cpu_total(), 5.0)])
            .expect("insert");
    }

    let got: Vec<i64> = values(&store, &cpu_total(), &raw_plan(T0, T0 + 3 * 3_600_000))
        .into_iter()
        .map(|(t, _)| t)
        .collect();

    assert_eq!(got, times, "exactly what was recorded, nothing in between");
}

fn count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("count")
}

#[test]
fn compaction_folds_old_raw_rows_into_minute_aggregates() {
    let dir = TempDir::new("compact");
    let store = HistoryStore::open(&dir.database()).expect("open");
    let policy = RetentionPolicy::DEFAULT;
    let minute = T0 - T0.rem_euclid(60_000);

    // Two full minutes of 5-second samples, then "now" is a day and a bit later.
    for step in 0..24 {
        let value = if step == 7 { 95.0 } else { 20.0 + step as f64 };
        store
            .insert_batch(minute + step * CADENCE, &[(cpu_total(), value)])
            .expect("insert");
    }
    let now = minute + policy.raw_ms + 3 * 60_000;

    let report = store.compact(now, &policy).expect("compact");

    assert_eq!(report.aggregates_written, 2);
    assert_eq!(report.raw_rows_deleted, 24);
    assert_eq!(report.batches_deleted, 24);
    let side = side_connection(&dir);
    assert_eq!(count(&side, "raw_metric_samples"), 0);

    let (min, max, avg, n): (f64, f64, f64, i64) = side
        .query_row(
            "SELECT min_value, max_value, avg_value, sample_count
             FROM aggregate_metric_samples ORDER BY bucket_start_ms LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("first bucket");
    assert_eq!(n, 12);
    assert_eq!(min, 20.0);
    assert_eq!(max, 95.0, "the peak survives compaction");
    let expected: f64 = (0..12)
        .map(|step| if step == 7 { 95.0 } else { 20.0 + step as f64 })
        .sum::<f64>()
        / 12.0;
    assert!((avg - expected).abs() < 1e-9);

    // The compacted history still answers a week-long query.
    let plan = QueryPlan::new(HistoryRange::SevenDays, now, CADENCE);
    let series = &store.query(&[cpu_total()], &plan).expect("query")[0];
    assert_eq!(
        series.points.len(),
        1,
        "both minutes fall in one 15-min bucket"
    );
    assert_eq!(series.points[0].max, Some(95.0));
    assert_eq!(series.points[0].count, Some(24));
}

#[test]
fn compaction_keeps_recent_raw_rows_and_expires_old_aggregates() {
    let dir = TempDir::new("expire");
    let store = HistoryStore::open(&dir.database()).expect("open");
    let policy = RetentionPolicy::DEFAULT;
    let now = T0;

    let ancient = now - policy.aggregate_ms - 3_600_000;
    let old = now - policy.raw_ms - 3_600_000;
    let recent = now - 60_000;
    for t in [ancient, old, recent] {
        store
            .insert_batch(t, &[(cpu_total(), 1.0)])
            .expect("insert");
    }

    store.compact(now, &policy).expect("first pass");
    // The ancient row became an aggregate — which is itself past retention.
    let side = side_connection(&dir);
    assert_eq!(count(&side, "raw_metric_samples"), 1, "recent raw row kept");
    assert_eq!(
        count(&side, "aggregate_metric_samples"),
        1,
        "only `old` kept"
    );

    // Idempotent: a second pass changes nothing.
    let again = store.compact(now, &policy).expect("second pass");
    assert_eq!(again, CompactionReport::default());
}

#[test]
fn compaction_merges_into_an_existing_bucket_exactly() {
    let dir = TempDir::new("merge");
    let store = HistoryStore::open(&dir.database()).expect("open");
    let policy = RetentionPolicy::DEFAULT;
    let minute = T0 - T0.rem_euclid(60_000);
    let now = minute + policy.raw_ms + 120_000;

    store
        .insert_batch(minute, &[(cpu_total(), 10.0)])
        .expect("a");
    store.compact(now, &policy).expect("first");
    // A late row for the same minute — e.g. after a backwards clock step.
    store
        .insert_batch(minute + 30_000, &[(cpu_total(), 30.0)])
        .expect("b");
    store.compact(now, &policy).expect("second");

    let (min, max, avg, n): (f64, f64, f64, i64) = side_connection(&dir)
        .query_row(
            "SELECT min_value, max_value, avg_value, sample_count FROM aggregate_metric_samples",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("bucket");
    assert_eq!((min, max, n), (10.0, 30.0, 2));
    assert!((avg - 20.0).abs() < 1e-9);
}

#[test]
fn raw_rows_are_deleted_only_when_their_aggregate_was_written() {
    let dir = TempDir::new("compact-rollback");
    let store = HistoryStore::open(&dir.database()).expect("open");
    let policy = RetentionPolicy::DEFAULT;
    store
        .insert_batch(T0, &[(cpu_total(), 1.0)])
        .expect("insert");
    side_connection(&dir)
        .execute_batch(
            "CREATE TRIGGER no_aggregates BEFORE INSERT ON aggregate_metric_samples
             BEGIN SELECT RAISE(ABORT, 'disk full, say'); END;",
        )
        .expect("trigger");

    let result = store.compact(T0 + policy.raw_ms + 120_000, &policy);

    assert!(result.is_err());
    assert_eq!(
        count(&side_connection(&dir), "raw_metric_samples"),
        1,
        "the raw row survives a failed aggregation"
    );
}

#[test]
fn the_database_holds_no_source_identifier_in_clear() {
    let dir = TempDir::new("privacy");
    let store = HistoryStore::open(&dir.database()).expect("open");
    let wifi = metric(
        "network.receive.bytes_per_second",
        "network:mac-9009df3e97f2",
    );
    let disk = metric(
        "storage.io.read.bytes_per_second",
        "storage:serial-s677nx0w",
    );
    store
        .insert_batch(T0, &[(wifi.clone(), 1.0), (disk.clone(), 2.0)])
        .expect("insert");

    let digests: Vec<String> = {
        let side = side_connection(&dir);
        let mut statement = side
            .prepare("SELECT source_digest FROM metric_series ORDER BY series_id")
            .expect("prepare");
        let rows = statement
            .query_map([], |row| row.get(0))
            .expect("query")
            .collect::<Result<Vec<String>, _>>()
            .expect("rows");
        rows
    };
    assert_eq!(digests.len(), 2);
    for digest in &digests {
        assert!(!digest.contains("9009df3e97f2"));
        assert!(!digest.contains("s677nx0w"));
    }
    assert!(digests[0].starts_with("network:"));
    assert!(digests[1].starts_with("storage:"));

    drop(store);
    let bytes = std::fs::read(dir.database()).expect("read");
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("9009df3e97f2"));
    assert!(!text.contains("s677nx0w"));

    // And the digest still resolves the right series.
    let store = HistoryStore::open(&dir.database()).expect("reopen");
    assert_eq!(values(&store, &wifi, &raw_plan(T0, T0)), [(T0, 1.0)]);
}

#[test]
fn source_digests_are_stable_and_distinct() {
    let a = SourceId::new("network:mac-001122334455").expect("valid");
    let b = SourceId::new("network:mac-001122334456").expect("valid");

    assert_eq!(source_digest(&a), source_digest(&a));
    assert_ne!(source_digest(&a), source_digest(&b));
    assert_eq!(source_digest(&a).len(), "network:".len() + 16);
}

#[test]
fn a_reader_sees_series_the_writer_creates_later() {
    let dir = TempDir::new("late-series");
    let store = HistoryStore::open(&dir.database()).expect("open");
    let plan = raw_plan(T0, T0 + CADENCE);

    assert!(values(&store, &cpu_total(), &plan).is_empty());
    store
        .insert_batch(T0 + CADENCE, &[(cpu_total(), 4.0)])
        .expect("insert");

    assert_eq!(values(&store, &cpu_total(), &plan), [(T0 + CADENCE, 4.0)]);
}

/// Not a correctness test: prints the timings quoted in the Phase 10 report.
///
/// `cargo test --manifest-path src-tauri/Cargo.toml history_performance -- --ignored --nocapture`
#[test]
#[ignore]
fn history_performance_report() {
    use std::time::Instant;

    let dir = TempDir::new("perf");
    let store = HistoryStore::open(&dir.database()).expect("open");
    let metrics: Vec<MetricRef> = (0..100)
        .map(|i| metric("cpu.usage.logical", &format!("cpu:logical-{i}")))
        .collect();
    let now = T0;
    let start = now - 86_400_000;

    let mut insert = Vec::new();
    let mut t = start;
    let mut step = 0_u64;
    while t <= now {
        let batch: Vec<(MetricRef, f64)> = metrics
            .iter()
            .enumerate()
            .map(|(i, m)| (m.clone(), ((step + i as u64) % 100) as f64))
            .collect();
        let began = Instant::now();
        store.insert_batch(t, &batch).expect("insert");
        insert.push(began.elapsed());
        t += CADENCE;
        step += 1;
    }
    insert.sort();
    println!(
        "synthetic: {} batches x 100 metrics; insert median {:?}, p99 {:?}, max {:?}",
        insert.len(),
        insert[insert.len() / 2],
        insert[insert.len() * 99 / 100],
        insert[insert.len() - 1]
    );

    for range in [
        HistoryRange::FifteenMinutes,
        HistoryRange::OneHour,
        HistoryRange::SixHours,
        HistoryRange::OneDay,
    ] {
        let plan = QueryPlan::new(range, now, CADENCE);
        let began = Instant::now();
        let series = store.query(&metrics[..1], &plan).expect("query");
        println!(
            "synthetic: {range:?} one series -> {} points in {:?}",
            series[0].points.len(),
            began.elapsed()
        );
    }

    let stats = store.stats().expect("stats");
    println!(
        "synthetic: {} raw rows, file {} B + wal {} B, {:.1} B/row",
        stats.raw_rows,
        stats.file_bytes,
        stats.wal_bytes,
        (stats.file_bytes + stats.wal_bytes) as f64 / stats.raw_rows as f64
    );

    let began = Instant::now();
    let report = store
        .compact(now + 3_600_000, &RetentionPolicy::DEFAULT)
        .expect("compact");
    println!("synthetic: compaction {report:?} in {:?}", began.elapsed());
}

#[test]
fn a_checkpoint_leaves_an_empty_wal_and_keeps_the_data() {
    let dir = TempDir::new("checkpoint");
    let store = HistoryStore::open(&dir.database()).expect("open");
    store
        .insert_batch(T0, &[(cpu_total(), 3.0)])
        .expect("insert");

    store.checkpoint().expect("checkpoint");

    let stats = store.stats().expect("stats");
    assert_eq!(stats.wal_bytes, 0);
    assert_eq!(stats.raw_rows, 1);
}
