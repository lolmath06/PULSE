# PULSE Metrics Engine

> **Phase 8 — processes and applications.**
>
> The model, the provider contract, the engine and the frontend API were built
> in Phase 1. Phase 2 added the first native collectors: CPU usage and physical
> memory, on both Fedora and Windows. Phase 3 made the CPU support real —
> per-logical-processor usage and frequency, plus topology. Phase 4 added the
> first GPU support: inventory, stable identity, and seven metrics per device
> across NVIDIA, AMD and Intel; Phase 5 added temperatures and fan speeds to
> both families. Phase 6 added storage: physical devices, mounted filesystems,
> delta-based I/O rates and the standardised NVMe health log. Phase 7 added
> networking: interfaces, their identity, delta-based traffic rates, link
> speeds and Wi-Fi link quality. Phase 8 added processes — and with them the
> first family whose detail deliberately does **not** go through the engine:
> three machine-wide counts are registered, and the several hundred process
> rows are served by a snapshot service of their own. The catalog is **sized by
> the machine** — `16 + 3N + P + 11G + 13D + 4V + 11I + 4W` for `N` logical
> processors, `P` addressable CPU packages, `G` GPUs, `D` storage devices, `V`
> volumes, `I` network interfaces and `W` Wi-Fi radios — and there are six
> providers per platform. Note what is absent from that formula: the number of
> processes.

## Documents

| Document                             | Contents                                                                                   |
| ------------------------------------ | ------------------------------------------------------------------------------------------ |
| [`model.md`](model.md)               | Definition, sample, value, unit, kind, category, availability, errors, contract version    |
| [`identifiers.md`](identifiers.md)   | `MetricKey`, `SourceId`, stability rules, why labels are not identifiers                   |
| [`providers.md`](providers.md)       | The provider contract, registration, collisions, isolation                                 |
| [`cpu-memory.md`](cpu-memory.md)     | Phase 2: aggregate CPU usage and physical memory                                           |
| [`cpu-advanced.md`](cpu-advanced.md) | Phase 3: per-logical-processor usage and frequency, CPU topology, processor groups         |
| [`gpu.md`](gpu.md)                   | Phase 4: GPU inventory, identity, NVML, DXGI, AMD sysfs                                    |
| [`thermals.md`](thermals.md)         | Phase 5: temperatures and fan speeds, `hwmon`, and the readings PULSE refuses to publish   |
| [`storage.md`](storage.md)           | Phase 6: devices vs volumes, identity, I/O deltas, `statvfs`, NVMe SMART/Health            |
| [`network.md`](network.md)           | Phase 7: interfaces, permanent vs randomised MAC, rtnetlink, nl80211, Wi-Fi quality        |
| [`processes.md`](processes.md)       | Phase 8: why processes are not catalog metrics, PID reuse, CPU normalisation, applications |

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

The GPU provider follows the same rule: each device is read at most once per
request, whichever of its seven metrics were asked for — one NVML round trip or
one sysfs pass, never seven.

The storage provider does it at the widest scale yet: **one `/proc/diskstats`
read feeds every device's six I/O metrics at once**, which matters for more than
speed — rates are derived from the interval between two snapshots, so every
device's counters must be captured at the _same_ instant, or two disks' figures
describe two slightly different windows.

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

| Command                     | Returns                                                                                     |
| --------------------------- | ------------------------------------------------------------------------------------------- |
| `get_metrics_engine_status` | `EngineStatus` — schema version, state, provider and metric counts                          |
| `get_metric_catalog`        | `MetricDefinition[]`, ordered by `(key, sourceId)`                                          |
| `sample_metrics(metrics)`   | `MetricSample[]`, one per requested reference, in order                                     |
| `get_process_snapshot`      | `ProcessSnapshot` — deliberately **not** an engine call, see [`processes.md`](processes.md) |

Called through `src/services/metrics.ts`. Types in `src/types/metrics.ts`.

## Deliberately still absent

No scheduler, no polling loop, no history, no ring buffer, no SQLite, no
adaptive sampling, no subscriptions, no events. The interaction model remains
`request → sample → response`, and the UI refreshes on demand.

This is why a first CPU sample may report `temporarilyUnavailable`: usage is a
rate, and without a sampler there may be no usable delta yet. PULSE says so
rather than reporting `0%`. The same applies to every `storage.io.*` metric —
see [`storage.md`](storage.md#activity-is-measured-not-read) — and to every
`network.receive.*` and `network.transmit.*` metric, see
[`network.md`](network.md#traffic-is-measured-not-read). Process CPU shares and
process I/O rates behave the same way, for the same reason — see
[`processes.md`](processes.md#16-manual-refresh-only).

### 8. Cardinality is an architectural constraint, not a performance detail

The engine is built for sources that are **stable**: a CPU, a GPU, a disk, an
interface. Phase 8 was the first family that is not, and it was resolved by
keeping the three low-cardinality figures in the engine and putting the
ephemeral rows behind their own service and command, rather than by making the
engine tolerate thousands of disposable references. See
[`processes.md`](processes.md#1-the-architectural-decision-two-tiers-not-one).

Planned for later phases:

- **Scheduler** — cadence per metric, driven by subscriptions and by the active
  mode's budget, with declared sampling costs so expensive metrics run rarely.
- **History** — a bounded ring buffer per metric in Rust, with downsampling.
- **Transport** — Tauri events for streaming samples, so the UI stops polling.

## Planned metric families

| Family         | Fedora source              | Windows source                                 | Notes                            |
| -------------- | -------------------------- | ---------------------------------------------- | -------------------------------- |
| CPU load       | `/proc/stat`               | `GetSystemTimes`, `NtQuerySystemInformationEx` | Delta-based; **done**            |
| CPU frequency  | `cpufreq/scaling_cur_freq` | `CallNtPowerInformation`                       | Per logical processor; **done**  |
| CPU topology   | `cpuN/topology/`           | `GetLogicalProcessorInformationEx`             | **done**                         |
| Memory         | `/proc/meminfo`            | `GlobalMemoryStatusEx`                         | Use `MemAvailable`               |
| Storage usage  | `statvfs`                  | `GetDiskFreeSpaceExW`                          | **done**                         |
| Storage I/O    | `/proc/diskstats`          | `IOCTL_DISK_PERFORMANCE`                       | Delta-based; **done**            |
| Storage health | NVMe log via ioctl         | NVMe log via storage property                  | NVMe only; **done**              |
| Network        | `/proc/net/dev`            | PDH `\Network Interface`                       | Delta-based                      |
| Temperatures   | `hwmon`                    | **open decision**                              | See platform docs                |
| Fans           | `hwmon` `fanN_input`       | needs ring-0 driver                            | Often unavailable                |
| GPU            | `/sys/class/drm`, NVML     | NVML / ADLX / DXGI                             | Vendor-dependent                 |
| Processes      | `/proc/[pid]`              | `CreateToolhelp32Snapshot` + handles           | **Not in the catalog**; **done** |

The Windows temperature story and the GPU vendor matrix remain the two genuinely
hard problems; both are documented in
[`../platforms/windows.md`](../platforms/windows.md) and
[`../platforms/fedora.md`](../platforms/fedora.md).
