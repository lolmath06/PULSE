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
//!   never called from the main thread or during `setup`; the portal backend
//!   may wait for the user to answer the desktop's dialog, so neither is it.
//!
//! The global shortcut has one backend per session, chosen once at launch by
//! `overlay::global_shortcut::choose_backend`: the plugin on Windows and X11,
//! the XDG Desktop Portal on native Wayland (`crate::portal`), or none.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::commands::ui_config::{update_section, UiConfigState};
use crate::overlay::backend::{
    backend_info, select_backend, window_policy, OverlayBackendInfo, OverlayBackendKind,
};
use crate::overlay::capabilities::{
    capabilities, detect_display_server, DisplayServer, OverlayCapabilities, RuntimeFacts,
    SessionFacts,
};
use crate::overlay::geometry::{capture, place, MonitorInfo, OverlayGeometry};
use crate::overlay::global_shortcut::{choose_backend, ShortcutBackend};
use crate::overlay::gnome_bridge::{bridge_status, GnomeBridgeStatus, GnomeFacts, Handshake};
use crate::overlay::input::{handle_event, InputEvent, OverlayInputMode, OverlayInputs};
use crate::overlay::settings::{
    allow_exit, handle_main_close, parse_settings, HotkeyManager, ShortcutRegistrar,
};
use crate::overlay::spec::{self, id_from_label, label_for, parse_overlays, OverlaySpec};
use crate::portal::PortalService;

pub const MINI_LABEL: &str = "mini";
pub const MAIN_LABEL: &str = "main";

/// How long geometry changes from a drag are collected before being saved.
const GEOMETRY_DEBOUNCE: Duration = Duration::from_millis(300);

/// Backend state for the desktop integration.
pub struct DesktopState {
    pub display: DisplayServer,
    hotkey: Mutex<HotkeyManager>,
    hotkey_fact: Mutex<Option<Result<String, String>>>,
    /// Chosen once at launch; `None` until then.
    hotkey_backend: Mutex<Option<ShortcutBackend>>,
    portal: Mutex<Option<std::sync::Arc<PortalService>>>,
    /// The overlay bridge D-Bus service (Linux only).
    bridge: Mutex<Option<crate::bridge::BridgeService>>,
    /// The GNOME bridge facts, gathered at launch and on refresh — never
    /// polled. `None` until first gathered.
    gnome: Mutex<Option<GnomeFacts>>,
    /// The bridge's own shortcut, read with the facts.
    gnome_hotkey: Mutex<Option<String>>,
    /// The running extension's last `Hello`.
    handshake: std::sync::Arc<Mutex<Option<Handshake>>>,
    tray_fact: Mutex<Option<Result<(), String>>>,
    reconcile: Mutex<()>,
    applied: Mutex<HashMap<String, OverlayGeometry>>,
    /// The input mode each overlay window should have (click-through when
    /// locked); see `overlay::input`.
    inputs: Mutex<OverlayInputs>,
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
            hotkey_backend: Mutex::new(None),
            portal: Mutex::new(None),
            bridge: Mutex::new(None),
            gnome: Mutex::new(None),
            gnome_hotkey: Mutex::new(None),
            handshake: std::sync::Arc::new(Mutex::new(None)),
            tray_fact: Mutex::new(None),
            reconcile: Mutex::new(()),
            applied: Mutex::new(HashMap::new()),
            inputs: Mutex::new(OverlayInputs::default()),
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

/// Applies Edit or Locked to an overlay window. `event` says why: a new
/// window, or a reconciliation (see `overlay::input` for when that reaches
/// the native window).
///
/// Locked: clicks pass through (where the platform honours it) and the window
/// cannot take focus — a game keeps the keyboard. Resizing needs the Edit
/// grip, so a locked overlay cannot be resized by accident either; the window
/// itself stays "resizable", because GTK ignores the requested size of a
/// non-resizable window and falls back to the web view's natural height.
///
/// The input mode is applied first and waited for: by the time this returns,
/// a Locked overlay's native window is click-through and an Edit one takes
/// the pointer again. The frontend hides or shows its edit controls from the
/// same configuration change, independently; neither waits for the other.
fn apply_lock<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>, event: InputEvent) {
    let locked = matches!(
        event,
        InputEvent::Created { locked: true } | InputEvent::Reconciled { locked: true }
    );
    let policy = window_policy(locked);
    if let Some(state) = app.try_state::<DesktopState>() {
        let label = window.label();
        let outcome = handle_event(&state.inputs, label, event, |mode| {
            debug_assert_eq!(mode, policy.input);
            crate::overlay_native::set_overlay_input_mode(window, mode)
        });
        report_input(label, event, outcome);
    }
    crate::overlay_native::apply_focus_and_stacking(window, policy);
}

/// Logs a failed input-mode change, and — with `PULSE_OVERLAY_INPUT_DEBUG`
/// set — every applied one with what the native layer found.
fn report_input(
    label: &str,
    event: InputEvent,
    outcome: Option<(OverlayInputMode, Result<String, String>)>,
) {
    match outcome {
        Some((mode, Err(error))) => {
            eprintln!("PULSE: overlay '{label}' input {mode:?} not applied ({event:?}): {error}")
        }
        Some((mode, Ok(details))) if std::env::var_os("PULSE_OVERLAY_INPUT_DEBUG").is_some() => {
            eprintln!("PULSE: overlay '{label}' input {mode:?} ({event:?}): {details}")
        }
        _ => {}
    }
}

/// Re-applies the desired input mode whenever the overlay's native window is
/// mapped again, since its surface may then be new. Connected once per window.
fn watch_remap<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let app = app.clone();
    let label = window.label().to_string();
    crate::overlay_native::watch_remap(window, move |apply| {
        if let Some(state) = app.try_state::<DesktopState>() {
            let outcome = handle_event(&state.inputs, &label, InputEvent::Mapped, apply);
            report_input(&label, InputEvent::Mapped, outcome);
        }
    });
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
        // The compositor-visible marker the GNOME Shell bridge matches.
        .title(spec::window_title(&spec.id))
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
    watch_remap(app, &window);
    window.show()?;
    apply_lock(
        app,
        &window,
        InputEvent::Created {
            locked: spec.locked,
        },
    );
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
            lock(&state.inputs).on_event(&label, InputEvent::Destroyed);
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
                apply_lock(
                    app,
                    &window,
                    InputEvent::Reconciled {
                        locked: spec.locked,
                    },
                );
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
        let _ = apply_hotkey(&app, wanted.as_deref(), false);
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
    crate::window_native::prepare_main_window(&window);
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

/// Chooses the session's shortcut backend (once) and, for the portal, starts
/// its worker. Must not run on the main thread: it may touch D-Bus.
fn init_hotkey_backend<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    if lock(&state.hotkey_backend).is_some() {
        return;
    }
    let mut connection = None;
    let mut backend = choose_backend(state.display, || {
        let (availability, found) = crate::portal::probe();
        connection = found;
        availability
    });
    if let (ShortcutBackend::Portal { .. }, Some(connection)) = (&backend, connection) {
        let toggler = app.clone();
        match PortalService::start(connection, move || {
            overlay_action(&toggler, OverlayAction::ToggleLockAll);
        }) {
            Ok(service) => *lock(&state.portal) = Some(std::sync::Arc::new(service)),
            Err(error) => {
                backend = ShortcutBackend::Unavailable {
                    reason: format!("the desktop portal could not be used: {error}"),
                }
            }
        }
    }
    eprintln!("PULSE: global shortcut backend: {backend:?}");
    *lock(&state.hotkey_backend) = Some(backend);
}

/// Registers (plugin) or binds (portal) `wanted` as the shortcut. Must not
/// run on the main thread; with the portal it may wait for the user.
///
/// `explicit`: the user asked (see `PortalShortcut::apply`).
pub fn apply_hotkey<R: Runtime>(
    app: &AppHandle<R>,
    wanted: Option<&str>,
    explicit: bool,
) -> Result<(), String> {
    let Some(state) = app.try_state::<DesktopState>() else {
        return Err("desktop integration not ready".into());
    };
    let backend = lock(&state.hotkey_backend).clone();
    let result = match backend {
        Some(ShortcutBackend::Plugin) => {
            lock(&state.hotkey).apply(&mut PluginRegistrar(app), wanted)
        }
        Some(ShortcutBackend::Portal { .. }) => {
            let service = lock(&state.portal).clone();
            match service {
                // Not holding any lock while the desktop's dialog is open.
                Some(service) => service.apply(wanted, explicit),
                None => Err("the portal backend is not running".into()),
            }
        }
        Some(ShortcutBackend::Unavailable { reason }) => match wanted {
            Some(_) => Err(reason),
            None => Ok(()),
        },
        None => Err("desktop integration not ready".into()),
    };
    let effective = lock(&state.portal)
        .as_ref()
        .and_then(|service| service.snapshot().effective);
    *lock(&state.hotkey_fact) = Some(match (&result, wanted) {
        (Ok(()), Some(shortcut)) => Ok(effective.unwrap_or_else(|| shortcut.to_string())),
        (Ok(()), None) => Err("no shortcut configured".into()),
        (Err(error), _) => Err(error.clone()),
    });
    result
}

/// Starts the overlay bridge D-Bus service (prototype) once. It only runs the
/// existing Edit ↔ Locked toggle; a failure is logged once and is not fatal.
fn start_bridge<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    if lock(&state.bridge).is_some() {
        return;
    }
    let toggler = app.clone();
    let handshake = state.handshake.clone();
    let emitter = app.clone();
    let hello = move |version: u32| {
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let previous = lock(&handshake).replace(Handshake { version, at });
        if previous.map(|p| p.version) != Some(version) {
            eprintln!("PULSE: GNOME bridge extension v{version} connected");
        }
        let _ = emitter.emit(DESKTOP_STATUS_EVENT, ());
    };
    match crate::bridge::start(
        move || overlay_action(&toggler, OverlayAction::ToggleLockAll),
        hello,
    ) {
        Ok(Some(service)) => {
            eprintln!(
                "PULSE: overlay bridge on the session bus: {} {}",
                crate::bridge::BUS_NAME,
                crate::bridge::OBJECT_PATH
            );
            *lock(&state.bridge) = Some(service);
        }
        Ok(None) => {}
        Err(error) => eprintln!("PULSE: overlay bridge unavailable: {error}"),
    }
}

/// Gathers the GNOME bridge facts again (GNOME Shell's extension state, the
/// installed copy, the bridge's shortcut). At launch, and when the user
/// refreshes or acts — never on a timer. Must not run on the main thread.
pub fn refresh_gnome_bridge<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    let facts = crate::gnome_bridge::gather(state.display);
    let hotkey = match &facts.shell {
        Some(Ok(shell)) => {
            let loaded_from = shell
                .extension
                .as_ref()
                .and_then(|info| info.path.as_deref());
            crate::gnome_bridge::read_hotkey(loaded_from)
        }
        _ => None,
    };
    *lock(&state.gnome) = Some(facts);
    *lock(&state.gnome_hotkey) = hotkey;
}

/// The bridge's status: the cached facts with the latest handshake.
fn gnome_status(state: &DesktopState) -> Option<GnomeBridgeStatus> {
    let mut facts = lock(&state.gnome).clone()?;
    facts.handshake = *lock(&state.handshake);
    Some(bridge_status(&facts))
}

/// Enables or disables the extension through GNOME Shell's own API, on the
/// user's explicit request, then refreshes.
pub fn set_gnome_bridge_enabled<R: Runtime>(
    app: &AppHandle<R>,
    enabled: bool,
) -> Result<(), String> {
    let result = crate::gnome_bridge::set_enabled(enabled);
    refresh_gnome_bridge(app);
    let _ = app.emit(DESKTOP_STATUS_EVENT, ());
    result
}

/// Whether the GNOME bridge is the overlay backend right now.
pub fn gnome_bridge_active<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<DesktopState>()
        .and_then(|state| gnome_status(&state))
        .is_some_and(|status| status.is_active())
}

/// Emitted when the desktop status changed outside the frontend's request.
pub const DESKTOP_STATUS_EVENT: &str = "desktop-status-changed";

/// Releases the portal session and the bridge when PULSE quits.
pub fn shutdown<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    if let Some(mut bridge) = lock(&state.bridge).take() {
        bridge.stop();
        eprintln!("PULSE: overlay bridge stopped");
    }
    let service = lock(&state.portal).take();
    if let Some(service) = service {
        service.close();
        eprintln!("PULSE: global shortcut portal session closed");
    }
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

/// The tray menu's words. English until the interface sends its own
/// language's (`set_tray_labels`): the native menu cannot read the frontend's
/// translations, so the main window hands them over whenever its language
/// changes. Only the labels change; the item ids and actions never do.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayLabels {
    pub open: String,
    pub edit: String,
    pub lock: String,
    pub toggle: String,
    pub quit: String,
}

impl Default for TrayLabels {
    fn default() -> Self {
        Self {
            open: "Open PULSE".into(),
            edit: "Edit overlays".into(),
            lock: "Lock overlays".into(),
            toggle: "Show / hide overlays".into(),
            quit: "Quit PULSE".into(),
        }
    }
}

impl TrayLabels {
    /// Each label trimmed and bounded; a blank or oversized one keeps its
    /// English default rather than leaving an empty menu entry.
    pub fn sanitized(self) -> Self {
        let defaults = Self::default();
        let pick = |text: String, fallback: String| {
            let trimmed = text.trim();
            if trimmed.is_empty() || trimmed.chars().count() > 64 || trimmed.contains('\n') {
                fallback
            } else {
                trimmed.to_string()
            }
        };
        Self {
            open: pick(self.open, defaults.open),
            edit: pick(self.edit, defaults.edit),
            lock: pick(self.lock, defaults.lock),
            toggle: pick(self.toggle, defaults.toggle),
            quit: pick(self.quit, defaults.quit),
        }
    }
}

fn tray_menu<R: Runtime>(
    app: &AppHandle<R>,
    labels: &TrayLabels,
) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};

    let open = MenuItem::with_id(app, "open", &labels.open, true, None::<&str>)?;
    let edit = MenuItem::with_id(app, "edit", &labels.edit, true, None::<&str>)?;
    let lock_all = MenuItem::with_id(app, "lock", &labels.lock, true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", &labels.toggle, true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", &labels.quit, true, None::<&str>)?;
    Menu::with_items(app, &[&open, &edit, &lock_all, &toggle, &separator, &quit])
}

/// Builds the tray once. A failure (no StatusNotifier host, no icon) is
/// recorded as a capability fact, never fatal.
pub fn build_tray<R: Runtime>(app: &AppHandle<R>) {
    use tauri::tray::TrayIconBuilder;

    let result = (|| -> tauri::Result<()> {
        let menu = tray_menu(app, &TrayLabels::default())?;
        let mut builder = TrayIconBuilder::with_id("pulse")
            .tooltip("PULSE")
            .menu(&menu)
            .show_menu_on_left_click(true)
            .on_menu_event(|app, event| match event.id().as_ref() {
                "open" => show_main(app),
                "edit" => overlay_action(app, OverlayAction::EditAll),
                "lock" => overlay_action(app, OverlayAction::LockAll),
                "toggle" => overlay_action(app, OverlayAction::ToggleVisibleAll),
                "quit" => {
                    eprintln!(
                        "PULSE: tray Quit requested at {:?}",
                        std::time::SystemTime::now()
                    );
                    app.exit(0);
                }
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

/// Relabels the tray menu in the interface's language. Without a tray (it
/// could not be created) there is nothing to relabel, which is not an error.
pub fn set_tray_labels<R: Runtime>(app: &AppHandle<R>, labels: TrayLabels) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id("pulse") else {
        return Ok(());
    };
    let menu = tray_menu(app, &labels.sanitized())?;
    tray.set_menu(Some(menu))
}

// --- capabilities -------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopStatus {
    /// The overlay backend this session uses.
    pub backend: OverlayBackendInfo,
    /// The GNOME bridge, where it applies (GNOME on Wayland); otherwise its
    /// status says it does not apply.
    pub gnome_bridge: Option<GnomeBridgeStatus>,
    /// The repository copy of the extension and its installer, when PULSE
    /// runs from a source checkout.
    pub gnome_bridge_source: Option<String>,
    pub capabilities: OverlayCapabilities,
    /// The shortcut in force: as requested (plugin) or as the desktop
    /// describes it (portal).
    pub hotkey: Option<String>,
    pub hotkey_error: Option<String>,
    pub hotkey_backend: Option<ShortcutBackend>,
}

pub fn status<R: Runtime>(app: &AppHandle<R>) -> Option<DesktopStatus> {
    let state = app.try_state::<DesktopState>()?;
    let backend = lock(&state.hotkey_backend).clone();
    let gnome = gnome_status(&state);
    let gnome_active = gnome.as_ref().is_some_and(GnomeBridgeStatus::is_active);
    let gnome_hotkey = lock(&state.gnome_hotkey).clone();
    let runtime = RuntimeFacts {
        hotkey: lock(&state.hotkey_fact).clone(),
        tray: lock(&state.tray_fact).clone(),
        hotkey_backend: backend.clone(),
        gnome_desktop: lock(&state.gnome).as_ref().is_some_and(|f| f.gnome_desktop),
        gnome_bridge_active: gnome_active,
        gnome_bridge_hotkey: gnome_hotkey.clone(),
    };
    let overlay_backend = select_backend(state.display, gnome_active);
    let (hotkey, hotkey_error) = match (&backend, lock(&state.portal).as_ref()) {
        _ if overlay_backend == OverlayBackendKind::GnomeBridge => (gnome_hotkey, None),
        (Some(ShortcutBackend::Portal { .. }), Some(service)) => {
            let snapshot = service.snapshot();
            (
                snapshot.effective.or(snapshot.requested),
                snapshot.last_error,
            )
        }
        (Some(ShortcutBackend::Unavailable { .. }), _) => (None, None),
        _ => {
            let manager = lock(&state.hotkey);
            (
                manager.current().map(String::from),
                manager.last_error().map(String::from),
            )
        }
    };
    Some(DesktopStatus {
        backend: backend_info(overlay_backend),
        gnome_bridge: gnome,
        gnome_bridge_source: crate::gnome_bridge::source_dir(),
        capabilities: capabilities(state.display, &runtime),
        hotkey,
        hotkey_error,
        hotkey_backend: backend,
    })
}

/// Everything set up at launch: state, tray, then — off the main thread —
/// the saved shortcut and the saved overlays.
pub fn setup<R: Runtime>(app: &AppHandle<R>) {
    app.manage(DesktopState::new());
    build_tray(app);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // Overlays first: binding through the portal may wait for the user.
        reconcile(&handle);
        init_hotkey_backend(&handle);
        start_bridge(&handle);
        refresh_gnome_bridge(&handle);
        if let Some(bridge) = handle
            .try_state::<DesktopState>()
            .and_then(|state| gnome_status(&state))
            .filter(|status| {
                status.state != crate::overlay::gnome_bridge::BridgeState::NotApplicable
            })
        {
            eprintln!("PULSE: GNOME bridge: {}", bridge.summary);
        }
        let wanted = parse_settings(section(&handle, "settings").as_ref()).hotkey;
        // With the GNOME bridge active the shortcut is Mutter's, through the
        // extension: there is nothing for the portal or the plugin to bind.
        if !gnome_bridge_active(&handle) {
            if let Err(error) = apply_hotkey(&handle, wanted.as_deref(), true) {
                eprintln!("PULSE: global shortcut unavailable: {error}");
            }
        }
        if let Some(status) = status(&handle) {
            let caps = &status.capabilities;
            eprintln!(
                "PULSE: desktop {:?} — {}: always-on-top {:?}, click-through {:?}, positioning {:?}, hotkey {:?} ({}), tray {:?}",
                caps.display_server,
                status.backend.label,
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

#[cfg(test)]
mod tests {
    use super::TrayLabels;

    #[test]
    fn tray_labels_keep_a_translation_and_refuse_an_empty_or_odd_one() {
        let labels = TrayLabels {
            open: "  Ouvrir PULSE ".into(),
            edit: String::new(),
            lock: "x".repeat(65),
            toggle: "two\nlines".into(),
            quit: "Quitter PULSE".into(),
        }
        .sanitized();
        let defaults = TrayLabels::default();
        assert_eq!(labels.open, "Ouvrir PULSE");
        assert_eq!(labels.edit, defaults.edit);
        assert_eq!(labels.lock, defaults.lock);
        assert_eq!(labels.toggle, defaults.toggle);
        assert_eq!(labels.quit, "Quitter PULSE");
    }

    #[test]
    fn tray_labels_arrive_in_camel_case() {
        let labels: TrayLabels = serde_json::from_value(serde_json::json!({
            "open": "a", "edit": "b", "lock": "c", "toggle": "d", "quit": "e"
        }))
        .expect("deserialise");
        assert_eq!(labels.toggle, "d");
    }
}
