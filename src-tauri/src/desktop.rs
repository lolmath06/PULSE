//! Desktop integration: overlay windows, the Mini window, the tray, the global
//! shortcut and what closing the main window does.
//!
//! Every *decision* is made by the platform-independent `crate::overlay`
//! module; this file only applies them through Tauri, and contains no
//! `cfg(target_os)` branch. Overlay windows are owned by the backend: the
//! `overlays` configuration section says which exist, and [`reconcile`] makes
//! the windows match it — whoever changed it (an editor in the main window, the
//! global shortcut, the tray).
//!
//! Two threading rules, both about deadlocks:
//!
//! - windows are created from the async runtime, never from a synchronous
//!   command (which would deadlock on Windows);
//! - the global-shortcut plugin blocks on a main-thread round trip, so it is
//!   never called from the main thread or during `setup`.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{
    AppHandle, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::commands::ui_config::{update_section, UiConfigState};
use crate::overlay::capabilities::{
    capabilities, detect_display_server, DisplayServer, OverlayCapabilities, RuntimeFacts,
    SessionFacts,
};
use crate::overlay::geometry::{capture, place, MonitorInfo, OverlayGeometry};
use crate::overlay::settings::{
    allow_exit, handle_main_close, parse_settings, HotkeyManager, ShortcutRegistrar,
};
use crate::overlay::spec::{self, id_from_label, label_for, parse_overlays, OverlaySpec};

pub const MINI_LABEL: &str = "mini";
pub const MAIN_LABEL: &str = "main";

/// How long geometry changes from a drag are collected before being saved.
const GEOMETRY_DEBOUNCE: Duration = Duration::from_millis(300);

/// Backend state for the desktop integration.
pub struct DesktopState {
    pub display: DisplayServer,
    hotkey: Mutex<HotkeyManager>,
    hotkey_fact: Mutex<Option<Result<String, String>>>,
    tray_fact: Mutex<Option<Result<(), String>>>,
    reconcile: Mutex<()>,
    applied: Mutex<HashMap<String, OverlayGeometry>>,
    pending: Mutex<HashMap<String, OverlayGeometry>>,
    flush_scheduled: AtomicBool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl DesktopState {
    pub fn new() -> Self {
        Self {
            display: detect_display_server(&SessionFacts::from_env()),
            hotkey: Mutex::new(HotkeyManager::default()),
            hotkey_fact: Mutex::new(None),
            tray_fact: Mutex::new(None),
            reconcile: Mutex::new(()),
            applied: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            flush_scheduled: AtomicBool::new(false),
        }
    }

    /// Whether this session can place windows and read their position.
    pub fn can_position(&self) -> bool {
        matches!(
            self.display,
            DisplayServer::Windows | DisplayServer::X11 | DisplayServer::XWayland
        )
    }
}

impl Default for DesktopState {
    fn default() -> Self {
        Self::new()
    }
}

// --- configuration access ----------------------------------------------------

fn section<R: Runtime>(app: &AppHandle<R>, name: &str) -> Option<Value> {
    app.try_state::<UiConfigState>()
        .and_then(|config| config.store.section(name))
}

fn overlay_specs<R: Runtime>(app: &AppHandle<R>) -> Vec<OverlaySpec> {
    parse_overlays(section(app, "overlays").as_ref())
}

// --- monitors ----------------------------------------------------------------

fn monitors<R: Runtime>(app: &AppHandle<R>) -> (Vec<MonitorInfo>, Option<usize>) {
    let list: Vec<MonitorInfo> = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|monitor| MonitorInfo {
            name: monitor.name().cloned(),
            x: monitor.position().x,
            y: monitor.position().y,
            width: monitor.size().width,
            height: monitor.size().height,
            scale: monitor.scale_factor(),
        })
        .collect();
    let primary = app.primary_monitor().ok().flatten().and_then(|primary| {
        list.iter().position(|m| {
            m.name.as_ref() == primary.name()
                && m.x == primary.position().x
                && m.y == primary.position().y
        })
    });
    (list, primary)
}

// --- overlay windows -----------------------------------------------------------

/// Applies Edit or Locked to an overlay window.
///
/// Locked: clicks pass through (where the platform honours it) and the window
/// cannot take focus — a game keeps the keyboard. Resizing needs the Edit
/// grip, so a locked overlay cannot be resized by accident either; the window
/// itself stays "resizable", because GTK ignores the requested size of a
/// non-resizable window and falls back to the web view's natural height.
fn apply_lock<R: Runtime>(window: &WebviewWindow<R>, locked: bool) {
    let _ = window.set_ignore_cursor_events(locked);
    let _ = window.set_focusable(!locked);
    let _ = window.set_always_on_top(true);
}

fn create_overlay<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    spec: &OverlaySpec,
) -> tauri::Result<()> {
    let label = label_for(&spec.id);
    let (screens, primary) = monitors(app);
    let placement = place(&spec.geometry, &screens, primary);
    let (width, height) = placement
        .as_ref()
        .map(|p| (p.width, p.height))
        .unwrap_or((spec.geometry.width, spec.geometry.height));
    let url = WebviewUrl::App(format!("index.html?window=overlay&id={}", spec.id).into());

    let window = WebviewWindowBuilder::new(app, &label, url)
        .title(format!("PULSE — {}", spec.name))
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .resizable(true)
        // Never take focus when appearing — even in Edit mode; the lock state
        // (and so focusability) is applied once the window is shown.
        .focused(false)
        .focusable(false)
        .inner_size(width, height)
        .min_inner_size(24.0, 16.0)
        .visible(false)
        .build()?;

    if state.can_position() {
        if let Some(placement) = &placement {
            let _ = window.set_position(PhysicalPosition::new(
                placement.physical_x,
                placement.physical_y,
            ));
        }
    }
    lock(&state.applied).insert(spec.id.clone(), spec.geometry.clone());
    window.show()?;
    apply_lock(&window, spec.locked);
    Ok(())
}

/// Makes the overlay windows match the configuration: creates the visible
/// ones that are missing, closes the ones hidden or deleted, applies the lock
/// state, and moves or resizes a window only when the stored geometry changed
/// from what was last applied or captured (so a drag never fights itself).
pub fn reconcile<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    let _guard = lock(&state.reconcile);
    let specs = overlay_specs(app);

    for (label, window) in app.webview_windows() {
        let Some(id) = id_from_label(&label) else {
            continue;
        };
        if !specs.iter().any(|spec| spec.id == id && spec.visible) {
            let _ = window.destroy();
            lock(&state.applied).remove(id);
        }
    }

    for spec in specs.iter().filter(|spec| spec.visible) {
        match app.get_webview_window(&label_for(&spec.id)) {
            None => {
                if let Err(error) = create_overlay(app, &state, spec) {
                    eprintln!("PULSE: overlay '{}' could not be created: {error}", spec.id);
                }
            }
            Some(window) => {
                apply_lock(&window, spec.locked);
                let previous = lock(&state.applied).get(&spec.id).cloned();
                if previous.as_ref() != Some(&spec.geometry) {
                    let (screens, primary) = monitors(app);
                    if let Some(placement) = place(&spec.geometry, &screens, primary) {
                        let _ = window
                            .set_size(tauri::LogicalSize::new(placement.width, placement.height));
                        if state.can_position() {
                            let _ = window.set_position(PhysicalPosition::new(
                                placement.physical_x,
                                placement.physical_y,
                            ));
                        }
                    }
                    lock(&state.applied).insert(spec.id.clone(), spec.geometry.clone());
                }
            }
        }
    }
}

/// Runs [`reconcile`] off the calling thread.
pub fn schedule_reconcile<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { reconcile(&app) });
}

/// Called after any change to the `overlays` section.
pub fn on_overlays_changed<R: Runtime>(app: &AppHandle<R>) {
    schedule_reconcile(app);
}

/// Called after any change to the `settings` section.
pub fn on_settings_changed<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let wanted = parse_settings(section(&app, "settings").as_ref()).hotkey;
        let _ = apply_hotkey(&app, wanted.as_deref());
    });
}

/// Records a moved or resized overlay, debounced into one configuration write.
fn record_geometry<R: Runtime>(app: &AppHandle<R>, id: &str, window: &tauri::Window<R>) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    let Some(spec) = overlay_specs(app).into_iter().find(|spec| spec.id == id) else {
        return;
    };
    if spec.locked {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    let Ok(size) = window.inner_size() else {
        return;
    };
    let logical = size.to_logical::<f64>(scale);
    let geometry = if state.can_position() {
        let Ok(position) = window.outer_position() else {
            return;
        };
        let (screens, _) = monitors(app);
        capture(
            position.x,
            position.y,
            logical.width,
            logical.height,
            &screens,
        )
    } else {
        OverlayGeometry {
            width: logical.width,
            height: logical.height,
            ..spec.geometry.clone()
        }
    };
    lock(&state.applied).insert(id.to_string(), geometry.clone());
    lock(&state.pending).insert(id.to_string(), geometry);

    if !state.flush_scheduled.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            std::thread::sleep(GEOMETRY_DEBOUNCE);
            let Some(state) = app.try_state::<DesktopState>() else {
                return;
            };
            state.flush_scheduled.store(false, Ordering::SeqCst);
            let pending: Vec<(String, OverlayGeometry)> = lock(&state.pending).drain().collect();
            let keep_position = !state.can_position();
            update_section(&app, "overlays", "backend", |value| {
                let mut changed = false;
                for (id, geometry) in &pending {
                    changed |= spec::set_geometry(value, id, geometry, keep_position);
                }
                changed
            });
        });
    }
}

/// Window events the desktop layer cares about.
pub fn on_window_event<R: Runtime>(
    app: &AppHandle<R>,
    window: &tauri::Window<R>,
    event: &WindowEvent,
) {
    let label = window.label().to_string();
    if let Some(id) = id_from_label(&label) {
        match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => record_geometry(app, id, window),
            // Closed from the window manager (Alt+F4, a panel): hide it in the
            // configuration rather than destroying a window the configuration
            // would recreate. It comes back with "Show all".
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let id = id.to_string();
                update_section(app, "overlays", "backend", |value| {
                    let mut changed = false;
                    if let Some(items) = value.get_mut("items").and_then(Value::as_array_mut) {
                        for item in items {
                            if item.get("id").and_then(Value::as_str) == Some(id.as_str()) {
                                item["visible"] = Value::Bool(false);
                                changed = true;
                            }
                        }
                    }
                    changed
                });
                // Never leave PULSE running with nothing on screen: if the main
                // window was hidden (keep-running) and this was the last
                // visible overlay, bring the main window back.
                let main_hidden = app
                    .get_webview_window(MAIN_LABEL)
                    .and_then(|main| main.is_visible().ok())
                    == Some(false);
                if main_hidden && !spec::any_visible(section(app, "overlays").as_ref()) {
                    show_main(app);
                }
            }
            _ => {}
        }
        return;
    }
    if label == MAIN_LABEL {
        if let WindowEvent::CloseRequested { api, .. } = event {
            let settings = parse_settings(section(app, "settings").as_ref());
            let visible = spec::visible_count(section(app, "overlays").as_ref());
            let action = handle_main_close(
                settings.close_behavior,
                visible,
                || api.prevent_close(),
                || {
                    let _ = window.hide();
                },
                || app.exit(0),
            );
            eprintln!(
                "PULSE: main window closed — {:?} ({:?}, {visible} visible overlay(s))",
                action, settings.close_behavior
            );
        }
    }
}

// --- global actions --------------------------------------------------------------

/// What the tray, the shortcut and the overlay editor can do to every overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OverlayAction {
    LockAll,
    EditAll,
    ToggleLockAll,
    ShowAll,
    HideAll,
    ToggleVisibleAll,
}

pub fn overlay_action<R: Runtime>(app: &AppHandle<R>, action: OverlayAction) {
    update_section(app, "overlays", "backend", |value| match action {
        OverlayAction::LockAll => spec::set_all_locked(value, true),
        OverlayAction::EditAll => {
            let unlocked = spec::set_all_locked(value, false);
            // Editing hidden overlays is pointless: show them too.
            spec::set_all_visible(value, true) || unlocked
        }
        OverlayAction::ToggleLockAll => spec::toggle_all_locked(value).is_some(),
        OverlayAction::ShowAll => spec::set_all_visible(value, true),
        OverlayAction::HideAll => spec::set_all_visible(value, false),
        OverlayAction::ToggleVisibleAll => {
            let visible = spec::any_visible(Some(value));
            spec::set_all_visible(value, !visible)
        }
    });
}

/// Brings the main window back: the same window if it was hidden, or — should
/// it ever have been destroyed — a new one built from the same configuration
/// and label, so there is never a second main window.
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    eprintln!("PULSE: showing the main window");
    let window = match app.get_webview_window(MAIN_LABEL) {
        Some(window) => window,
        None => {
            let Some(config) = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == MAIN_LABEL)
                .cloned()
            else {
                return;
            };
            match WebviewWindowBuilder::from_config(app, &config)
                .and_then(|builder| builder.build())
            {
                Ok(window) => window,
                Err(error) => {
                    eprintln!("PULSE: the main window could not be recreated: {error}");
                    return;
                }
            }
        }
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// Whether Tauri may exit on its own (its last window was destroyed). An
/// explicit Quit never asks this; see `overlay::settings::allow_exit`.
pub fn allow_implicit_exit<R: Runtime>(app: &AppHandle<R>) -> bool {
    let settings = parse_settings(section(app, "settings").as_ref());
    let visible = spec::visible_count(section(app, "overlays").as_ref());
    allow_exit(false, settings.close_behavior, visible)
}

pub fn open_mini<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MINI_LABEL) {
        window.show()?;
        window.unminimize()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(
        app,
        MINI_LABEL,
        WebviewUrl::App("index.html?window=mini".into()),
    )
    .title("PULSE Mini")
    .inner_size(380.0, 280.0)
    .min_inner_size(200.0, 120.0)
    .resizable(true)
    .build()?;
    Ok(())
}

// --- global shortcut -------------------------------------------------------------

struct PluginRegistrar<'a, R: Runtime>(&'a AppHandle<R>);

impl<R: Runtime> ShortcutRegistrar for PluginRegistrar<'_, R> {
    fn register(&mut self, shortcut: &str) -> Result<(), String> {
        let parsed = Shortcut::from_str(shortcut)
            .map_err(|error| format!("not a valid shortcut ({error})"))?;
        self.0
            .global_shortcut()
            .register(parsed)
            .map_err(|error| error.to_string())
    }

    fn unregister(&mut self, shortcut: &str) {
        if let Ok(parsed) = Shortcut::from_str(shortcut) {
            let _ = self.0.global_shortcut().unregister(parsed);
        }
    }
}

/// Registers `wanted` as the shortcut. Must not run on the main thread.
pub fn apply_hotkey<R: Runtime>(app: &AppHandle<R>, wanted: Option<&str>) -> Result<(), String> {
    let Some(state) = app.try_state::<DesktopState>() else {
        return Err("desktop integration not ready".into());
    };
    let result = lock(&state.hotkey).apply(&mut PluginRegistrar(app), wanted);
    *lock(&state.hotkey_fact) = Some(match (&result, wanted) {
        (Ok(()), Some(shortcut)) => Ok(shortcut.to_string()),
        (Ok(()), None) => Err("no shortcut configured".into()),
        (Err(error), _) => Err(error.clone()),
    });
    result
}

/// The plugin's handler: the configured shortcut toggles Edit ↔ Locked.
pub fn on_shortcut<R: Runtime>(app: &AppHandle<R>, shortcut: &Shortcut, pressed: ShortcutState) {
    if pressed != ShortcutState::Pressed {
        return;
    }
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    let ours = lock(&state.hotkey)
        .current()
        .and_then(|current| Shortcut::from_str(current).ok())
        .is_some_and(|current| &current == shortcut);
    if ours {
        overlay_action(app, OverlayAction::ToggleLockAll);
    }
}

// --- tray --------------------------------------------------------------------------

/// Builds the tray once. A failure (no StatusNotifier host, no icon) is
/// recorded as a capability fact, never fatal.
pub fn build_tray<R: Runtime>(app: &AppHandle<R>) {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::TrayIconBuilder;

    let result = (|| -> tauri::Result<()> {
        let open = MenuItem::with_id(app, "open", "Open PULSE", true, None::<&str>)?;
        let edit = MenuItem::with_id(app, "edit", "Edit overlays", true, None::<&str>)?;
        let lock_all = MenuItem::with_id(app, "lock", "Lock overlays", true, None::<&str>)?;
        let toggle = MenuItem::with_id(app, "toggle", "Show / hide overlays", true, None::<&str>)?;
        let separator = PredefinedMenuItem::separator(app)?;
        let quit = MenuItem::with_id(app, "quit", "Quit PULSE", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&open, &edit, &lock_all, &toggle, &separator, &quit])?;
        let mut builder = TrayIconBuilder::with_id("pulse")
            .tooltip("PULSE")
            .menu(&menu)
            .show_menu_on_left_click(true)
            .on_menu_event(|app, event| match event.id().as_ref() {
                "open" => show_main(app),
                "edit" => overlay_action(app, OverlayAction::EditAll),
                "lock" => overlay_action(app, OverlayAction::LockAll),
                "toggle" => overlay_action(app, OverlayAction::ToggleVisibleAll),
                "quit" => app.exit(0),
                _ => {}
            });
        if let Some(icon) = app.default_window_icon() {
            builder = builder.icon(icon.clone());
        }
        builder.build(app)?;
        Ok(())
    })();
    if let Some(state) = app.try_state::<DesktopState>() {
        *lock(&state.tray_fact) = Some(
            result
                .as_ref()
                .map(|_| ())
                .map_err(|error| error.to_string()),
        );
    }
    if let Err(error) = result {
        eprintln!("PULSE: tray unavailable: {error}");
    }
}

// --- capabilities -------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopStatus {
    pub capabilities: OverlayCapabilities,
    pub hotkey: Option<String>,
    pub hotkey_error: Option<String>,
}

pub fn status<R: Runtime>(app: &AppHandle<R>) -> Option<DesktopStatus> {
    let state = app.try_state::<DesktopState>()?;
    let runtime = RuntimeFacts {
        hotkey: lock(&state.hotkey_fact).clone(),
        tray: lock(&state.tray_fact).clone(),
    };
    let manager = lock(&state.hotkey);
    Some(DesktopStatus {
        capabilities: capabilities(state.display, &runtime),
        hotkey: manager.current().map(String::from),
        hotkey_error: manager.last_error().map(String::from),
    })
}

/// Everything set up at launch: state, tray, then — off the main thread —
/// the saved shortcut and the saved overlays.
pub fn setup<R: Runtime>(app: &AppHandle<R>) {
    app.manage(DesktopState::new());
    build_tray(app);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let wanted = parse_settings(section(&handle, "settings").as_ref()).hotkey;
        if let Err(error) = apply_hotkey(&handle, wanted.as_deref()) {
            eprintln!("PULSE: global shortcut unavailable: {error}");
        }
        reconcile(&handle);
        if let Some(status) = status(&handle) {
            let caps = &status.capabilities;
            eprintln!(
                "PULSE: desktop {:?}: always-on-top {:?}, click-through {:?}, positioning {:?}, hotkey {:?} ({}), tray {:?}",
                caps.display_server,
                caps.always_on_top.status,
                caps.click_through.status,
                caps.positioning.status,
                caps.global_hotkey.status,
                status.hotkey.as_deref().unwrap_or("none"),
                caps.tray.status,
            );
        }
    });
}
