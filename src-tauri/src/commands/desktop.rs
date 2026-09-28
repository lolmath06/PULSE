//! Tauri commands for overlays, the Mini window and application lifetime.

use tauri::AppHandle;

use crate::desktop::{self, DesktopStatus, OverlayAction};

/// What overlays can do on this session, and the global shortcut's state.
#[tauri::command]
pub fn get_desktop_status(app: AppHandle) -> Option<DesktopStatus> {
    desktop::status(&app)
}

/// Changes the global shortcut. On a conflict the previous shortcut stays in
/// force and the error says so; the setting is saved only on success.
#[tauri::command]
pub async fn set_overlay_hotkey(
    app: AppHandle,
    shortcut: Option<String>,
) -> Result<Option<String>, String> {
    let wanted = shortcut
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    desktop::apply_hotkey(&app, wanted.as_deref())?;
    crate::commands::ui_config::update_section(&app, "settings", "backend", |value| {
        let next = serde_json::json!(wanted);
        if value.get("overlayHotkey") == Some(&next) {
            return false;
        }
        value["overlayHotkey"] = next;
        true
    });
    Ok(wanted)
}

/// Locks, unlocks, shows or hides every overlay.
#[tauri::command]
pub async fn overlay_action(app: AppHandle, action: OverlayAction) {
    desktop::overlay_action(&app, action);
}

/// Brings the main window back (from an overlay's *Open PULSE*).
#[tauri::command]
pub async fn open_main_window(app: AppHandle) {
    desktop::show_main(&app);
}

/// Opens the Mini window: a small, ordinary PULSE window.
#[tauri::command]
pub async fn open_mini_window(app: AppHandle) -> Result<(), String> {
    desktop::open_mini(&app).map_err(|error| error.to_string())
}

/// Quits PULSE: overlays close, the live and history schedulers stop, SQLite
/// is checkpointed and the configuration flushed (see `RunEvent::Exit`).
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
