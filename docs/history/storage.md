# Metric History — Storage

## Technology

SQLite through [`rusqlite`](https://crates.io/crates/rusqlite) **0.32.1**
(`libsqlite3-sys` 0.30.1, **SQLite 3.46.0**) with the
**`bundled`** feature: SQLite's amalgamation is compiled into PULSE, so Fedora
needs no `libsqlite3` package and Windows no `sqlite3.dll`, and both run the same
SQLite version. No server, no network.

### Why 0.32 and not the latest

PULSE's minimum Rust is 1.77.2, and that is a promise about the whole locked
dependency graph. rusqlite 0.32.1 is the newest line that really builds on
1.77.2: `libsqlite3-sys` 0.31+ uses `#[expect]` (Rust 1.81) and 0.38 uses
`cfg_select!`. Neither declares a `rust-version`, so this was established by
building each candidate (0.32 … 0.39) with the 1.77.2 toolchain. Everything
PULSE needs — WAL, `WITHOUT ROWID`, UPSERT, `prepare_cached`, read-only
connections — is in SQLite 3.46. See [`../development/msrv.md`](../development/msrv.md).

## Location

`<app local data dir>/history.sqlite3`, resolved by Tauri
(`app.path().app_local_data_dir()`), never from the working directory or a
hard-coded home:

| Platform | Path                                                                                                              |
| -------- | ----------------------------------------------------------------------------------------------------------------- |
| Fedora   | `$XDG_DATA_HOME/dev.pulse.app/history.sqlite3` — measured: `~/.local/share/dev.pulse.app/history.sqlite3`         |
| Windows  | `%LOCALAPPDATA%\dev.pulse.app\history.sqlite3` (local, not roaming: a history does not belong on another machine) |

## Schema (version 1)

```sql
schema_metadata          (key TEXT PK, value TEXT)            -- schema_version = 1
metric_series            (series_id INTEGER PK,
                          metric_key TEXT, source_digest TEXT,
                          UNIQUE (metric_key, source_digest))
sample_batches           (batch_id INTEGER PK, timestamp_ms, row_count)
                          + INDEX (timestamp_ms)
raw_metric_samples       (series_id, timestamp_ms, value REAL,
                          PRIMARY KEY (series_id, timestamp_ms)) WITHOUT ROWID
aggregate_metric_samples (series_id, bucket_start_ms, bucket_ms,
                          min_value, max_value, avg_value, sample_count,
                          PRIMARY KEY (series_id, bucket_start_ms)) WITHOUT ROWID
```

- A sample row is **three numbers**. Unit, display name and source label come
  from the metric catalog and are never repeated per row.
- `WITHOUT ROWID` + `PRIMARY KEY (series, timestamp)`: the key _is_ the
  `(metric, timestamp)` index, each row is stored once, and a duplicate sample
  for the same series and instant is impossible (`INSERT OR IGNORE`).

## Migrations

`history/schema.rs` holds an ordered list of `(version, SQL)`. On open:

- no `schema_metadata` → all migrations;
- older → only the missing ones, **each in its own transaction with the version
  bump**;
- **newer** → `FutureSchema` error. The file is checked **before any write,
  even a pragma**, and is never migrated backwards, truncated or deleted
  (test: `a_future_schema_is_refused_and_the_file_is_not_modified`, byte-for-byte).
- A file that is not a database → error, file untouched.

## SQLite settings

| Setting              | Value    | Why                                                                                                        |
| -------------------- | -------- | ---------------------------------------------------------------------------------------------------------- |
| `journal_mode`       | `WAL`    | the writer and the query connection never block each other                                                 |
| `synchronous`        | `NORMAL` | one fsync per checkpoint, not per batch; a power cut may lose the last few batches, never corrupt the file |
| `busy_timeout`       | 2000 ms  | a rare lock wait is retried rather than failing a batch                                                    |
| `journal_size_limit` | 16 MiB   | the WAL is truncated back after checkpoints                                                                |
| `foreign_keys`       | `ON`     | a sample can only reference an existing series                                                             |

## Concurrency

Two connections: a **writer** (batches, compaction) and a **read-only reader**
(queries, stats), each behind its own short-lived mutex. With WAL a week-long
query does not delay the next batch. Tauri commands run queries in
`spawn_blocking`, never on the IPC thread.

## Batches

One transaction per batch (≈ 79 rows on the reference machine), prepared cached
statements, series ids cached **only after commit** — a rolled-back batch cannot
leave a phantom id behind (test: `a_failed_batch_is_rolled_back_entirely`,
which injects a failure mid-batch with a trigger).

## Privacy

The file contains metric keys, timestamps and numbers. **Nothing else.**

A source identifier can be derived from hardware (`network:mac-…`,
`storage:serial-…`), so the series table stores a **digest** instead:
`kind:` + 64 bits of SHA-256 over a fixed, versioned prefix and the full
identifier — `network:3fa1…`, never the MAC. This is pseudonymisation: it keeps
identifiers out of the file; someone with a candidate identifier can still
confirm a match. Tested on the raw file bytes
(`the_database_holds_no_source_identifier_in_clear`) and checked on the real
Fedora file with `strings` (no MAC, serial, interface name, `nvme`, or user
name). No command line, path, user name, SSID, BSSID, IP, host name, file name
or URL is ever written.

## Measured footprint

**Real (Fedora, debug build, 79 rows/batch):** 3 634 raw rows → 126 976 bytes
after checkpoint (≈ 35 B/row including fixed page overhead of a small file).

**Synthetic (100 series × 17 281 batches = 1 728 100 rows, release build):**
29 605 888 B + 4 692 712 B WAL → **19.8 B/row** at scale.

Extrapolation for this machine:

```text
raw, 24 h   = 79 rows/batch × 17 280 batches/day × 19.8 B ≈ 27 MB
aggregates  = 79 series × 1 440 buckets/day × 6 days × ~40 B ≈ 27 MB   (estimate)
steady state (7 days)                                     ≈ 55 MB
```

The aggregate row size is an estimate (5 numbers + key); it was not measured on
a week of real data. Freed pages are reused, so the file stops growing once
retention reaches steady state; PULSE does not `VACUUM`.
