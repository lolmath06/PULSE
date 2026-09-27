# PULSE

**Your system, at a glance.**

> A modular, cross-platform system monitoring dashboard for Windows and Fedora Linux.

[![CI](https://github.com/pulse-monitor/pulse/actions/workflows/ci.yml/badge.svg)](https://github.com/pulse-monitor/pulse/actions/workflows/ci.yml)
[![License: Proprietary](https://img.shields.io/badge/License-Proprietary-informational.svg)](LICENSE)

---

## Status

**Phase 10 — Persistent history & modular visualization.** Version `0.1.0-dev`.

PULSE can now answer, on Fedora and Windows behind exactly the same contract:

> How many physical cores and logical processors does my CPU have, what is each
> logical processor doing, which GPUs does this machine have, what is their
> stable identity, what load, memory and clocks do they report, how hot are the
> processor and the graphics card actually running, which storage devices does
> this machine have, how full are its filesystems, how much is it really
> reading and writing, what does an NVMe controller say about its own wear —
> and which network interfaces does it have, which are connected, how much are
> they actually carrying, how good is the Wi-Fi link — and what is actually
> running on this machine, which applications are using the processor, the
> memory and the disk, and how many processes and threads are there?

On top of the Phase 0 foundation, the Phase 1 metrics contract and the Phase 2
collectors, it reads **per-logical-processor usage, per-logical-processor
frequency and CPU topology natively on both platforms** — `/proc/stat` and
`/sys/devices/system/cpu` on one side; `GetSystemTimes`,
`NtQuerySystemInformationEx`, `CallNtPowerInformation` and
`GetLogicalProcessorInformationEx` on the other. No third-party monitoring
crate, no subprocess, no elevated privileges.

GPU support arrived in Phase 4: adapters are inventoried through DRM on Fedora
and DXGI on Windows, identified by the most stable thing each platform genuinely
offers (an NVML hardware UUID where possible, a PCI address otherwise), and
measured through NVML or the `amdgpu` driver's sysfs attributes — **both loaded
at runtime, so a missing vendor driver costs metrics rather than preventing
PULSE from starting**. A card no backend serves is still shown, named and
identified, with an honest reason on every metric it cannot provide.

Phase 5 added temperatures and cooling: the CPU package temperature through
`hwmon` on Fedora, and per GPU a core temperature, a hotspot temperature, a
memory temperature and a fan speed in genuine revolutions per minute. What makes
that work is mostly what it refuses — a thermal limit is not a temperature, a
core average is not a package reading, a fan control percentage is not an RPM,
and a hotspot is a different sensor from the die. Where a reading is not
available it says so, with the reason, and **PULSE never writes to a fan
control, a limit or a power setting**.

Phase 6 added storage. Physical devices are inventoried through
`/sys/class/block` on Fedora and SetupAPI on Windows; filesystems through
`/proc/self/mountinfo` and the volume GUID API; activity from
`/proc/diskstats` and `IOCTL_DISK_PERFORMANCE`, as **rates derived between two
samples** rather than totals read once; and an NVMe controller's standardised
SMART / Health log through a read-only ioctl on each platform. What makes that
work is again mostly what it refuses — a disk is not a filesystem, a mount point
is not an identity, `0 ms` is not a latency for a disk that completed no
operation, and `100 - percentage_used` is not a health score.

Phase 7 added networking. Interfaces are inventoried through `rtnetlink` on
Fedora and `GetIfTable2` on Windows — one transaction returning every
interface's identity, state and counters at a single instant; traffic, packet
rates, errors and drops are **rates derived between two samples** rather than
totals read once; and Wi-Fi link quality comes from `nl80211` or the Windows
realtime-quality API. What makes that work is again mostly what it refuses — a
local drop counter is not Internet packet loss, a link capacity in bits is not
traffic in bytes, and a signal strength in dBm is never converted into a
made-up quality percentage.

Phase 8 added processes and applications, and with them the first deliberate
**limit** on the metrics engine. A desktop runs several hundred processes, most
of them for less than a second; a catalog entry is a promise that a saved
dashboard reference still resolves months later. Putting six metrics per PID in
the catalog would add roughly two thousand definitions and replace most of them
every refresh, so PULSE registers exactly three low-cardinality figures —
process count, running count, total thread count — and serves the rows through
a `ProcessSnapshotService` and a command of its own. Processes are identified by
**PID plus start time**, because a PID is recycled and PULSE refuses to credit
a new process with its predecessor's CPU time. Process CPU is normalised
against the whole machine on both platforms, so one thread saturating one of 32
logical processors reads `3.1 %` and the column sums to roughly what the system
CPU gauge shows — rather than `3200 %` on Fedora and `100 %` on Windows for the
same work. And what PULSE does **not** collect matters as much: no command
lines, no arguments, no environment variables.

Since Phase 9, clicking a process opens an **inspector** — owner, start time,
architecture, executable, Fedora package or Windows signature and publisher,
SHA-256 on demand — and a right-click menu offers **explicit** controls: suspend
and resume, end process or process tree, priority and CPU affinity. Every action
targets the exact process instance (PID + start token, re-validated just before
acting), destructive ones are confirmed, nothing ever runs automatically, and
PULSE never elevates. Provenance is shown as evidence, never as a verdict. See
[`docs/processes/`](docs/processes/inspector.md).

The catalog is **sized by the machine**: `16 + 3N + P + 11G + 13D + 4V + 11I +
4W` metrics for `N` logical processors, `P` addressable CPU packages, `G` GPUs,
`D` storage devices, `V` volumes, `I` network interfaces and `W` Wi-Fi radios —
314 on the 32-thread, single-package, single-GPU laptop this was built on, with
its internal NVMe drive, an external USB disk, seven mounted filesystems, twelve
published network interfaces and one Wi-Fi radio — and the same 314 whether it
is running 180 processes or 900. Nothing anywhere hardcodes those numbers. References stay identical across operating systems, so a widget
bound to `cpu.usage.logical@cpu:logical-3`, to an NVIDIA card's UUID, to a
drive's serial number, or to a Wi-Fi radio's permanent MAC address, moves from
Fedora to Windows unchanged.

PULSE is careful about distinctions other monitors blur: a **physical core** is
not a **logical processor**, dedicated **VRAM** is not system memory shared with
an integrated GPU, a **storage device** is not a **volume** is not a **mount
point**, and a **dropped frame** is not **packet loss**. It is equally careful
about identity — a GPU is never identified by its product name, its DRM card
number, its NVML index or its DXGI adapter index; a disk is never identified by
`nvme0n1`, `sda` or `PhysicalDrive0`; and a network interface is never
identified by `eth0`, an interface index, an IP address or the **randomised**
MAC both operating systems now put on Wi-Fi by default; and a process is never
identified by its PID alone — because every one of those can change between
boots, between networks, or between one process and the next.

**Phase 10 — history and visualization.** PULSE now also remembers: one
backend scheduler records the stable numeric metrics every five seconds into a
local SQLite file (raw for 24 hours, one-minute min/max/average/count for seven
days), and every system section has a history chart — CPU Total, memory,
temperatures, GPU, storage read/write, network download/upload and process
counts. Each chart can be drawn as a line, area, sparkline, value, bar or gauge,
with presets (Clean, Minimal, Technical, Gaming, Compact, Neon, Transparent) and
a Customize panel for colours, size, curve, fill, background, grid, axes and
statistics. Live cards still refresh on demand, and Refresh never writes history.

## Vision

Most system monitors decide for you what matters. PULSE is built on the opposite
premise: **you compose it.** Dashboards, widgets, layouts, colours and modes are
configuration, not hardcoded screens.

Two ideas shape the design:

- **Modes describe how PULSE behaves** — a normal window, a low-overhead Gaming
  view, a Development view, or **Mini**: a permanent, borderless, always-on-top
  desktop overlay that lives at the edge of your screen while you work or play.
- **Dashboards describe what is displayed** — sets of widgets and their layout,
  independent of the mode rendering them.

Keeping those separate is what makes it possible to have both a 20-widget
Personal dashboard and a single thin line of numbers at the bottom of the screen,
driven by the same engine.

## Planned features

- CPU, GPU, RAM, storage, network, temperature and sensor monitoring
- Real-time graphs and historical data
- Fully configurable dashboards
- Movable, resizable widgets
- Personal mode with 20+ widgets
- Gaming, Development and Mini modes
- Mini: a permanent desktop overlay — transparent, borderless, always-on-top,
  optionally click-through, freely positioned, multi-monitor
- Alerts, themes, presets
- Tray icon and autostart
- Packaged builds for Windows and Fedora Linux

None of this is implemented yet. See [Roadmap](#roadmap).

## Platforms

PULSE targets **Windows** and **Fedora Linux** as two first-class platforms.
Neither is the reference platform; neither is experimental.

|                     | Windows                      | Fedora Linux                          |
| ------------------- | ---------------------------- | ------------------------------------- |
| Support level       | First-class                  | First-class                           |
| Versions            | Windows 10 and 11            | Fedora 39+ (Wayland and X11)          |
| Data sources        | PDH, Win32, WMI, vendor SDKs | `/proc`, `/sys`, `hwmon`, vendor SDKs |
| Packaging (planned) | NSIS, MSI                    | RPM, plus DEB and AppImage            |

This has a concrete consequence, written into the project's rules:

> **A system-facing feature is not considered complete until its behavior on
> both Windows and Fedora Linux has been designed and, whenever materially
> testable, validated.**

macOS is not a target.

## Stack

| Layer           | Technology                             |
| --------------- | -------------------------------------- |
| Desktop shell   | [Tauri 2](https://tauri.app)           |
| Backend         | Rust                                   |
| Frontend        | React 19 + TypeScript                  |
| Build           | Vite                                   |
| Package manager | pnpm                                   |
| Tests           | Vitest (frontend), `cargo test` (Rust) |
| Quality         | ESLint, Prettier, rustfmt, Clippy      |

## Prerequisites

- **Node.js 20.19+** (22 LTS recommended) and **pnpm 10+** (`corepack enable pnpm`)
- **Rust 1.77.2+** via [rustup](https://rustup.rs) — verified minimum for the
  whole locked dependency graph ([MSRV](docs/development/msrv.md))

### Fedora Linux

```bash
sudo dnf install -y \
  webkit2gtk4.1-devel \
  openssl-devel \
  curl wget file \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  gcc gcc-c++ make
```

### Windows

- **Microsoft C++ Build Tools** with the "Desktop development with C++" workload
- **WebView2 Runtime** (preinstalled on Windows 11 and current Windows 10)

## Development setup

```bash
git clone https://github.com/pulse-monitor/pulse.git
cd pulse
pnpm install
```

## Running PULSE

```bash
pnpm app:dev
```

This starts the Vite dev server and the Rust backend, and opens the PULSE
window.

For frontend-only work (no backend, faster iteration):

```bash
pnpm dev
```

The status bar will report _"Backend unavailable — UI-only mode"_. That is
expected outside the Tauri runtime.

## Commands

| Command                             | What it does                            |
| ----------------------------------- | --------------------------------------- |
| `pnpm app:dev`                      | Run PULSE in development                |
| `pnpm app:build`                    | Build the desktop application           |
| `pnpm dev`                          | Vite dev server only (no backend)       |
| `pnpm build`                        | Typecheck and build the frontend bundle |
| `pnpm typecheck`                    | TypeScript, no emit                     |
| `pnpm lint` / `pnpm lint:fix`       | ESLint                                  |
| `pnpm format` / `pnpm format:check` | Prettier                                |
| `pnpm test` / `pnpm test:watch`     | Vitest                                  |
| `pnpm rust:fmt`                     | `cargo fmt --check`                     |
| `pnpm rust:lint`                    | Clippy, warnings denied                 |
| `pnpm rust:test`                    | `cargo test`                            |
| `pnpm check:all`                    | Everything CI runs                      |

## Structure

```text
PULSE/
├── src/                 # React frontend
│   ├── app/             # router, routes, app constants
│   ├── components/      # reusable components
│   ├── features/        # feature slices
│   ├── hooks/
│   ├── layouts/
│   ├── pages/           # one component per route
│   ├── services/        # the invoke() boundary
│   ├── stores/
│   ├── styles/          # design tokens, global CSS
│   ├── types/           # shared types, mirrors of Rust payloads
│   ├── utils/
│   └── visualization/   # generic chart engine: renderers, config, presets, Customize
├── src-tauri/           # Rust backend
│   └── src/
│       ├── commands/    # Tauri command surface
│       ├── history/     # persistent metric history: scheduler, SQLite store, queries
│       ├── metrics/     # metrics engine, model, well-known declarations
│       ├── platform/    # the platform seam
│       ├── processes/   # process snapshots — high-cardinality, outside the catalog
│       │   ├── linux/   # /proc, /sys, hwmon, Wayland/X11
│       │   └── windows/ # WMI, PDH, vendor SDKs
│       ├── services/    # cross-platform logic
│       └── state/
├── docs/                # architecture, platforms, metrics, widgets
└── .github/workflows/   # CI
```

The layering rule, in one line:

```text
React UI  →  Tauri commands  →  application core  →  platform layer  →  { Linux | Windows }
```

The frontend never reads `/proc`, `/sys`, WMI, PDH or NVML. Ever. Only the
platform layer knows which OS it is running on.

## Documentation

Start at [`docs/README.md`](docs/README.md).

- [Architecture overview](docs/architecture/overview.md)
- [Mini overlay design](docs/architecture/mini-overlay.md)
- [Getting started](docs/development/getting-started.md)
- [Testing](docs/development/testing.md)
- [Fedora Linux](docs/platforms/fedora.md) · [Windows](docs/platforms/windows.md)
- [Metrics engine](docs/metrics/README.md) — [model](docs/metrics/model.md),
  [identifiers](docs/metrics/identifiers.md), [providers](docs/metrics/providers.md),
  [CPU & memory](docs/metrics/cpu-memory.md),
  [advanced CPU](docs/metrics/cpu-advanced.md), [GPU](docs/metrics/gpu.md),
  [thermals](docs/metrics/thermals.md), [storage](docs/metrics/storage.md),
  [network](docs/metrics/network.md), [processes](docs/metrics/processes.md)
- [History](docs/history/architecture.md) — [storage](docs/history/storage.md),
  [retention](docs/history/retention.md)
- [Visualization](docs/visualization/architecture.md) —
  [renderers](docs/visualization/renderers.md),
  [customization](docs/visualization/customization.md)
- [Widgets](docs/widgets/README.md)

## Roadmap

| Phase | Scope                                                          | Status      |
| ----- | -------------------------------------------------------------- | ----------- |
| 0     | Foundation: structure, platform abstraction, shell, docs, CI   | Done        |
| 1     | Metrics engine: model, contract, catalog, frontend API         | Done        |
| 2     | First real collectors: aggregate CPU usage, physical memory    | Done        |
| 3     | Advanced CPU: per-logical-processor usage, frequency, topology | Done        |
| 4     | GPU inventory, identity and core telemetry                     | Done        |
| 5     | Temperatures and fan speeds                                    | Done        |
| 6     | Storage devices, volumes, I/O and NVMe health                  | Done        |
| 7     | Network interfaces, traffic and Wi-Fi quality                  | Done        |
| 8     | Processes, applications and their CPU, memory and I/O          | Done        |
| 9     | Process inspector, provenance and controls                     | Done        |
| 10    | Persistent history, time series, modular visualization engine  | **Current** |
| 11    | Configurable dashboard, widgets, desktop overlay               | Planned     |
| 1+    | Active network probes                                          | Planned     |
| 1+    | Widget engine, configurable dashboards                         | Planned     |
| 1+    | Mini overlay, Gaming and Development modes                     | Planned     |
| 1+    | Themes, presets, alerts, tray, autostart                       | Planned     |
| 1+    | Packaged installers and releases                               | Planned     |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Run `pnpm check:all` before opening a
pull request, and remember the cross-platform rule above.

## Security

See [SECURITY.md](SECURITY.md).

## License

[Proprietary](LICENSE).
