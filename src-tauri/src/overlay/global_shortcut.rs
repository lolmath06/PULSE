//! Which global-shortcut backend PULSE uses, and the XDG Desktop Portal
//! shortcut session — as decisions and a state machine, without D-Bus.
//!
//! Why a second backend: the Tauri global-shortcut plugin grabs keys through
//! X11. On a native Wayland session that grab lands on the XWayland server,
//! which only sees keys while an X11 window has the keyboard focus — so the
//! registration *succeeds* and the shortcut never fires while Firefox (or any
//! Wayland application) is focused. Wayland's answer is the desktop portal
//! interface `org.freedesktop.portal.GlobalShortcuts`, through which the
//! compositor itself delivers the shortcut, whichever application has focus.
//!
//! | session                   | backend                                        |
//! | ------------------------- | ---------------------------------------------- |
//! | Windows                   | plugin (RegisterHotKey)                        |
//! | X11, XWayland             | plugin (XGrabKey) — unchanged                  |
//! | Wayland + portal present  | XDG Desktop Portal GlobalShortcuts             |
//! | Wayland, no portal        | none — *unsupported*, with the reason          |
//!
//! On Wayland PULSE never falls back to the X11 grab: it would register, look
//! fine, and never trigger.
//!
//! The D-Bus client that implements [`PortalBus`] lives in `crate::portal`;
//! everything here is plain data so it is tested with a fake bus and type
//! checked by the Windows harness.

use serde::Serialize;

use super::capabilities::DisplayServer;

/// The portal action id. Stable: the desktop remembers the user's approval
/// and chosen keys under it.
pub const ACTION_ID: &str = "toggle-overlays-edit";

/// What the desktop shows the user for the action.
pub const ACTION_DESCRIPTION: &str = "PULSE: toggle overlays between Edit and Locked";

/// Whether the session's desktop portal offers `GlobalShortcuts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortalAvailability {
    Available { version: u32 },
    Unavailable { reason: String },
}

/// The backend in force for this session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ShortcutBackend {
    /// The Tauri global-shortcut plugin (Win32 `RegisterHotKey`, X11 `XGrabKey`).
    Plugin,
    /// `org.freedesktop.portal.GlobalShortcuts`.
    Portal { version: u32 },
    /// No backend can deliver a global shortcut on this session.
    Unavailable { reason: String },
}

/// Picks the backend. `probe_portal` is only called on native Wayland, so
/// neither Windows nor X11 ever touches D-Bus for this.
pub fn choose_backend(
    server: DisplayServer,
    probe_portal: impl FnOnce() -> PortalAvailability,
) -> ShortcutBackend {
    match server {
        DisplayServer::Windows | DisplayServer::X11 | DisplayServer::XWayland => {
            ShortcutBackend::Plugin
        }
        DisplayServer::Wayland => match probe_portal() {
            PortalAvailability::Available { version } => ShortcutBackend::Portal { version },
            PortalAvailability::Unavailable { reason } => ShortcutBackend::Unavailable { reason },
        },
        DisplayServer::Other => ShortcutBackend::Unavailable {
            reason: "global shortcuts are not supported on this platform".into(),
        },
    }
}

/// Converts PULSE's shortcut syntax (`Ctrl+Shift+F12`, the plugin's) to the
/// XDG shortcut trigger syntax (`CTRL+SHIFT+F12`): modifiers `CTRL`, `ALT`,
/// `SHIFT`, `LOGO`, then one XKB key name.
pub fn to_portal_trigger(shortcut: &str) -> Result<String, String> {
    let invalid = || format!("not a valid shortcut ({shortcut})");
    let parts: Vec<&str> = shortcut.split('+').map(str::trim).collect();
    let (key, modifiers) = parts.split_last().ok_or_else(invalid)?;
    let mut out: Vec<&str> = Vec::new();
    for modifier in modifiers {
        let name = match modifier.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "commandorcontrol" | "cmdorctrl" | "cmdorcontrol" => "CTRL",
            "alt" | "option" => "ALT",
            "shift" => "SHIFT",
            "super" | "meta" | "logo" | "cmd" | "command" | "win" => "LOGO",
            _ => return Err(invalid()),
        };
        if !out.contains(&name) {
            out.push(name);
        }
    }
    if out.is_empty() {
        return Err(format!(
            "{} — a global shortcut needs a modifier",
            invalid()
        ));
    }
    let key = portal_key(key).ok_or_else(invalid)?;
    let mut trigger = out.join("+");
    trigger.push('+');
    trigger.push_str(&key);
    Ok(trigger)
}

fn portal_key(key: &str) -> Option<String> {
    let lower = key.to_ascii_lowercase();
    let stripped = lower
        .strip_prefix("key")
        .filter(|rest| rest.len() == 1)
        .or_else(|| lower.strip_prefix("digit").filter(|rest| rest.len() == 1))
        .unwrap_or(&lower);
    if stripped.len() == 1 && stripped.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Some(stripped.to_string());
    }
    if let Some(number) = stripped.strip_prefix('f') {
        if let Ok(n) = number.parse::<u8>() {
            if (1..=24).contains(&n) {
                return Some(format!("F{n}"));
            }
        }
    }
    let named = match stripped {
        "space" => "space",
        "enter" | "return" => "Return",
        "tab" => "Tab",
        "escape" | "esc" => "Escape",
        "backspace" => "BackSpace",
        "delete" => "Delete",
        "insert" => "Insert",
        "home" => "Home",
        "end" => "End",
        "pageup" => "Page_Up",
        "pagedown" => "Page_Down",
        "up" | "arrowup" => "Up",
        "down" | "arrowdown" => "Down",
        "left" | "arrowleft" => "Left",
        "right" | "arrowright" => "Right",
        _ => return None,
    };
    Some(named.to_string())
}

/// Why a portal call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortalError {
    /// No session bus, no portal, or the interface is missing.
    Unavailable(String),
    /// The portal answered with an error, or did not answer in time.
    Failed(String),
}

impl std::fmt::Display for PortalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortalError::Unavailable(reason) => write!(f, "desktop portal unavailable: {reason}"),
            PortalError::Failed(reason) => write!(f, "desktop portal error: {reason}"),
        }
    }
}

/// The answer to `BindShortcuts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindOutcome {
    /// Bound. `trigger` is the desktop's own description of the keys in
    /// force — which may differ from the requested ones (the user chose).
    Bound { trigger: Option<String> },
    /// The user cancelled the desktop's dialog, or the desktop refused.
    Rejected,
}

/// The three portal calls the session needs. Implemented over D-Bus in
/// `crate::portal`, and by a fake in tests.
pub trait PortalBus {
    /// `CreateSession`; returns the session handle.
    fn create_session(&mut self) -> Result<String, PortalError>;
    /// `BindShortcuts` for [`ACTION_ID`] with the preferred trigger.
    fn bind(&mut self, session: &str, preferred_trigger: &str) -> Result<BindOutcome, PortalError>;
    /// `org.freedesktop.portal.Session.Close`.
    fn close_session(&mut self, session: &str);
}

/// The portal shortcut session: at most one session is kept open, and a new
/// binding replaces the old one only once the desktop has accepted it.
#[derive(Debug, Default)]
pub struct PortalShortcut {
    session: Option<String>,
    requested: Option<String>,
    effective: Option<String>,
    last_error: Option<String>,
    /// The shortcut whose last attempt failed: not retried until the user
    /// asks again, so a declined dialog never reappears on its own.
    failed: Option<String>,
}

impl PortalShortcut {
    /// The open session, if a shortcut is bound.
    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    /// The shortcut PULSE asked for (PULSE syntax).
    pub fn requested(&self) -> Option<&str> {
        self.requested.as_deref()
    }

    /// The keys the desktop says are in force, if it said.
    pub fn effective(&self) -> Option<&str> {
        self.effective.as_deref()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Makes `wanted` the bound shortcut (`None` unbinds it).
    ///
    /// A change opens a **new** session and binds it; only when the desktop
    /// accepts is the previous session closed. A rejection or an error closes
    /// the new session and leaves the previous binding working. Either way at
    /// most one session remains open.
    ///
    /// `explicit` is true when the user asked (changing the shortcut, or
    /// launching PULSE). Otherwise — any other settings change — a shortcut
    /// that already failed is not tried again, so the desktop's dialog never
    /// reappears unasked.
    pub fn apply(
        &mut self,
        bus: &mut dyn PortalBus,
        wanted: Option<&str>,
        explicit: bool,
    ) -> Result<(), String> {
        if wanted == self.requested.as_deref() && (wanted.is_none() || self.session.is_some()) {
            return Ok(());
        }
        let Some(wanted) = wanted else {
            self.close(bus);
            self.last_error = None;
            self.failed = None;
            return Ok(());
        };
        if !explicit && self.failed.as_deref() == Some(wanted) {
            return Err(self
                .last_error
                .clone()
                .unwrap_or_else(|| format!("{wanted}: not bound")));
        }
        let fail = |this: &mut Self, message: String| {
            this.last_error = Some(message.clone());
            this.failed = Some(wanted.to_string());
            Err(message)
        };
        let trigger = match to_portal_trigger(wanted) {
            Ok(trigger) => trigger,
            Err(error) => return fail(self, error),
        };
        let session = match bus.create_session() {
            Ok(session) => session,
            Err(error) => return fail(self, error.to_string()),
        };
        match bus.bind(&session, &trigger) {
            Ok(BindOutcome::Bound { trigger }) => {
                if let Some(old) = self.session.replace(session) {
                    bus.close_session(&old);
                }
                self.requested = Some(wanted.to_string());
                self.effective = trigger.filter(|t| !t.trim().is_empty());
                self.last_error = None;
                self.failed = None;
                Ok(())
            }
            Ok(BindOutcome::Rejected) => {
                bus.close_session(&session);
                fail(
                    self,
                    format!("{wanted}: not bound — the desktop's shortcut request was declined"),
                )
            }
            Err(error) => {
                bus.close_session(&session);
                fail(self, format!("{wanted}: {error}"))
            }
        }
    }

    /// Whether an `Activated`/`Deactivated` signal is for our binding.
    pub fn is_ours(&self, session: &str, action_id: &str) -> bool {
        action_id == ACTION_ID && self.session.as_deref() == Some(session)
    }

    /// `ShortcutsChanged`: the user changed the keys in the desktop settings.
    pub fn shortcuts_changed(&mut self, session: &str, trigger: Option<String>) {
        if self.session.as_deref() == Some(session) {
            self.effective = trigger.filter(|t| !t.trim().is_empty());
        }
    }

    /// Closes the session (disable, or quitting PULSE).
    pub fn close(&mut self, bus: &mut dyn PortalBus) {
        if let Some(session) = self.session.take() {
            bus.close_session(&session);
        }
        self.requested = None;
        self.effective = None;
    }
}

/// What a portal signal means for PULSE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalAction {
    /// Our shortcut was pressed: toggle every overlay Edit ↔ Locked.
    Toggle,
    /// Released, or not ours: nothing to do.
    Ignore,
}

/// `Activated` toggles (once per press); `Deactivated` — the release — does
/// nothing, so a press never toggles twice.
///
/// `active_session` is [`PortalShortcut::session`] as last published by the
/// worker that owns the state machine.
pub fn signal_action(
    active_session: Option<&str>,
    member: &str,
    session: &str,
    action_id: &str,
) -> SignalAction {
    if member == "Activated" && action_id == ACTION_ID && active_session == Some(session) {
        SignalAction::Toggle
    } else {
        SignalAction::Ignore
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scripted portal that records every call.
    #[derive(Default)]
    struct FakePortal {
        calls: Vec<String>,
        open: Vec<String>,
        next: u32,
        unavailable: bool,
        reject: bool,
        bind_error: bool,
        trigger: Option<String>,
    }

    impl PortalBus for FakePortal {
        fn create_session(&mut self) -> Result<String, PortalError> {
            if self.unavailable {
                self.calls.push("create:unavailable".into());
                return Err(PortalError::Unavailable(
                    "org.freedesktop.portal.Desktop is not on the session bus".into(),
                ));
            }
            self.next += 1;
            let session = format!("/session/{}", self.next);
            self.calls.push(format!("create:{session}"));
            self.open.push(session.clone());
            Ok(session)
        }
        fn bind(&mut self, session: &str, trigger: &str) -> Result<BindOutcome, PortalError> {
            self.calls.push(format!("bind:{session}:{trigger}"));
            if self.bind_error {
                return Err(PortalError::Failed("timed out".into()));
            }
            if self.reject {
                return Ok(BindOutcome::Rejected);
            }
            Ok(BindOutcome::Bound {
                trigger: self.trigger.clone(),
            })
        }
        fn close_session(&mut self, session: &str) {
            self.calls.push(format!("close:{session}"));
            self.open.retain(|open| open != session);
        }
    }

    fn portal() -> PortalAvailability {
        PortalAvailability::Available { version: 1 }
    }

    #[test]
    fn wayland_with_the_portal_uses_the_portal() {
        assert_eq!(
            choose_backend(DisplayServer::Wayland, portal),
            ShortcutBackend::Portal { version: 1 }
        );
    }

    #[test]
    fn wayland_without_the_portal_is_unsupported_with_the_reason() {
        let backend = choose_backend(DisplayServer::Wayland, || PortalAvailability::Unavailable {
            reason: "no org.freedesktop.portal.GlobalShortcuts".into(),
        });
        assert_eq!(
            backend,
            ShortcutBackend::Unavailable {
                reason: "no org.freedesktop.portal.GlobalShortcuts".into()
            },
            "never the X11 grab, which would register and never fire"
        );
    }

    #[test]
    fn x11_xwayland_and_windows_keep_the_plugin_and_never_probe() {
        for server in [
            DisplayServer::X11,
            DisplayServer::XWayland,
            DisplayServer::Windows,
        ] {
            let backend = choose_backend(server, || panic!("no D-Bus probe on {server:?}"));
            assert_eq!(backend, ShortcutBackend::Plugin);
        }
        assert!(matches!(
            choose_backend(DisplayServer::Other, portal),
            ShortcutBackend::Unavailable { .. }
        ));
    }

    #[test]
    fn shortcuts_convert_to_the_xdg_trigger_syntax() {
        assert_eq!(
            to_portal_trigger("Ctrl+Shift+F12").unwrap(),
            "CTRL+SHIFT+F12"
        );
        assert_eq!(to_portal_trigger("alt + shift + o").unwrap(), "ALT+SHIFT+o");
        assert_eq!(to_portal_trigger("Super+KeyP").unwrap(), "LOGO+p");
        assert_eq!(to_portal_trigger("Ctrl+Digit1").unwrap(), "CTRL+1");
        assert_eq!(
            to_portal_trigger("Ctrl+Alt+Space").unwrap(),
            "CTRL+ALT+space"
        );
        assert_eq!(to_portal_trigger("Ctrl+PageUp").unwrap(), "CTRL+Page_Up");
        assert!(to_portal_trigger("F12").is_err(), "a modifier is required");
        assert!(to_portal_trigger("Ctrl+Banana").is_err());
        assert!(to_portal_trigger("Hyper+F1").is_err());
        assert!(to_portal_trigger("Ctrl+F25").is_err());
        assert!(to_portal_trigger("").is_err());
    }

    #[test]
    fn create_then_bind() {
        let mut bus = FakePortal {
            trigger: Some("Ctrl+Shift+F12".into()),
            ..FakePortal::default()
        };
        let mut shortcut = PortalShortcut::default();
        shortcut
            .apply(&mut bus, Some("Ctrl+Shift+F12"), true)
            .expect("bound");
        assert_eq!(
            bus.calls,
            ["create:/session/1", "bind:/session/1:CTRL+SHIFT+F12"]
        );
        assert_eq!(shortcut.session(), Some("/session/1"));
        assert_eq!(shortcut.requested(), Some("Ctrl+Shift+F12"));
        assert_eq!(shortcut.effective(), Some("Ctrl+Shift+F12"));
        // Applying the same shortcut again opens nothing new.
        shortcut
            .apply(&mut bus, Some("Ctrl+Shift+F12"), true)
            .expect("idempotent");
        assert_eq!(bus.calls.len(), 2, "no repeated session creation");
        assert_eq!(bus.open, ["/session/1"]);
    }

    #[test]
    fn activated_toggles_once_and_deactivated_does_nothing() {
        let mut bus = FakePortal::default();
        let mut shortcut = PortalShortcut::default();
        shortcut
            .apply(&mut bus, Some("Ctrl+Shift+F12"), true)
            .unwrap();
        assert_eq!(
            signal_action(shortcut.session(), "Activated", "/session/1", ACTION_ID),
            SignalAction::Toggle
        );
        assert_eq!(
            signal_action(shortcut.session(), "Deactivated", "/session/1", ACTION_ID),
            SignalAction::Ignore
        );
        assert_eq!(
            signal_action(shortcut.session(), "Activated", "/session/9", ACTION_ID),
            SignalAction::Ignore,
            "another session (another application)"
        );
        assert_eq!(
            signal_action(
                shortcut.session(),
                "Activated",
                "/session/1",
                "other-action"
            ),
            SignalAction::Ignore
        );
    }

    #[test]
    fn changing_the_binding_replaces_the_session_after_acceptance() {
        let mut bus = FakePortal::default();
        let mut shortcut = PortalShortcut::default();
        shortcut
            .apply(&mut bus, Some("Ctrl+Shift+F12"), true)
            .unwrap();
        shortcut.apply(&mut bus, Some("Alt+Shift+O"), true).unwrap();
        assert_eq!(
            bus.calls,
            [
                "create:/session/1",
                "bind:/session/1:CTRL+SHIFT+F12",
                "create:/session/2",
                "bind:/session/2:ALT+SHIFT+o",
                "close:/session/1",
            ]
        );
        assert_eq!(bus.open, ["/session/2"], "exactly one session remains");
        assert!(!shortcut.is_ours("/session/1", ACTION_ID));
        assert!(shortcut.is_ours("/session/2", ACTION_ID));
        assert_eq!(
            shortcut.effective(),
            None,
            "the desktop did not describe it"
        );
    }

    #[test]
    fn disable_closes_the_session() {
        let mut bus = FakePortal::default();
        let mut shortcut = PortalShortcut::default();
        shortcut
            .apply(&mut bus, Some("Ctrl+Shift+F12"), true)
            .unwrap();
        shortcut.apply(&mut bus, None, true).expect("disabled");
        assert!(bus.open.is_empty());
        assert_eq!(shortcut.session(), None);
        assert_eq!(shortcut.requested(), None);
        assert!(!shortcut.is_ours("/session/1", ACTION_ID));
        shortcut.apply(&mut bus, None, true).expect("idempotent");
        assert_eq!(
            bus.calls.last().map(String::as_str),
            Some("close:/session/1")
        );
    }

    #[test]
    fn cleanup_on_quit_closes_the_session() {
        let mut bus = FakePortal::default();
        let mut shortcut = PortalShortcut::default();
        shortcut
            .apply(&mut bus, Some("Ctrl+Shift+F12"), true)
            .unwrap();
        shortcut.close(&mut bus);
        assert!(bus.open.is_empty());
        shortcut.close(&mut bus);
        assert_eq!(
            bus.calls.iter().filter(|c| c.starts_with("close")).count(),
            1,
            "closed once"
        );
    }

    #[test]
    fn a_rejected_request_is_reported_and_keeps_the_previous_binding() {
        let mut bus = FakePortal::default();
        let mut shortcut = PortalShortcut::default();
        shortcut
            .apply(&mut bus, Some("Ctrl+Shift+F12"), true)
            .unwrap();
        bus.reject = true;
        let error = shortcut
            .apply(&mut bus, Some("Alt+Shift+O"), true)
            .expect_err("rejected");
        assert!(error.contains("declined"), "{error}");
        assert_eq!(bus.open, ["/session/1"], "the rejected session is closed");
        assert_eq!(shortcut.requested(), Some("Ctrl+Shift+F12"));
        assert!(shortcut.is_ours("/session/1", ACTION_ID));
        assert!(shortcut.last_error().is_some());

        // First binding rejected: nothing is bound, nothing is left open.
        let mut fresh = PortalShortcut::default();
        let mut refusing = FakePortal {
            reject: true,
            ..FakePortal::default()
        };
        assert!(fresh
            .apply(&mut refusing, Some("Ctrl+Shift+F12"), true)
            .is_err());
        assert_eq!(fresh.session(), None);
        assert!(refusing.open.is_empty());
    }

    #[test]
    fn an_unavailable_portal_or_failing_bind_is_an_error_not_a_binding() {
        let mut shortcut = PortalShortcut::default();
        let mut gone = FakePortal {
            unavailable: true,
            ..FakePortal::default()
        };
        let error = shortcut
            .apply(&mut gone, Some("Ctrl+Shift+F12"), true)
            .expect_err("no portal");
        assert!(error.contains("unavailable"), "{error}");
        assert_eq!(shortcut.session(), None);

        let mut failing = FakePortal {
            bind_error: true,
            ..FakePortal::default()
        };
        assert!(shortcut
            .apply(&mut failing, Some("Ctrl+Shift+F12"), true)
            .is_err());
        assert!(failing.open.is_empty(), "the unbound session is closed");
        // An invalid shortcut never reaches the bus.
        let mut untouched = FakePortal::default();
        assert!(shortcut
            .apply(&mut untouched, Some("banana"), true)
            .is_err());
        assert!(untouched.calls.is_empty());
    }

    #[test]
    fn a_declined_shortcut_is_only_retried_when_the_user_asks() {
        let mut bus = FakePortal {
            reject: true,
            ..FakePortal::default()
        };
        let mut shortcut = PortalShortcut::default();
        assert!(shortcut
            .apply(&mut bus, Some("Ctrl+Shift+F12"), true)
            .is_err());
        let calls = bus.calls.len();
        // Another setting changed: the same shortcut is re-applied implicitly.
        let error = shortcut
            .apply(&mut bus, Some("Ctrl+Shift+F12"), false)
            .expect_err("still not bound");
        assert!(error.contains("declined"));
        assert_eq!(bus.calls.len(), calls, "no new session, no new dialog");
        // The user asks again: the desktop is asked again.
        bus.reject = false;
        shortcut
            .apply(&mut bus, Some("Ctrl+Shift+F12"), true)
            .expect("approved this time");
        assert_eq!(bus.open.len(), 1);
    }

    #[test]
    fn the_desktop_may_change_the_keys() {
        let mut bus = FakePortal::default();
        let mut shortcut = PortalShortcut::default();
        shortcut
            .apply(&mut bus, Some("Ctrl+Shift+F12"), true)
            .unwrap();
        shortcut.shortcuts_changed("/session/1", Some("Super+O".into()));
        assert_eq!(shortcut.effective(), Some("Super+O"));
        shortcut.shortcuts_changed("/session/7", Some("ignored".into()));
        assert_eq!(shortcut.effective(), Some("Super+O"));
    }
}
