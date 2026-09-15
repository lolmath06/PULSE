# PULSE Metrics Engine

> Status: **not implemented.** Phase 0 ships no metrics. This is the design
> contract later phases must satisfy.

## Position in the architecture

The metrics engine sits in the cross-platform application core. It consumes the
platform layer and never reads the system itself.

```text
widgets (React)
   │  subscribe
metrics engine (Rust, cross-platform)
   │  collector trait
platform layer  ──  linux/ (/proc, /sys, hwmon)   windows/ (PDH, WMI, SDKs)
```

## Principles

### 1. Pull once, fan out

A metric is sampled once per tick regardless of how many widgets show it. Ten
CPU widgets cost one `/proc/stat` read.

### 2. Subscription-driven sampling

Widgets subscribe; they never poll. **A metric nobody is subscribed to is never
sampled.** This is what makes Gaming mode and the Mini overlay genuinely cheap
rather than nominally cheap.

### 3. Sampling lives in Rust

Not in the webview. The Mini overlay and future tray must keep producing data
while the main window is hidden or closed, and history must survive it.

### 4. Availability is a first-class state

Every metric resolves to one of:

- `Available(value)`
- `Unavailable { reason }` — no sensor, no kernel module, no vendor driver
- `RequiresPermission { what }` — needs root / administrator

A missing sensor is **normal**, especially for temperatures. The UI must say so
plainly. Showing `0 °C` for an absent sensor is a bug, not a fallback.

### 5. Deltas are the engine's job, not the widget's

CPU utilisation, disk I/O and network throughput are all rate metrics derived
from counter deltas. The engine owns the previous sample and the timing; a
widget receives a rate, never two counters and a stopwatch.

### 6. Sampling cost is declared

Collectors declare how expensive they are (reading `/proc/meminfo` is not
WMI-querying disk inventory). The scheduler uses this to run cheap metrics
frequently and expensive ones rarely, and to respect a mode's budget.

### 7. Both platforms or it is not done

> A system-facing feature is not considered complete until its behavior on both
> Windows and Fedora Linux has been designed and, whenever materially testable,
> validated.

A collector that only exists for one OS is an incomplete collector. The other
platform must at minimum return a documented `Unavailable { reason }`.

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

The Windows temperature story and the GPU vendor matrix are the two genuinely
hard problems. Both are documented in
[`../platforms/windows.md`](../platforms/windows.md) and
[`../platforms/fedora.md`](../platforms/fedora.md); neither should be
improvised during implementation.

## History

A bounded ring buffer per metric, in Rust, with downsampling for longer windows.
Persistence format and retention are a later decision. Keeping it out of the
webview is not.

## Transport

- **Commands** for one-shot queries (inventory, capability probing).
- **Events** for streaming samples, so the UI does not poll the backend either.
