# Changelog

All notable changes to PULSE are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **`NtQuerySystemInformationEx` is now resolved dynamically** rather than
  imported at load time. It backs `cpu.usage.logical` on Windows and lives in
  `ntdll`, which Microsoft documents as subject to change — so an `extern`
  block made it a load-time import, and on a Windows that does not export the
  symbol the loader would have failed the process **before PULSE could report
  anything**. That inverted the principle the availability model exists to
  enforce: a capability being unavailable is not the application being unable
  to start.

  The symbol is now looked up once at provider construction with
  `GetModuleHandleW("ntdll.dll")` and `GetProcAddress`, then probed with one
  undersized query to confirm it understands the information class.
  `GetModuleHandleW` rather than `LoadLibrary`: `ntdll` is already mapped into
  every Win32 process, so nothing touches the filesystem and no arbitrary
  library is loaded. There is no `#[link]` attribute and no `extern "system" { }`
  block left anywhere in PULSE.

  If the module, the symbol, or the probe fails, the result is a structured
  `Unsupported` availability — no `unwrap`, no `expect`, no panic, and
  `WindowsCpuProvider` still constructs. Only `cpu.usage.logical` becomes
  unavailable; `cpu.usage.total` (`GetSystemTimes`), `cpu.count.*`
  (`GetLogicalProcessorInformationEx`), `cpu.frequency.*`
  (`CallNtPowerInformation`) and `memory.*` all keep working, because each sits
  on its own entry point. This is also why the aggregate is read from
  `GetSystemTimes` rather than summed from the per-processor array.

  The per-processor metrics **stay in the catalog** with an honest status
  instead of disappearing, so a dashboard holding
  `cpu.usage.logical@cpu:logical-3` keeps the same reference on a machine
  without the capability. Provider count is unaffected and stays at 2.

  Resolution happens once and the handle is reused: no `GetProcAddress` per
  refresh or per processor, and no mutable global. The raw pointer is private
  to a safe wrapper and never circulates through the provider.

- Per-processor counter reads now go through a `ProcessorTimesSource` seam, so
  the group stitching, short-reply handling and every failure path are unit
  tested from Fedora against a fake source rather than requiring the real DLL.
- `records_in_reply` replaced `usize::is_multiple_of`, which is newer than the
  crate's declared `rust-version` of 1.77.2 — a latent MSRV violation that was
  invisible while the code sat behind `cfg(target_os = "windows")`.

### Added — Phase 3: Advanced CPU Metrics

Real per-processor CPU detail on both Fedora Linux and Windows, behind one
contract. PULSE can now say how many physical cores and logical processors a
machine has, what each logical processor is doing, and at what frequency the OS
reports it running.

- **`cpu.usage.logical`** (`cpu:logical-N`, percent) — per logical processor.
  `/proc/stat`'s `cpuN` lines on Fedora; `NtQuerySystemInformationEx` with
  `SystemProcessorPerformanceInformation` on Windows. The formula is identical
  to `cpu.usage.total`'s, so the two reconcile instead of being two definitions
  of "busy" that share a name.
- **`cpu.frequency.current`** and **`cpu.frequency.max`** (`cpu:logical-N`,
  hertz) — `cpufreq/scaling_cur_freq` and `cpufreq/cpuinfo_max_freq` on Fedora;
  `CallNtPowerInformation(ProcessorInformation)` on Windows.
- **`cpu.count.logical`, `cpu.count.physical`, `cpu.count.package`**
  (`cpu:system`, count) — `/sys/devices/system/cpu/` topology on Fedora;
  `GetLogicalProcessorInformationEx` on Windows. Declared as `state` rather than
  `gauge`, because averaging a core count over time is meaningless and
  `MetricKind::State` refuses it by construction.
- **A catalog sized by the machine** — `8 + 3N` metrics for `N` logical
  processors: 104 on a 32-thread laptop, 20 on a four-thread virtual machine.
  No count is hardcoded anywhere, in Rust or in React; `wellknown::cpu` exposes
  a generator over a discovered topology rather than a constant list. The
  provider count stays at **2** per platform — one CPU provider owns every CPU
  metric on the machine, not one provider per processor.
- **Logical processor, physical core and package kept rigorously distinct.**
  With simultaneous multithreading two logical processors share one physical
  core; on a hybrid CPU the ratio is not even constant (8 P-cores × 2 threads
  plus 16 E-cores = 24 cores, 32 logical processors). Physical cores are counted
  as distinct `(package_id, core_id)` pairs on Linux and as
  `RelationProcessorCore` **records** on Windows — never as set affinity bits,
  which would count threads.
- **`cpu:logical-N` identifiers**, documented as *logical slots of this system*
  rather than hardware serial numbers: stable across reboots and restarts on one
  machine, meaningless on another. On Linux the ordinal is the kernel's own CPU
  number, so it matches `htop` and `taskset`.
- **Windows processor groups handled properly** — the implementation is not
  capped at 64 logical processors. PULSE assigns ordinals by sorting
  `(group, index in group)`, so `group 1 bit 0` becomes `cpu:logical-64` and
  never collides with `cpu:logical-0`. The mapping is deterministic, so it
  cannot shift between runs and silently re-point saved references. Tests cover
  128- and 256-processor layouts across two and four groups, from Fedora.
- **Hertz everywhere on the wire.** Linux CPUFreq reports kHz and the Windows
  power API reports MHz; both are converted in the platform layer, with overflow
  checks. A zero reading means "not reported" on both platforms and is published
  as unavailable — never as `0 Hz`, which renders as `0 GHz` and reads as a
  claim that the core has stopped.
- **A multi-processor CPU baseline** behind a single mutex holding the aggregate
  and every logical processor. A processor that appears, disappears, reports
  counters that rewind, or reports no elapsed time is answered with
  `temporarilyUnavailable` and a reason — never `0%` — and its baseline is
  re-primed so the next request succeeds. A departed processor's stale baseline
  is dropped, so one that comes back does not difference against pre-offline
  counters.
- **Failure granularity per metric, per processor.** A missing
  `cpu17/cpufreq` costs `cpu17`'s frequency and nothing else; `cpu.usage.total`,
  every `cpu.usage.logical` and the topology counts keep working. On Windows the
  aggregate deliberately stays on the documented `GetSystemTimes`, so it
  survives the per-processor counter call being unavailable. A count the
  platform genuinely cannot determine is `notDetected` with a reason, never
  guessed by halving the logical count.
- **CPU details card** on Overview — physical cores, logical processors and
  packages, then a compact auto-filling grid of every logical processor with its
  usage, a meter and its current frequency, with the maximum in the tooltip. It
  has its own Refresh button and stays usable from 4 to 128+ processors. An
  unknown frequency shows `—` with the reason in its tooltip, never `0 GHz`.
- **The frontend discovers processors from the catalog** rather than holding any
  list of CPUs, and **sorts them numerically**: the backend's `(key, sourceId)`
  string ordering yields `logical-1, logical-10, logical-11, logical-2`, which
  would scramble the table. A processor appears as soon as any one of its
  metrics does, so a missing frequency never costs it a row.
- **`formatHertz`** (`src/utils/units.ts`) — Hz to `800 MHz` / `3.20 GHz` at the
  display edge only. The contract stays in hertz; no GHz exists in Rust.
- **Still no scheduler.** One sample on mount, one per Refresh click, and a test
  that advances timers by sixty seconds and asserts no further request is made.

### Changed

- `MetricsEngineCard` and the engine-status tests no longer assume five metrics;
  the count is read from the engine and is now machine-dependent.
- `CpuUsageTracker` takes a whole-machine `CpuSnapshot` rather than a single
  pair of counters, and reports self-contradictory counters as a transient
  unavailability with a re-primed baseline rather than as a hard error — the
  same treatment as every other unmeasurable case.
- `metrics::wellknown::cpu` became a module directory (`topology`, `usage`,
  `frequency`), and `cpu::definitions` now takes the discovered topology.
- `windows-sys` gained the `Win32_System_Power`,
  `Win32_System_WindowsProgramming` and `Wdk_System_SystemInformation` features.

### Documentation

- New [`docs/metrics/cpu-advanced.md`](docs/metrics/cpu-advanced.md) — the
  logical/physical/package vocabulary, the dynamic catalog, `cpu:logical-N`
  stability and its implication for future dashboard selectors, both platforms'
  data sources, the Windows per-processor API comparison and why
  `NtQuerySystemInformationEx` was chosen, processor groups, hybrid CPUs, the
  hertz contract, what `cpu.frequency.current` does and does not claim, the
  tracker, and the per-refresh cost on each platform.
- Updated `docs/metrics/README.md`, `docs/metrics/providers.md`,
  `docs/metrics/identifiers.md`, `docs/platforms/fedora.md`,
  `docs/platforms/windows.md`, `docs/architecture/overview.md` and `README.md`.

## [Phase 2]

### Added — Phase 2: Real CPU & Memory Metrics

PULSE's first real system metrics, implemented natively on both Fedora Linux
and Windows. Five metrics, sharing one set of references across both operating
systems.

- **`cpu.usage.total`** (`cpu:system`, percent) — `/proc/stat` on Fedora,
  `GetSystemTimes` on Windows.
- **`memory.total`, `memory.used`, `memory.available`** (`memory:system`,
  bytes) and **`memory.usage.percent`** — `/proc/meminfo` on Fedora,
  `GlobalMemoryStatusEx` on Windows.
- **Providers** `linux.cpu` / `linux.memory` and `windows.cpu` /
  `windows.memory`, registered automatically at startup by the platform layer.
- **One PULSE memory convention on both platforms** — `used = total -
  available`, `usage_percent = used / total * 100`, so `used + available` always
  equals `total` exactly. `MemAvailable` is used rather than `MemFree`, which
  would make a machine with a warm page cache look nearly full. Windows'
  rounded `dwMemoryLoad` is ignored in favour of computing from the byte counts,
  so the percentage agrees with the figures shown beside it.
- **Shared metric declarations** in `metrics/wellknown/` — key, source, unit,
  kind and user-facing text live in one place, and platform providers supply
  only raw counters. A contract test asserts the Linux and Windows declarations
  differ in `providerId` and nothing else, which is what lets a dashboard move
  between operating systems.
- **CPU baseline without blocking** — usage is a rate, so each provider captures
  a baseline at construction and each request compares against the previous
  reading. No `sleep` inside a command. When no usable delta exists the sample
  is `temporarilyUnavailable` with a reason, never `0%`, and the baseline is
  reset so the next request succeeds.
- **`HostPlatform::metric_providers()`** — the platform layer decides which
  providers exist; `services::metrics::build_engine()` composes them. The engine
  still contains no `cfg(target_os)` at all.
- **Live system sample card** on Overview, with a Refresh button and the sample
  timestamp. It shows real values only; an unavailable metric is explained
  rather than shown as zero.
- **Display formatting helpers** (`src/utils/units.ts`) — bytes to GiB/MiB,
  percentages, sample times. Presentation only: the contract still carries
  bytes, percent and Unix epoch milliseconds.
- **Tests** — 216 Rust (was 122) and 44 frontend (was 22), including
  `/proc/stat` and `/proc/meminfo` fixtures for malformed, truncated and
  unusual input, the Windows counter arithmetic, and host tests that assert
  invariants rather than a particular amount of RAM.
- **Documentation** — new `docs/metrics/cpu-memory.md`; metrics README,
  providers, identifiers, both platform guides, architecture overview and README
  updated.

### Changed

- Both platform modules now compile on **every** host, with only the FFI calls
  and `HostPlatform` implementations gated behind `cfg(target_os)`. A Fedora
  test run therefore exercises the Windows arithmetic and vice versa — there was
  no good reason for Windows maths to be untestable from a Linux machine.
- `metrics::build_engine` takes providers as an argument instead of building an
  empty engine, keeping the metrics layer free of platform knowledge.

### Notes

- **No scheduler and no polling.** The model is still
  `request → sample → response`; the UI refreshes on demand. A hidden interval
  in the frontend would be a scheduler in disguise, and a test asserts the card
  does not poll.
- **No elevated privileges.** All five metrics work as an ordinary user on both
  platforms.
- `guest` and `guest_nice` are excluded from the `/proc/stat` total — the kernel
  already counts them inside `user` and `nice`, and adding them again is the
  classic bug that makes a busy host look idle.
- Windows counts idle time inside `KernelTime`; applying the Linux formula there
  would report an idle machine as heavily busy.
- Dependencies: only `windows-sys` (raw FFI, Windows-only) was added. No
  cross-platform monitoring crate — validating PULSE's own native architecture
  is part of what this phase is for.

### Added — Phase 1: Metrics Engine Foundation

The universal metrics contract. **No hardware data is collected yet**: the
engine registers no providers, so PULSE reports an empty catalog rather than
inventing numbers.

- **Metric model** (`src-tauri/src/metrics/model/`) — validated `MetricKey`,
  `SourceId`, `ProviderId` and `MetricRef`; `MetricDefinition`, `MetricSample`,
  `MetricValue`, `MetricUnit`, `MetricKind`, `MetricCategory`, `Availability`
  and `MetricError`.
- **Identity separated from presentation** — `MetricKey` says *what* is
  measured, `SourceId` says *on what*. A device's product name is never an
  identifier, so two identical drives stay distinguishable and a renamed device
  does not break saved dashboards.
- **Canonical units** — the backend always reports hertz, bytes and Celsius;
  display conversion belongs to the frontend.
- **Seven availability states** — `unsupported`, `notDetected`,
  `permissionDenied`, `temporarilyUnavailable`, `providerError` and
  `notRegistered` stay distinguishable from `available` and from each other.
- **`MetricProvider` trait** — synchronous, `Send + Sync`, with structured
  errors.
- **`MetricsEngine`** — provider registration with atomic collision detection,
  a deterministically ordered catalog, `HashMap`-indexed reference resolution,
  request-order sampling with per-provider deduplication, and failure isolation
  so one broken provider cannot blank out the others.
- **`METRICS_SCHEMA_VERSION = 1`** — an explicit contract version, mirrored in
  TypeScript and checked by the UI.
- **Tauri commands** — `get_metrics_engine_status`, `get_metric_catalog`,
  `sample_metrics`, backed by an `Arc<MetricsEngine>` in Tauri state.
- **TypeScript contract** — `src/types/metrics.ts` and `src/services/metrics.ts`,
  with no `any`.
- **Metrics Engine card** on Overview — an architecture check reporting status,
  schema version and counts. It displays no hardware readings.
- **Tests** — 122 Rust tests (up from 14) and 22 frontend tests (up from 9),
  including contract tests that pin every payload's exact field set so Rust and
  TypeScript cannot drift apart silently.
- **Documentation** — `docs/metrics/model.md`, `docs/metrics/identifiers.md`,
  `docs/metrics/providers.md`; `docs/metrics/README.md` and
  `docs/architecture/overview.md` updated.

### Notes

- A non-finite float is rejected at construction: `serde_json` would serialise
  `NaN` as JSON `null` while the sample still claimed to be available.
- Provider *panics* are not sandboxed. `panic = "abort"` in the release profile
  makes `catch_unwind` a debug-only guarantee, so the contract requires
  providers not to panic instead of pretending to contain them. Revisiting this
  is a deliberate open decision recorded in `docs/metrics/providers.md`.

## [0.1.0-dev] — Phase 0: Foundation

The foundation of PULSE. No monitoring functionality yet — this release
establishes the architecture everything else will be built on.

### Added

- **Project foundation** — Tauri 2 + React 19 + TypeScript + Vite + Rust,
  managed with pnpm.
- **Platform abstraction layer** (`src-tauri/src/platform/`) — a `HostPlatform`
  trait with Linux and Windows implementations, plus an `UnsupportedPlatform`
  fallback. This is the only place in the codebase that branches on the
  operating system.
- **Per-target dependencies** — Windows-only crates are declared under
  `[target.'cfg(target_os = "windows")'.dependencies]`, so a platform dependency
  can never break the other OS.
- **`get_platform_info` command** — the first real React → Tauri → Rust →
  React round trip, returning OS, architecture, OS version, display server
  (Linux) and the PULSE version.
- **Application shell** — a dark-themed interface with client-side navigation
  across Overview, Gaming, Development, Personal and Mini.
- **Status bar** — reports the platform detected by the backend, and degrades
  gracefully when PULSE runs outside the Tauri runtime.
- **Linux platform support** — `/etc/os-release` parsing and Wayland/X11
  display-server detection.
- **Windows platform support** — OS version reporting, distinguishing
  Windows 10 from Windows 11 by build number.
- **Tests** — 14 Rust unit tests and 9 frontend tests covering platform
  detection, payload serialisation, parsing logic, navigation and graceful
  degradation.
- **CI** — GitHub Actions on Ubuntu and Windows: install, lint, typecheck,
  tests and builds for both the frontend and the Rust backend.
- **Documentation** — architecture overview, Mini overlay design notes, Fedora
  and Windows platform guides, metrics and widget contracts, getting-started
  and testing guides.
- **Repository hygiene** — proprietary licence, contributing guide, security policy,
  issue and pull request templates, EditorConfig, Prettier, ESLint, rustfmt and
  Clippy configuration.

### Notes

- `ubuntu-latest` in CI catches Linux regressions but is **not** a Fedora test.
  Real Fedora validation is manual.
- The `0.1.0-dev` version string is valid SemVer but not valid for MSI
  packaging, which requires a strictly numeric version. This must be resolved
  before the first Windows installer is produced.

[Unreleased]: https://github.com/pulse-monitor/pulse/compare/v0.1.0-dev...HEAD
[0.1.0-dev]: https://github.com/pulse-monitor/pulse/releases/tag/v0.1.0-dev
