# Changelog

All notable changes to PULSE are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
