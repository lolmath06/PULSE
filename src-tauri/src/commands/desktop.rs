//! Tauri commands for overlays, the Mini window and application lifetime.

use tauri::AppHandle;

use crate::desktop::{self, DesktopStatus, OverlayAction, TrayLabels};

/// What overlays can do on this session, and the global shortcut's state.
#[tauri::command]
pub fn get_desktop_status(app: AppHandle) -> Option<DesktopStatus> {
    desktop::status(&app)
}

/// Changes the global shortcut. On a conflict (or, with the desktop portal,
/// a declined request) the previous shortcut stays in force and the error
/// says so; the setting is saved only on success. With the portal this waits
/// for the user to answer the desktop's dialog.
#[tauri::command]
pub async fn set_overlay_hotkey(
    app: AppHandle,
    shortcut: Option<String>,
) -> Result<Option<String>, String> {
    let wanted = shortcut
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if desktop::gnome_bridge_active(&app) {
        // GNOME delivers the shortcut through the bridge: its binding lives in
        // the extension's own setting, which Mutter watches.
        crate::gnome_bridge::write_hotkey(wanted.as_deref())?;
        desktop::refresh_gnome_bridge(&app);
    } else {
        desktop::apply_hotkey(&app, wanted.as_deref(), true)?;
    }
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

/// Gathers the GNOME bridge facts again (the user pressed Refresh, or the
/// overlay page opened) and returns the new status.
#[tauri::command]
pub async fn refresh_gnome_bridge(app: AppHandle) -> Option<DesktopStatus> {
    desktop::refresh_gnome_bridge(&app);
    desktop::status(&app)
}

/// Enables or disables the GNOME bridge extension through GNOME Shell's own
/// API — only ever from the user's Enable / Disable button.
#[tauri::command]
pub async fn set_gnome_bridge_enabled(
    app: AppHandle,
    enabled: bool,
) -> Result<Option<DesktopStatus>, String> {
    desktop::set_gnome_bridge_enabled(&app, enabled)?;
    Ok(desktop::status(&app))
}

/// Relabels the tray menu in the interface language (sent by the main window
/// at startup and whenever the language changes).
#[tauri::command]
pub fn set_tray_labels(app: AppHandle, labels: TrayLabels) -> Result<(), String> {
    desktop::set_tray_labels(&app, labels).map_err(|error| error.to_string())
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
