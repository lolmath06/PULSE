//! PULSE's platform and metrics layers, compiled for a Windows target from a
//! non-Windows host.
//!
//! The modules below are **the application's own source files**, included by
//! path. Nothing is copied: a change to `src-tauri/src/platform/windows/` is
//! checked here on the next run, and a file that only compiles on Fedora fails
//! here immediately.
//!
//! See `Cargo.toml` for what this harness does and does not prove, and
//! `docs/platforms/windows.md` for how to run it.

#[path = "../../../src-tauri/src/metrics/mod.rs"]
pub mod metrics;

#[path = "../../../src-tauri/src/platform/mod.rs"]
pub mod platform;

// The process snapshot service and its Windows collector are tauri-free too,
// so the harness type checks them as well: the Toolhelp enumeration, the
// FILETIME arithmetic, the handle wrappers and the snapshot service itself.
#[path = "../../../src-tauri/src/processes/mod.rs"]
pub mod processes;

// Phase 10: the persistent history — SQLite store, migrations, queries,
// retention, the scheduler and the database-path abstraction. All tauri-free,
// so the real Windows persistence code is type checked here, not a stub.
#[path = "../../../src-tauri/src/history/mod.rs"]
pub mod history;

// Phase 11: the shared UI configuration store — atomic writes, backups,
// corruption recovery and the coalescing writer.
#[path = "../../../src-tauri/src/live/mod.rs"]
pub mod live;

// Phase 11: the overlay core — capabilities per display server, monitor/DPI
// geometry, overlay specs, settings and the global-shortcut conflict logic.
// The Tauri calls that apply them (`src-tauri/src/desktop.rs`) need the full
// Tauri crate, which this harness cannot build for Windows from Fedora.
#[path = "../../../src-tauri/src/overlay/mod.rs"]
pub mod overlay;

#[path = "../../../src-tauri/src/ui_config/mod.rs"]
pub mod ui_config;

#[path = "../../../src-tauri/src/services/mod.rs"]
pub mod services;
