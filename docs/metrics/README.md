# PULSE Metrics Engine

> **Phase 1 — the engine exists and is tested. It collects nothing yet.**
>
> The model, the provider contract, the engine and the frontend API are
> implemented. No system collector is registered, so the catalog is empty by
> design rather than by accident.

## Documents

| Document                           | Contents                                                                                |
| ---------------------------------- | --------------------------------------------------------------------------------------- |
| [`model.md`](model.md)             | Definition, sample, value, unit, kind, category, availability, errors, contract version |
| [`identifiers.md`](identifiers.md) | `MetricKey`, `SourceId`, stability rules, why labels are not identifiers                |
| [`providers.md`](providers.md)     | The provider contract, registration, collisions, isolation                              |

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

## Deliberately not in Phase 1

No scheduler, no polling loop, no history, no ring buffer, no SQLite, no
adaptive sampling, no subscriptions, no events. The interaction model is
`request → sample → response`, which is all the widget engine needs to be built
against.

Planned for later phases:

- **Scheduler** — cadence per metric, driven by subscriptions and by the active
  mode's budget, with declared sampling costs so expensive metrics run rarely.
- **History** — a bounded ring buffer per metric in Rust, with downsampling.
- **Transport** — Tauri events for streaming samples, so the UI stops polling.

## Planned metric families

| Family        | Fedora source              | Windows source                | Notes                  |
| ------------- | -------------------------- | ----------------------------- | ---------------------- |
| CPU load      | `/proc/stat`               | `GetSystemTimes`, PDH         | Delta-based            |
| CPU frequency | `cpufreq/scaling_cur_freq` | PDH `% Processor Performance` | Per-core               |
| Memory        | `/proc/meminfo`            | `GlobalMemoryStatusEx`        | Use `MemAvailable`     |
| Storage usage | `statvfs`                  | `GetDiskFreeSpaceEx`          |                        |
| Storage I/O   | `/proc/diskstats`          | PDH `\LogicalDisk`            | Delta-based            |
| Network       | `/proc/net/dev`            | PDH `\Network Interface`      | Delta-based            |
| Temperatures  | `hwmon`                    | **open decision**             | See platform docs      |
| Fans          | `hwmon` `fanN_input`       | needs ring-0 driver           | Often unavailable      |
| GPU           | `/sys/class/drm`, NVML     | NVML / ADLX / DXGI            | Vendor-dependent       |
| Processes     | `/proc/[pid]`              | `NtQuerySystemInformation`    | Expensive; low cadence |

The Windows temperature story and the GPU vendor matrix remain the two genuinely
hard problems; both are documented in
[`../platforms/windows.md`](../platforms/windows.md) and
[`../platforms/fedora.md`](../platforms/fedora.md).
