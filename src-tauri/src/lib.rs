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

pub mod bridge;
pub mod commands;
pub mod desktop;
pub mod gnome_bridge;
pub mod history;
pub mod live;
pub mod metrics;
pub mod overlay;
pub mod overlay_native;
pub mod platform;
pub mod portal;
pub mod processes;
pub mod services;
pub mod state;
pub mod ui_config;

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
    platform::prepare_runtime_environment();

    let app = tauri::Builder::default()
        .manage(state::AppState::new())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    desktop::on_shortcut(app, shortcut, event.state());
                })
                .build(),
        )
        .on_window_event(|window, event| {
            use tauri::Manager;
            desktop::on_window_event(window.app_handle(), window, event);
        })
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

            // The shared UI configuration, in the app's config directory. Never
            // fails: an unreadable file means defaults, and the load outcome
            // says why.
            let config_path = match app.path().app_config_dir() {
                Ok(dir) => ui_config::config_path(&dir),
                Err(error) => {
                    eprintln!("PULSE: no config directory ({error}); using a temporary one");
                    ui_config::config_path(&std::env::temp_dir().join("dev.pulse.app"))
                }
            };
            let store = Arc::new(ui_config::UiConfigStore::open(&config_path));
            eprintln!(
                "PULSE: UI configuration {:?}: {}",
                store.load_outcome(),
                config_path.display()
            );
            let writer = ui_config::writer::ConfigWriter::spawn(
                Arc::clone(&store),
                ui_config::writer::QUIET_PERIOD,
            );
            app.manage(commands::ui_config::UiConfigState { store, writer });

            // The live widget feed: one shared 1-second sampler for whatever
            // the visible widgets show, never persisted. It sleeps while no
            // window subscribes.
            let live = live::LiveService::start(
                app.state::<state::AppState>().metrics_handle(),
                Arc::new(history::SystemClock),
                Arc::new(commands::live::TauriLiveEvents(app.handle().clone())),
                live::LIVE_CADENCE,
            );
            app.manage(Arc::new(live));

            // Overlays, the tray and the global shortcut — after the config
            // store, which says which overlays exist.
            desktop::setup(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::platform::get_platform_info,
            commands::metrics::get_metrics_engine_status,
            commands::metrics::get_metric_catalog,
            commands::metrics::sample_metrics,
            commands::history::get_history_status,
            commands::history::get_metric_history,
            commands::metrics::get_source_refs,
            commands::live::set_live_subscription,
            commands::live::get_live_buffer,
            commands::desktop::get_desktop_status,
            commands::desktop::set_overlay_hotkey,
            commands::desktop::refresh_gnome_bridge,
            commands::desktop::set_gnome_bridge_enabled,
            commands::desktop::overlay_action,
            commands::desktop::set_tray_labels,
            commands::desktop::open_main_window,
            commands::desktop::open_mini_window,
            commands::desktop::quit_app,
            commands::ui_config::get_ui_config,
            commands::ui_config::set_ui_config_section,
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
        if let tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::Destroyed,
            ..
        } = &event
        {
            use tauri::Manager;
            // A closed window no longer needs its live metrics.
            if let Some(live) = handle.try_state::<std::sync::Arc<live::LiveService>>() {
                live.remove_subscriber(label);
            }
        }
        if let tauri::RunEvent::ExitRequested { code, api, .. } = &event {
            // `code: None` is Tauri's implicit exit (its last window was
            // destroyed); `Some` is an explicit Quit, which always quits.
            if code.is_none() && !desktop::allow_implicit_exit(handle) {
                api.prevent_exit();
                eprintln!("PULSE: implicit exit refused — keep running with visible overlays");
            } else {
                eprintln!(
                    "PULSE: exit requested (code {code:?}) at {:?}",
                    std::time::SystemTime::now()
                );
            }
        }
        if let tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::Destroyed,
            ..
        } = &event
        {
            eprintln!("PULSE: window destroyed: {label}");
        }
        if let tauri::RunEvent::Exit = event {
            use tauri::Manager;
            let started = std::time::Instant::now();
            eprintln!(
                "PULSE: RunEvent::Exit at {:?}",
                std::time::SystemTime::now()
            );
            // Release the portal shortcut session (bounded wait).
            desktop::shutdown(handle);
            eprintln!(
                "PULSE: desktop shutdown complete (+{:?})",
                started.elapsed()
            );
            // Finish the batch in flight and let the store close cleanly, so
            // the WAL is checkpointed and no scheduler thread outlives PULSE.
            if let Some(history) = handle.try_state::<std::sync::Arc<history::HistoryService>>() {
                history.shutdown();
            }
            eprintln!(
                "PULSE: history shutdown complete (+{:?})",
                started.elapsed()
            );
            if let Some(live) = handle.try_state::<std::sync::Arc<live::LiveService>>() {
                live.shutdown();
            }
            eprintln!("PULSE: live shutdown complete (+{:?})", started.elapsed());
            // Write any configuration change still inside its quiet period.
            if let Some(config) = handle.try_state::<commands::ui_config::UiConfigState>() {
                config.writer.shutdown();
            }
            eprintln!("PULSE: config shutdown complete (+{:?})", started.elapsed());
        }
    });
}
