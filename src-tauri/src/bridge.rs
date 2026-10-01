//! The overlay bridge: a tiny user-session D-Bus service through which a
//! compositor-side companion (the GNOME Shell extension in
//! `integrations/gnome-shell/`) asks PULSE to toggle its overlays.
//!
//! Physically verified on Fedora 39 / GNOME 45 (Phase 11.5); see
//! `docs/overlay/gnome-bridge.md`.
//!
//! | what      | value                           |
//! | --------- | ------------------------------- |
//! | bus       | the user's session bus          |
//! | name      | `dev.pulse.app` (owned while PULSE runs) |
//! | object    | `/dev/pulse/app/OverlayBridge`  |
//! | interface | `dev.pulse.app.OverlayBridge`   |
//!
//! Four methods, nothing else: `Ping`, `GetOverlayBridgeVersion`,
//! `ToggleOverlayEditMode` (the shared Edit ↔ Locked toggle), and
//! `Hello(u version) → u`, with which the running extension announces itself.
//! The only argument anywhere is that version number, used for display; a
//! `Hello` is accepted only from the `gnome-shell` process (its PID comes from
//! the bus daemon). Nothing can make the bridge read a file, run a command or
//! reach any other PULSE function. PULSE stays the source of truth for the
//! overlays; the extension only sends the action.
//!
//! The name also lets the extension learn PULSE's PID from the bus daemon
//! (`GetConnectionUnixProcessID`), which is how it recognises PULSE's
//! overlay windows without trusting their titles alone.

pub const BUS_NAME: &str = "dev.pulse.app";
pub const OBJECT_PATH: &str = "/dev/pulse/app/OverlayBridge";
pub const INTERFACE: &str = "dev.pulse.app.OverlayBridge";
/// Bumped when the interface changes. 2 added `Hello`.
pub const VERSION: u32 = 2;

/// The highest companion version PULSE records; anything above is clamped.
#[cfg(target_os = "linux")]
const MAX_COMPANION_VERSION: u32 = 1_000;

/// The exported object. Its only effects are the callbacks it was built with.
pub struct OverlayBridge {
    // Read only by the D-Bus interface, which exists on Linux.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    toggle: Box<dyn Fn() + Send + Sync>,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    hello: Box<dyn Fn(u32) + Send + Sync>,
}

impl OverlayBridge {
    pub fn new(
        toggle: impl Fn() + Send + Sync + 'static,
        hello: impl Fn(u32) + Send + Sync + 'static,
    ) -> Self {
        Self {
            toggle: Box::new(toggle),
            hello: Box::new(hello),
        }
    }
}

/// Whether a process name (`/proc/<pid>/comm`) is GNOME Shell's.
pub fn is_gnome_shell_comm(comm: &str) -> bool {
    comm.trim() == "gnome-shell"
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

    /// Toggles the **visible** overlays: if any is in Edit mode, lock them;
    /// otherwise put them in Edit — the same action as the tray and the
    /// shortcut (`overlay::spec::toggle_all_locked`). Hidden overlays are
    /// never changed.
    fn toggle_overlay_edit_mode(&self) {
        (self.toggle)();
    }

    /// The running extension announces itself with its version and learns
    /// the bridge's. Refused unless the caller is the `gnome-shell` process.
    async fn hello(
        &self,
        companion_version: u32,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<u32> {
        let denied = || zbus::fdo::Error::AccessDenied("not GNOME Shell".into());
        let sender = header.sender().ok_or_else(denied)?.to_owned();
        let pid = zbus::fdo::DBusProxy::new(connection)
            .await?
            .get_connection_unix_process_id(sender.into())
            .await?;
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        if !is_gnome_shell_comm(&comm) {
            return Err(denied());
        }
        (self.hello)(companion_version.min(MAX_COMPANION_VERSION));
        Ok(VERSION)
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
pub fn start(
    toggle: impl Fn() + Send + Sync + 'static,
    hello: impl Fn(u32) + Send + Sync + 'static,
) -> Result<Option<BridgeService>, String> {
    #[cfg(target_os = "linux")]
    {
        let connection = zbus::blocking::connection::Builder::session()
            .and_then(|builder| builder.name(BUS_NAME))
            .and_then(|builder| builder.serve_at(OBJECT_PATH, OverlayBridge::new(toggle, hello)))
            .and_then(|builder| builder.build())
            .map_err(|error| error.to_string())?;
        Ok(Some(BridgeService {
            connection: Some(connection),
        }))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (toggle, hello);
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
        let bridge = OverlayBridge::new(
            move || {
                seen.fetch_add(1, Ordering::SeqCst);
            },
            |_| {},
        );
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
    fn the_interface_exposes_four_methods_and_one_numeric_argument() {
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
            [
                "Ping",
                "GetOverlayBridgeVersion",
                "ToggleOverlayEditMode",
                "Hello"
            ]
        );
        // One input argument in the whole interface: Hello's version number.
        assert_eq!(xml.matches("direction=\"in\"").count(), 1, "{xml}");
        assert!(
            xml.contains("<arg name=\"companion_version\" type=\"u\" direction=\"in\"/>"),
            "{xml}"
        );
        assert!(!xml.contains("<property"), "{xml}");
        assert!(!xml.contains("<signal"), "{xml}");
    }

    #[test]
    fn only_gnome_shell_may_say_hello() {
        assert!(is_gnome_shell_comm("gnome-shell\n"));
        assert!(!is_gnome_shell_comm("gnome-shell-calendar-server"));
        assert!(!is_gnome_shell_comm("python3"));
        assert!(!is_gnome_shell_comm(""));
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
