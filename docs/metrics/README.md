# PULSE Metrics Engine

> **Phase 3 — detailed CPU metrics.**
>
> The model, the provider contract, the engine and the frontend API were built
> in Phase 1. Phase 2 added the first native collectors: CPU usage and physical
> memory, on both Fedora and Windows. Phase 3 made the CPU support real —
> per-logical-processor usage and frequency, plus topology — so the catalog is
> now **sized by the machine** (`8 + 3N` for `N` logical processors) rather than
> being a fixed list of five. Still two providers per platform.

## Documents

| Document                             | Contents                                                                                |
| ------------------------------------ | --------------------------------------------------------------------------------------- |
| [`model.md`](model.md)               | Definition, sample, value, unit, kind, category, availability, errors, contract version |
| [`identifiers.md`](identifiers.md)   | `MetricKey`, `SourceId`, stability rules, why labels are not identifiers                |
| [`providers.md`](providers.md)       | The provider contract, registration, collisions, isolation                              |
| [`cpu-memory.md`](cpu-memory.md)     | Phase 2: aggregate CPU usage and physical memory                                        |
| [`cpu-advanced.md`](cpu-advanced.md) | Phase 3: per-logical-processor usage and frequency, CPU topology, processor groups      |

## Position in the architecture

```text
React / UI
    │  components and hooks
Metrics frontend service          src/services/metrics.ts
    │  Tauri commands
Tauri command surface             src-tauri/src/commands/metrics.rs
    │
MetricsEngine                     src-tauri/src/metrics/engine/
    ├── Provider A ───┐
    ├── Provider B ───┼──► platform/ ──► /proc, /sys, hwmon, PDH, WMI, NVML…
    └── Provider C ───┘
```

The engine contains **no platform code**. It knows about providers, a catalog
and references; where a number comes from is entirely the provider's business.
This is verified: `grep cfg(target_os)` over `src-tauri/src/metrics/` matches
nothing but documentation examples, and the whole module cross-compiles for
`x86_64-pc-windows-msvc`.

## Principles

### 1. Pull once, fan out

A metric is sampled once per request regardless of how many widgets show it.
Ten CPU widgets cost one `/proc/stat` read. The engine deduplicates references
and calls each provider at most once per request.

The memory provider goes further: all four `memory.*` metrics come from a
**single** read, so they are always mutually consistent — there is no torn read
where `used` and `available` come from different moments and fail to add up.

The CPU provider does the same at a larger scale: **one `/proc/stat` read feeds
`cpu.usage.total` and every `cpu.usage.logical`**, so the per-processor figures
reconcile with the aggregate instead of being sampled at slightly different
instants. On Windows, one call per processor group covers every logical
processor, and one `CallNtPowerInformation` returns the whole frequency array.

### 2. Sample only what is asked for

The engine never sweeps the catalog. A dashboard showing five metrics samples
five metrics, even when three hundred are registered. This is what will make
Gaming mode and the Mini overlay genuinely cheap rather than nominally cheap.

Resolution is `HashMap`-indexed (`MetricRef → provider`), so adding providers
does not slow down lookups.

### 3. Sampling lives in Rust

Not in the webview. The Mini overlay and the future tray must keep producing
data while the main window is hidden, and history must survive it.

### 4. Availability is a first-class state

A missing sensor is **normal**, especially for temperatures. Seven distinct
states keep "your machine has no such sensor" apart from "PULSE needs
elevation" and "the driver hiccupped". Showing `0 °C` for an absent sensor is a
bug, not a fallback. See [`model.md`](model.md#availability--why-it-has-seven-variants).

### 5. Canonical units in, display units out

Hertz, bytes, Celsius on the wire. The frontend renders GHz, GB and °F. See
[`model.md`](model.md#units--the-canonical-rule).

### 6. Failures are isolated

One provider failing marks only its own metrics. Every other provider's samples
are still returned.

### 7. Both platforms or it is not done

> A system-facing feature is not considered complete until its behavior on both
> Windows and Fedora Linux has been designed and, whenever materially testable,
> validated.

A collector that exists only for one OS is incomplete. The other platform must
at minimum declare the metric with an explicit `unsupported` availability and a
reason.

## Frontend API

| Command                     | Returns                                                            |
| --------------------------- | ------------------------------------------------------------------ |
| `get_metrics_engine_status` | `EngineStatus` — schema version, state, provider and metric counts |
| `get_metric_catalog`        | `MetricDefinition[]`, ordered by `(key, sourceId)`                 |
| `sample_metrics(metrics)`   | `MetricSample[]`, one per requested reference, in order            |

Called through `src/services/metrics.ts`. Types in `src/types/metrics.ts`.

## Deliberately still absent

No scheduler, no polling loop, no history, no ring buffer, no SQLite, no
adaptive sampling, no subscriptions, no events. The interaction model remains
`request → sample → response`, and the UI refreshes on demand.

This is why a first CPU sample may report `temporarilyUnavailable`: usage is a
rate, and without a sampler there may be no usable delta yet. PULSE says so
rather than reporting `0%`.

Planned for later phases:

- **Scheduler** — cadence per metric, driven by subscriptions and by the active
  mode's budget, with declared sampling costs so expensive metrics run rarely.
- **History** — a bounded ring buffer per metric in Rust, with downsampling.
- **Transport** — Tauri events for streaming samples, so the UI stops polling.

## Planned metric families

| Family        | Fedora source              | Windows source                                 | Notes                           |
| ------------- | -------------------------- | ---------------------------------------------- | ------------------------------- |
| CPU load      | `/proc/stat`               | `GetSystemTimes`, `NtQuerySystemInformationEx` | Delta-based; **done**           |
| CPU frequency | `cpufreq/scaling_cur_freq` | `CallNtPowerInformation`                       | Per logical processor; **done** |
| CPU topology  | `cpuN/topology/`           | `GetLogicalProcessorInformationEx`             | **done**                        |
| Memory        | `/proc/meminfo`            | `GlobalMemoryStatusEx`                         | Use `MemAvailable`              |
| Storage usage | `statvfs`                  | `GetDiskFreeSpaceEx`                           |                                 |
| Storage I/O   | `/proc/diskstats`          | PDH `\LogicalDisk`                             | Delta-based                     |
| Network       | `/proc/net/dev`            | PDH `\Network Interface`                       | Delta-based                     |
| Temperatures  | `hwmon`                    | **open decision**                              | See platform docs               |
| Fans          | `hwmon` `fanN_input`       | needs ring-0 driver                            | Often unavailable               |
| GPU           | `/sys/class/drm`, NVML     | NVML / ADLX / DXGI                             | Vendor-dependent                |
| Processes     | `/proc/[pid]`              | `NtQuerySystemInformation`                     | Expensive; low cadence          |

The Windows temperature story and the GPU vendor matrix remain the two genuinely
hard problems; both are documented in
[`../platforms/windows.md`](../platforms/windows.md) and
[`../platforms/fedora.md`](../platforms/fedora.md).
