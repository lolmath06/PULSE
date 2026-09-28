//! Tauri commands for the shared UI configuration.
//!
//! Every window — main, Mini, each overlay — reads and writes the same
//! document here. A write is validated by [`UiConfigStore`], broadcast to all
//! windows as [`UI_CONFIG_EVENT`], and saved atomically by the writer thread.

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime, State, WebviewWindow};

use crate::ui_config::writer::ConfigWriter;
use crate::ui_config::{UiConfigSnapshot, UiConfigStore};

/// Broadcast after every accepted change, to every window.
pub const UI_CONFIG_EVENT: &str = "ui-config-changed";

/// The store and its writer, managed by Tauri.
#[derive(Debug)]
pub struct UiConfigState {
    pub store: Arc<UiConfigStore>,
    pub writer: ConfigWriter,
}

/// The payload of [`UI_CONFIG_EVENT`]. Carries the section's new value, so a
/// window never has to ask again.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiConfigChanged {
    pub revision: u64,
    pub section: String,
    /// The window label that made the change, or `backend`.
    pub origin: String,
    pub value: Value,
}

/// Applies a change on behalf of `origin`, then broadcasts and schedules a save.
pub fn apply_section<R: Runtime>(
    app: &AppHandle<R>,
    section: &str,
    value: Value,
    origin: &str,
) -> Result<u64, String> {
    let state = app.state::<UiConfigState>();
    let revision = state
        .store
        .set_section(section, value.clone())
        .map_err(|error| error.to_string())?;
    state.writer.notify();
    let _ = app.emit(
        UI_CONFIG_EVENT,
        UiConfigChanged {
            revision,
            section: section.to_string(),
            origin: origin.to_string(),
            value,
        },
    );
    if section == "overlays" {
        crate::desktop::on_overlays_changed(app);
    }
    if section == "settings" {
        crate::desktop::on_settings_changed(app);
    }
    Ok(revision)
}

/// Changes a section in place on the backend's behalf (the shortcut, the tray,
/// a dragged overlay). `change` returns whether it changed anything; only then
/// is the section saved and broadcast.
pub fn update_section<R: Runtime>(
    app: &AppHandle<R>,
    section: &str,
    origin: &str,
    change: impl FnOnce(&mut Value) -> bool,
) -> Option<u64> {
    let state = app.try_state::<UiConfigState>()?;
    let mut value = state
        .store
        .section(section)
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    if !change(&mut value) {
        return None;
    }
    apply_section(app, section, value, origin).ok()
}

/// The whole document, its revision and how it was loaded.
#[tauri::command]
pub fn get_ui_config(state: State<'_, UiConfigState>) -> UiConfigSnapshot {
    state.store.snapshot()
}

/// Replaces one top-level section. Returns the new revision.
#[tauri::command]
pub fn set_ui_config_section(
    app: AppHandle,
    window: WebviewWindow,
    section: String,
    value: Value,
) -> Result<u64, String> {
    apply_section(&app, &section, value, window.label())
}
