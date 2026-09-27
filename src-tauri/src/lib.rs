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
//! history/    -- persistent metric history (one scheduler, SQLite)
//!    |
//! platform/   -- the single place where OS differences live
//!    +-- linux/    (/proc, /sys, hwmon, Wayland/X11)
//!    +-- windows/  (WMI, PDH, vendor SDKs)
//! ```
//!
//! No module above `platform` may contain `#[cfg(target_os = ...)]` branches
//! for system access, and the frontend never reads the system directly.

pub mod commands;
pub mod history;
pub mod metrics;
pub mod platform;
pub mod processes;
pub mod services;
pub mod state;

/// The event the history scheduler emits after each batch.
///
/// The payload is a [`history::BatchRecorded`] — batch id, timestamp, row
/// count — never the values themselves: listeners re-query what they display.
pub const HISTORY_EVENT: &str = "history-sample-recorded";

/// Forwards history batches to the webview as [`HISTORY_EVENT`].
struct TauriHistoryEvents(tauri::AppHandle);

impl history::HistoryEventSink for TauriHistoryEvents {
    fn batch_recorded(&self, event: &history::BatchRecorded) {
        use tauri::Emitter;
        if let Err(error) = self.0.emit(HISTORY_EVENT, event) {
            eprintln!("PULSE: could not emit {HISTORY_EVENT}: {error}");
        }
    }
}

/// Builds and runs the PULSE application.
pub fn run() {
    let app = tauri::Builder::default()
        .manage(state::AppState::new())
        .setup(|app| {
            use std::sync::Arc;
            use tauri::Manager;

            // History needs the engine and the platform's data directory, so
            // it starts here rather than in `AppState::new`. It is not a
            // provider: the engine and its six providers are untouched.
            let engine = app.state::<state::AppState>().metrics_handle();
            let data_dir = app
                .path()
                .app_local_data_dir()
                .map_err(|error| error.to_string());
            let events = Arc::new(TauriHistoryEvents(app.handle().clone()));
            let history = services::history::start_history(engine, data_dir, events);
            app.manage(Arc::new(history));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::platform::get_platform_info,
            commands::metrics::get_metrics_engine_status,
            commands::metrics::get_metric_catalog,
            commands::metrics::sample_metrics,
            commands::history::get_history_status,
            commands::history::get_metric_history,
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
        .build(tauri::generate_context!())
        .expect("error while building PULSE");

    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            use tauri::Manager;
            // Finish the batch in flight and let the store close cleanly, so
            // the WAL is checkpointed and no scheduler thread outlives PULSE.
            if let Some(history) = handle.try_state::<std::sync::Arc<history::HistoryService>>() {
                history.shutdown();
            }
        }
    });
}
