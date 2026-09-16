# PULSE

**Your system, at a glance.**

> A modular, cross-platform system monitoring dashboard for Windows and Fedora Linux.

[![CI](https://github.com/pulse-monitor/pulse/actions/workflows/ci.yml/badge.svg)](https://github.com/pulse-monitor/pulse/actions/workflows/ci.yml)
[![License: Proprietary](https://img.shields.io/badge/License-Proprietary-informational.svg)](LICENSE)

---

## Status

**Phase 4 — GPU Inventory & Core Metrics.** Version `0.1.0-dev`.

PULSE can now answer, on Fedora and Windows behind exactly the same contract:

> How many physical cores and logical processors does my CPU have, what is each
> logical processor doing, which GPUs does this machine have, what is their
> stable identity, and what load, memory and clocks do they report?

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

The catalog is **sized by the machine**: `9 + 3N + 7G` metrics for `N` logical
processors and `G` GPUs — 112 on the 32-thread, single-GPU laptop this was built
on. Nothing anywhere hardcodes those numbers. References stay identical across
operating systems, so a widget bound to `cpu.usage.logical@cpu:logical-3`, or to
an NVIDIA card's UUID, moves from Fedora to Windows unchanged.

PULSE is careful about distinctions other monitors blur: a **physical core** is
not a **logical processor**, and dedicated **VRAM** is not system memory shared
with an integrated GPU. It is equally careful about identity — a GPU is never
identified by its product name, its DRM card number, its NVML index or its DXGI
adapter index, because every one of those can change between boots.

There is still no scheduler and no history: the UI samples on demand.

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
- **Rust stable 1.77.2+** via [rustup](https://rustup.rs)

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
│   └── utils/
├── src-tauri/           # Rust backend
│   └── src/
│       ├── commands/    # Tauri command surface
│       ├── metrics/     # metrics engine, model, well-known declarations
│       ├── platform/    # the platform seam
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
  [advanced CPU](docs/metrics/cpu-advanced.md), [GPU](docs/metrics/gpu.md)
- [Widgets](docs/widgets/README.md)

## Roadmap

| Phase | Scope                                                          | Status      |
| ----- | -------------------------------------------------------------- | ----------- |
| 0     | Foundation: structure, platform abstraction, shell, docs, CI   | Done        |
| 1     | Metrics engine: model, contract, catalog, frontend API         | Done        |
| 2     | First real collectors: aggregate CPU usage, physical memory    | Done        |
| 3     | Advanced CPU: per-logical-processor usage, frequency, topology | **Current** |
| 4+    | Temperatures, GPU, storage, network, history, graphs           | Planned     |
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
