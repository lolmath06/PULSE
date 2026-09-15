# Changelog

All notable changes to PULSE are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
