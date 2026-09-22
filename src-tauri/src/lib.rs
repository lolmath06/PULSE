//! PULSE backend library.
//!
//! Layering (see `docs/architecture/overview.md`):
//!
//! ```text
//! React UI
//!    |
//! commands/   -- Tauri command surface, the only thing the UI can reach
//!    |
//! services/   -- cross-platform application logic
//!    |
//! metrics/    -- the metrics engine: model, catalog, providers
//!    |
//! platform/   -- the single place where OS differences live
//!    +-- linux/    (/proc, /sys, hwmon, Wayland/X11)
//!    +-- windows/  (WMI, PDH, vendor SDKs)
//! ```
//!
//! No module above `platform` may contain `#[cfg(target_os = ...)]` branches
//! for system access, and the frontend never reads the system directly.

pub mod commands;
pub mod metrics;
pub mod platform;
pub mod processes;
pub mod services;
pub mod state;

/// Builds and runs the PULSE application.
pub fn run() {
    tauri::Builder::default()
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::platform::get_platform_info,
            commands::metrics::get_metrics_engine_status,
            commands::metrics::get_metric_catalog,
            commands::metrics::sample_metrics,
            commands::processes::get_process_snapshot,
            commands::process_control::get_process_details,
            commands::process_control::get_process_provenance,
            commands::process_control::compute_process_sha256,
            commands::process_control::get_process_priority,
            commands::process_control::get_process_affinity,
            commands::process_control::suspend_process,
            commands::process_control::resume_process,
            commands::process_control::terminate_process,
            commands::process_control::terminate_process_tree,
            commands::process_control::set_process_priority,
            commands::process_control::set_process_affinity,
            commands::process_control::open_process_location,
            commands::process_control::open_web_search,
            commands::process_control::open_hash_lookup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PULSE");
}
