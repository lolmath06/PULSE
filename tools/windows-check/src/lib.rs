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

#[path = "../../../src-tauri/src/services/mod.rs"]
pub mod services;
