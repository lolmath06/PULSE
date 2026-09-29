//! The overlay bridge: a tiny user-session D-Bus service through which a
//! compositor-side companion (the GNOME Shell extension in
//! `integrations/gnome-shell/`) asks PULSE to toggle its overlays.
//!
//! **Prototype (Phase 11.5A), physically unproven.** See
//! `docs/overlay/gnome-bridge-poc.md`.
//!
//! | what      | value                           |
//! | --------- | ------------------------------- |
//! | bus       | the user's session bus          |
//! | name      | `dev.pulse.app` (owned while PULSE runs) |
//! | object    | `/dev/pulse/app/OverlayBridge`  |
//! | interface | `dev.pulse.app.OverlayBridge`   |
//!
//! Three methods, nothing else: `Ping`, `GetOverlayBridgeVersion`, and
//! `ToggleOverlayEditMode`, which runs the existing Edit ↔ Locked toggle.
//! No argument is ever taken, so the bridge cannot be made to read a file,
//! run a command or reach any other PULSE function. PULSE stays the source of
//! truth for the overlays; the extension only sends the action.
//!
//! The name also lets the extension learn PULSE's PID from the bus daemon
//! (`GetConnectionUnixProcessID`), which is how it recognises PULSE's
//! overlay windows without trusting their titles alone.

pub const BUS_NAME: &str = "dev.pulse.app";
pub const OBJECT_PATH: &str = "/dev/pulse/app/OverlayBridge";
pub const INTERFACE: &str = "dev.pulse.app.OverlayBridge";
/// Bumped when the interface changes.
pub const VERSION: u32 = 1;

/// The exported object. Its only effect is the callback it was built with.
pub struct OverlayBridge {
    // Read only by the D-Bus interface, which exists on Linux.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    toggle: Box<dyn Fn() + Send + Sync>,
}

impl OverlayBridge {
    pub fn new(toggle: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            toggle: Box::new(toggle),
        }
    }
}

#[cfg(target_os = "linux")]
#[zbus::interface(name = "dev.pulse.app.OverlayBridge")]
impl OverlayBridge {
    /// Liveness check for the extension and for manual testing.
    fn ping(&self) -> &str {
        "pong"
    }

    fn get_overlay_bridge_version(&self) -> u32 {
        VERSION
    }

    /// If any overlay is in Edit mode, lock them all; otherwise edit them all
    /// — the same action as the tray and the Phase 11 shortcut.
    fn toggle_overlay_edit_mode(&self) {
        (self.toggle)();
    }
}

/// The running service. Dropping it (or [`BridgeService::stop`]) releases the
/// name, so the bridge exists exactly while PULSE runs.
pub struct BridgeService {
    #[cfg(target_os = "linux")]
    connection: Option<zbus::blocking::Connection>,
}

impl BridgeService {
    /// Releases the bus name and closes the connection. Idempotent.
    pub fn stop(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(connection) = self.connection.take() {
            let _ = connection.release_name(BUS_NAME);
        }
    }

    pub fn is_running(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.connection.is_some()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }
}

impl Drop for BridgeService {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Starts the service on the session bus. `Ok(None)` where there is no such
/// bridge (non-Linux). An error — no session bus, or another PULSE already
/// owns the name — is reported once by the caller and is never fatal.
pub fn start(toggle: impl Fn() + Send + Sync + 'static) -> Result<Option<BridgeService>, String> {
    #[cfg(target_os = "linux")]
    {
        let connection = zbus::blocking::connection::Builder::session()
            .and_then(|builder| builder.name(BUS_NAME))
            .and_then(|builder| builder.serve_at(OBJECT_PATH, OverlayBridge::new(toggle)))
            .and_then(|builder| builder.build())
            .map_err(|error| error.to_string())?;
        Ok(Some(BridgeService {
            connection: Some(connection),
        }))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = toggle;
        Ok(None)
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use zbus::object_server::Interface;

    use super::*;

    fn counted() -> (OverlayBridge, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        let bridge = OverlayBridge::new(move || {
            seen.fetch_add(1, Ordering::SeqCst);
        });
        (bridge, count)
    }

    #[test]
    fn the_toggle_method_runs_the_existing_toggle_action_once() {
        let (bridge, count) = counted();
        bridge.toggle_overlay_edit_mode();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        bridge.toggle_overlay_edit_mode();
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert_eq!(bridge.ping(), "pong");
        assert_eq!(bridge.get_overlay_bridge_version(), VERSION);
        assert_eq!(
            count.load(Ordering::SeqCst),
            2,
            "read-only methods do nothing"
        );
    }

    #[test]
    fn the_interface_exposes_exactly_three_argument_free_methods() {
        let (bridge, _) = counted();
        let mut xml = String::new();
        bridge.introspect_to_writer(&mut xml, 0);
        assert!(
            xml.contains(&format!("<interface name=\"{INTERFACE}\">")),
            "{xml}"
        );
        let methods: Vec<&str> = xml
            .split("<method name=\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .collect();
        assert_eq!(
            methods,
            ["Ping", "GetOverlayBridgeVersion", "ToggleOverlayEditMode"]
        );
        // No input argument anywhere: nothing can be passed in.
        assert!(!xml.contains("direction=\"in\""), "{xml}");
        assert!(!xml.contains("<property"), "{xml}");
        assert!(!xml.contains("<signal"), "{xml}");
    }

    #[test]
    fn stopping_is_idempotent_and_releases_everything() {
        let mut service = BridgeService { connection: None };
        assert!(!service.is_running());
        service.stop();
        service.stop();
        assert!(!service.is_running());
    }
}
