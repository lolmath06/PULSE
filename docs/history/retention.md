# Metric History — Retention & Resolution

## Policy

```text
 now ───────── 24 h ─────────────────────── 7 d ────►
 │ raw samples, every 5 s   │ 1-minute aggregates  │ deleted
 │ exact values             │ min · max · avg · n  │
```

`history/retention.rs`, `RetentionPolicy::DEFAULT`:

| Rule             | Value                                            |
| ---------------- | ------------------------------------------------ |
| Raw kept         | 24 hours                                         |
| Aggregate bucket | 1 minute                                         |
| Aggregates kept  | 7 days                                           |
| Compaction runs  | at startup, then every hour — **never** per tick |

## Compaction

Per series, in **one transaction each**:

1. `INSERT … SELECT` raw rows older than the cutoff into one-minute buckets,
   keeping **min, max, average and count** — never the average alone, so a
   one-sample spike to 100 % survives as the bucket's `max`;
   `ON CONFLICT` merges into an existing bucket exactly (weighted average,
   min of mins, max of maxes, sum of counts).
2. `DELETE` those raw rows — **in the same transaction**, so a raw row is
   removed only if its aggregate was written (test:
   `raw_rows_are_deleted_only_when_their_aggregate_was_written`, with a trigger
   that makes the aggregate insert fail).
3. `DELETE` aggregates older than 7 days.

The raw cutoff is aligned down to a minute, so a minute is never split between
raw and aggregated halves. A failure stops the pass and leaves that series
untouched; the next pass resumes. A second pass is a no-op (tested).

Synthetic cost: compacting 72 000 raw rows of 100 series took 63 ms (release).

## Query resolution

`history/query.rs`: bucket = the smallest "nice" size ≥ max(cadence,
range / 720), so **no series ever exceeds ~720 points**, whatever the cadence
(tested for 1, 5, 10 and 30 s).

| Range | Bucket (5 s cadence) | Points at most | Source                                   |
| ----- | -------------------- | -------------- | ---------------------------------------- |
| 15 m  | raw 5 s              | 180            | raw, exact timestamps                    |
| 1 h   | raw 5 s              | 720            | raw, exact timestamps                    |
| 6 h   | 30 s                 | 720            | raw, bucketed                            |
| 24 h  | 2 min                | 720            | raw (and the oldest minutes' aggregates) |
| 7 d   | 15 min               | 672            | aggregates + raw                         |

A bucketed point carries `v` (sample-weighted average), `min`, `max` and `n`.
Raw rows and stored aggregates are merged into the same buckets. Both are read
row by row, so a corrupt value is skipped and counted instead of being coerced
to zero by `SUM`.

## Measured query times

| Query                  | Real (Fedora, debug, ~46 batches) | Synthetic (release, 1.7 M rows) |
| ---------------------- | --------------------------------- | ------------------------------- |
| 15 m, one series       | UI-driven, not isolated           | 0.15 ms (181 points)            |
| 1 h, one series        | —                                 | 0.08 ms (721 points)            |
| 6 h, one series        | —                                 | 0.51 ms (721 points)            |
| 24 h, one series       | —                                 | 1.9 ms (721 points)             |
| Batch insert, 100 rows | 0.31 ms median (79 rows)          | 0.22 ms median, 0.50 ms max     |

Reproduce the synthetic figures:

```bash
cargo test --release --manifest-path src-tauri/Cargo.toml history_performance -- --ignored --nocapture
```
