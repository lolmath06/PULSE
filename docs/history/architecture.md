# Metric History — Architecture

> Phase 10. How PULSE records what its metrics did, and why it is built the way
> it is.

## One scheduler, and only one

```text
MetricsEngine ──sample──► HistorySampler ──batch──► HistoryStore (SQLite)
      ▲                        ▲                         │
      │                  HistoryService thread           │ bounded queries
      │                  ("pulse-history")               ▼
 Refresh buttons                                  get_metric_history ──► charts
 (live only — never write)                                ▲
                                  history-sample-recorded ┘ (one event per batch)
```

| Piece            | File                               | Job                                                                                              |
| ---------------- | ---------------------------------- | ------------------------------------------------------------------------------------------------ |
| `HistoryService` | `src-tauri/src/history/service.rs` | The **only** timer that writes history: one thread, one tick per cadence, retention once an hour |
| `HistorySampler` | `history/sampler.rs`               | One tick: sample the historized metrics, keep finite numbers, write one batch                    |
| `TickSchedule`   | `history/sampler.rs`               | Monotonic, fixed-rate schedule that **never catches up** missed ticks                            |
| `HistoryStore`   | `history/store/`                   | SQLite file: schema, migrations, batch insert, bounded queries, compaction                       |
| `QueryPlan`      | `history/query.rs`                 | Range → bucket size, so no series exceeds ~720 points                                            |
| `selection`      | `history/selection.rs`             | The explicit allow-list of historized keys                                                       |
| `Clock`          | `history/clock.rs`                 | Injected wall clock (UTC ms); tests use `ManualClock`                                            |

Forbidden, and absent: a `setInterval` per chart, polling per widget, per
provider or per metric. The UI reloads a series **only** when the backend emits
`history-sample-recorded`, and only for panels that are on screen.

History is **not a provider**. The engine still has six providers; history is a
consumer of the engine, composed in `services/history.rs` and started in Tauri's
`setup` hook, because it needs the app's data directory.

The whole `history` module is tauri-free: the event sink is a trait
(`HistoryEventSink`) and the database path is an argument. That is what lets the
Windows harness type check all of it, and every test drive it with a
`ManualClock` instead of real seconds.

## Refresh never writes history

The Refresh buttons call `sample_metrics`, which reads the engine and returns.
They have no path to the store — there is no "write a sample" command at all.
A Rust test (`live_sampling_never_writes_history`) samples the same source
twenty-five times outside the scheduler and checks the database still holds one
batch; the Fedora runtime check confirmed zero duplicate batch timestamps while
every card sampled on mount.

## What a tick does

1. `engine.sample(historized refs)` — one call, each provider once.
2. Timestamp = wall clock **after** sampling, in UTC epoch milliseconds.
3. Keep samples that are `available` **and** numeric **and** finite.
   Unavailable is **not zero**: GPU telemetry missing, a sensor gone or a rate
   without its first interval writes **no row**, so the chart shows a gap.
4. One transaction for the whole batch (`insert_batch`).
5. Emit `history-sample-recorded { batchId, timestampMs, rowCount }` — never the
   values, never the catalog.

## Time

- **Stored:** UTC epoch milliseconds. Never local time, never a local `Date` as
  an identifier. The time zone is applied only when a label is displayed.
- **Scheduled:** the monotonic clock. A wall-clock step (NTP, a manual change)
  cannot make the scheduler burst or stall; it only shifts the timestamps of the
  next batches.
- **Sleep / hibernate:** the missed ticks are **not** fabricated. The schedule
  restarts from "now"; the chart shows the absence as a gap.
- **Backwards step:** a batch may carry an earlier timestamp than the previous
  one. It is stored as recorded; the chart breaks the line on any backwards step
  instead of folding it over itself; compaction merges late rows exactly.
- **DST:** does not exist in UTC.

## Gaps

A query answer carries `gapThresholdMs = 3 × bucket` (15 s for raw 5-second
data). The renderer starts a new line segment whenever two consecutive points
are further apart than that, or go backwards. Nothing is interpolated or drawn
across an absence. Test: `00, 05, 10, +2h, +2h05` → two segments (Rust and
frontend).

## Never per process

Only `process.count.total`, `process.count.running` and
`process.thread.count.total` on `process:system` are historized. A `process:`
source other than `process:system` is refused **twice**: by the selection and
again inside `insert_batch`, whatever its key. Tests:
`no_per_process_series_is_ever_historized`,
`a_per_process_series_is_refused_even_when_passed_directly`.

## Historized metrics

`history/selection.rs` — 25 keys, expanded per source by the live catalog:

| Family    | Keys                                                                                                                      |
| --------- | ------------------------------------------------------------------------------------------------------------------------- |
| CPU       | `cpu.usage.total`, `cpu.usage.logical` (per logical processor), `cpu.temperature.package`                                 |
| Memory    | `memory.used`, `memory.available`, `memory.usage.percent`                                                                 |
| GPU       | `gpu.usage.core`, `gpu.memory.used`, `gpu.memory.usage.percent`, `gpu.temperature.{core,hotspot,memory}`, `gpu.fan.speed` |
| Storage   | `storage.io.{read,write}.bytes_per_second`, `storage.io.{read,write}.iops`, `storage.volume.usage.percent`                |
| Network   | `network.{receive,transmit}.bytes_per_second`, `network.wifi.signal.{rssi,quality}`                                       |
| Processes | `process.count.total`, `process.count.running`, `process.thread.count.total`                                              |

Deliberately excluded: constants (`*.count`, totals, link speed, MTU),
`cpu.frequency.*` (would double the per-processor rows for a value that changes
far faster than 5 s), and **`storage.health.*` including drive temperature** — on
NVMe each read is an admin command, and one every five seconds keeps the SSD out
of its low-power states. Phase 10 also made the storage providers read health
**only** when a health key of that device is requested
(`wellknown::storage::requests_health`), so the I/O rates the history samples
every five seconds never wake the controller.

On the Fedora reference machine: 87 historized references, 79 of which produce
values (the rest are e.g. GPU usage without NVML — no rows, no zeros).

## Failure

- Database cannot open (read-only disk, a directory in the way, a newer
  schema): `HistoryService::unavailable(reason)`. No thread starts. The UI says
  _History unavailable: <reason>_; value/bar/gauge renderers fall back to one
  live sample. Live monitoring is untouched.
- A failed batch: logged, reported as `lastError`, the next tick tries again.
- A corrupt row: skipped and counted (`skippedRows`), never a panic.

## Shutdown

On `RunEvent::Exit` the service stops the thread (interrupting the wait, not
sitting it out), lets the batch in flight commit, and checkpoints the WAL
(`wal_checkpoint(TRUNCATE)`) so a closed PULSE leaves one self-contained file.

## Current value vs history

A history answer carries, per series, `latest`: the **last recorded sample** in
the window, exact even when the points are two-minute buckets. Every renderer's
_current_ is that value, so switching Area → Gauge → Value never changes the
number, and the header never contradicts the last point of the line. It can
differ from the live cards by up to one cadence (5 s) — they are two readings at
two instants, and the chart says when its own was taken (tooltip).
