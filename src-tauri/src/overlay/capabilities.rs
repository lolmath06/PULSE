//! What a desktop overlay can honestly do on this session.
//!
//! An overlay is an ordinary top-level window that asks the platform for
//! favours — stay above others, let clicks through, sit at a position. Whether
//! it gets them depends on the platform and, on Linux, on the display server
//! the window actually runs on. PULSE reports each capability as
//! **supported**, **limited** or **unsupported**, with the reason, and the UI
//! shows it; nothing is claimed that the platform does not do.
//!
//! These are *expectations from the platform*, refined by runtime facts (did
//! the hotkey register? was the tray created?). Physical verification is
//! documented per platform in `docs/overlay/platform-capabilities.md`.

use serde::{Deserialize, Serialize};

use super::global_shortcut::ShortcutBackend;

/// Which windowing system an overlay window really lives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayServer {
    Windows,
    /// Native X11 session.
    X11,
    /// Native Wayland: GTK talks to the compositor directly.
    Wayland,
    /// An X11 client inside a Wayland session (`GDK_BACKEND=x11`).
    XWayland,
    /// Anything else (macOS, BSD): no overlay support is claimed.
    Other,
}

/// The environment facts detection needs. Plain values, so tests cover every
/// combination without touching the real environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionFacts {
    pub os: String,
    pub xdg_session_type: Option<String>,
    pub wayland_display: Option<String>,
    pub display: Option<String>,
    pub gdk_backend: Option<String>,
}

impl SessionFacts {
    /// Reads the real process environment.
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        Self {
            os: std::env::consts::OS.to_string(),
            xdg_session_type: var("XDG_SESSION_TYPE"),
            wayland_display: var("WAYLAND_DISPLAY"),
            display: var("DISPLAY"),
            gdk_backend: var("GDK_BACKEND"),
        }
    }
}

/// Which backend GTK picks, by the same rule GTK uses: the first entry of
/// `GDK_BACKEND` if set, else Wayland when a Wayland display exists, else X11.
pub fn detect_display_server(facts: &SessionFacts) -> DisplayServer {
    match facts.os.as_str() {
        "windows" => return DisplayServer::Windows,
        "linux" => {}
        _ => return DisplayServer::Other,
    }
    let wayland_session =
        facts.wayland_display.is_some() || facts.xdg_session_type.as_deref() == Some("wayland");
    let requested = facts
        .gdk_backend
        .as_deref()
        .and_then(|backends| backends.split(',').next())
        .map(str::trim);
    let gtk_uses_wayland = match requested {
        Some("wayland") => facts.wayland_display.is_some(),
        Some("x11") => false,
        _ => facts.wayland_display.is_some(),
    };
    match (gtk_uses_wayland, wayland_session) {
        (true, _) => DisplayServer::Wayland,
        (false, true) => DisplayServer::XWayland,
        (false, false) if facts.display.is_some() => DisplayServer::X11,
        _ => DisplayServer::Other,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityStatus {
    Supported,
    Limited,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub status: CapabilityStatus,
    pub reason: String,
}

fn cap(status: CapabilityStatus, reason: &str) -> Capability {
    Capability {
        status,
        reason: reason.to_string(),
    }
}

/// Every overlay capability, for one session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayCapabilities {
    pub display_server: DisplayServer,
    pub always_on_top: Capability,
    pub click_through: Capability,
    pub positioning: Capability,
    pub transparent_window: Capability,
    pub global_hotkey: Capability,
    pub multi_monitor_positioning: Capability,
    pub tray: Capability,
}

/// What happened at runtime, which overrides the platform expectation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeFacts {
    /// `Some(Err)` when registering the global shortcut failed.
    pub hotkey: Option<Result<String, String>>,
    /// `Some(Err)` when the tray icon could not be created.
    pub tray: Option<Result<(), String>>,
    /// The global-shortcut backend chosen for this session, once known.
    pub hotkey_backend: Option<ShortcutBackend>,
    /// `XDG_CURRENT_DESKTOP` names GNOME.
    pub gnome_desktop: bool,
    /// GNOME Shell reports the PULSE overlay bridge enabled.
    pub gnome_bridge_active: bool,
    /// The bridge's shortcut, as PULSE writes shortcuts (`Ctrl+Shift+F12`).
    pub gnome_bridge_hotkey: Option<String>,
}

use CapabilityStatus::{Limited, Supported, Unsupported};

/// The platform's expected capabilities, before runtime facts.
pub fn expected_capabilities(server: DisplayServer) -> OverlayCapabilities {
    let windows_note =
        "PULSE's native Win32 overlay backend — implemented and compiled, not yet verified on a Windows machine";
    match server {
        DisplayServer::Windows => OverlayCapabilities {
            display_server: server,
            always_on_top: cap(
                Supported,
                &format!("HWND_TOPMOST, re-asserted on every Edit/Lock change — {windows_note}"),
            ),
            click_through: cap(
                Supported,
                &format!(
                    "a layered, hit-test-transparent window (WS_EX_LAYERED | WS_EX_TRANSPARENT) while locked; WS_EX_NOACTIVATE never takes focus — {windows_note}"
                ),
            ),
            positioning: cap(Supported, &format!("absolute placement per monitor — {windows_note}")),
            transparent_window: cap(Supported, &format!("per-pixel alpha — {windows_note}")),
            global_hotkey: cap(Supported, &format!("RegisterHotKey — {windows_note}")),
            multi_monitor_positioning: cap(
                Supported,
                &format!("monitor-relative placement with per-monitor DPI — {windows_note}"),
            ),
            tray: cap(Supported, &format!("notification-area icon — {windows_note}")),
        },
        DisplayServer::X11 | DisplayServer::XWayland => {
            let xwayland = server == DisplayServer::XWayland;
            OverlayCapabilities {
                display_server: server,
                always_on_top: cap(
                    Supported,
                    "_NET_WM_STATE_ABOVE, honoured by GNOME, KDE and most window managers",
                ),
                click_through: cap(Supported, "an empty input shape (X Shape extension)"),
                positioning: cap(Supported, "X11 windows are placed where they ask"),
                transparent_window: if xwayland {
                    cap(Supported, "the Wayland compositor composites XWayland windows")
                } else {
                    cap(Limited, "needs a compositing window manager; without one the background is opaque")
                },
                global_hotkey: if xwayland {
                    cap(
                        Limited,
                        "grabbed on the XWayland server: it fires only while an X11/XWayland window has the keyboard focus",
                    )
                } else {
                    cap(Supported, "XGrabKey on the root window")
                },
                multi_monitor_positioning: cap(Supported, "monitor-relative placement"),
                tray: cap(
                    Limited,
                    "StatusNotifier/AppIndicator: shown by KDE; GNOME needs the AppIndicator extension",
                ),
            }
        }
        DisplayServer::Wayland => OverlayCapabilities {
            display_server: server,
            always_on_top: cap(
                Limited,
                "Wayland has no protocol for an application to keep itself above others, and the compositor decides: measured on GNOME, the overlay goes behind an application once that application is focused. GNOME's own window menu (Alt+Space → “Always on Top”) is the user's choice to make, not PULSE's",
            ),
            click_through: cap(
                Limited,
                "an empty input region (wl_surface.set_input_region) is requested; the compositor decides whether clicks pass through",
            ),
            positioning: cap(
                Unsupported,
                "Wayland clients cannot choose or read their position; drag the overlay in Edit mode and the compositor places it",
            ),
            transparent_window: cap(Supported, "per-pixel alpha surfaces"),
            global_hotkey: cap(
                Unsupported,
                "on Wayland a global shortcut needs the XDG Desktop Portal (org.freedesktop.portal.GlobalShortcuts); an X11 grab would register and never fire while a Wayland application has focus",
            ),
            multi_monitor_positioning: cap(
                Unsupported,
                "the compositor chooses the monitor; PULSE cannot place a window on one",
            ),
            tray: cap(
                Limited,
                "StatusNotifier/AppIndicator: shown by KDE; GNOME needs the AppIndicator extension",
            ),
        },
        DisplayServer::Other => {
            let none = || cap(Unsupported, "desktop overlays are not supported on this platform");
            OverlayCapabilities {
                display_server: server,
                always_on_top: none(),
                click_through: none(),
                positioning: none(),
                transparent_window: none(),
                global_hotkey: none(),
                multi_monitor_positioning: none(),
                tray: none(),
            }
        }
    }
}

/// Expectations refined by what actually happened.
pub fn capabilities(server: DisplayServer, runtime: &RuntimeFacts) -> OverlayCapabilities {
    let mut capabilities = expected_capabilities(server);
    let portal = matches!(runtime.hotkey_backend, Some(ShortcutBackend::Portal { .. }));
    match &runtime.hotkey_backend {
        Some(ShortcutBackend::Portal { version }) => {
            capabilities.global_hotkey = cap(
                Supported,
                &format!(
                    "via the XDG Desktop Portal (GlobalShortcuts v{version}): the desktop delivers the shortcut whichever application has focus, and may ask you to approve it"
                ),
            );
        }
        Some(ShortcutBackend::Unavailable { reason }) => {
            capabilities.global_hotkey = cap(Unsupported, reason);
        }
        Some(ShortcutBackend::Plugin) | None => {}
    }
    if capabilities.global_hotkey.status != Unsupported || portal {
        match &runtime.hotkey {
            Some(Err(error)) => {
                let what = if portal {
                    "the shortcut is not bound"
                } else {
                    "the shortcut could not be registered"
                };
                capabilities.global_hotkey = cap(Unsupported, &format!("{what}: {error}"));
            }
            Some(Ok(shortcut)) => {
                let verb = if portal { "bound" } else { "registered" };
                let reason = format!(
                    "{shortcut} is {verb} — {}",
                    capabilities.global_hotkey.reason
                );
                capabilities.global_hotkey.reason = reason;
            }
            None => {}
        }
    }
    if server == DisplayServer::Wayland && runtime.gnome_desktop {
        capabilities.click_through = cap(
            Supported,
            "an empty input region, kept as GTK's own input shape so every compositor configure preserves it — verified on GNOME 45 (Fedora 39)",
        );
    }
    if server == DisplayServer::Wayland && runtime.gnome_bridge_active {
        capabilities.always_on_top = cap(
            Supported,
            "kept above other windows by the PULSE GNOME bridge (Mutter make_above), whichever application has focus — verified on GNOME 45",
        );
        let shortcut = runtime
            .gnome_bridge_hotkey
            .as_deref()
            .unwrap_or("The bridge's shortcut");
        capabilities.global_hotkey = cap(
            Supported,
            &format!(
                "{shortcut} is registered with Mutter by the PULSE GNOME bridge and reaches PULSE whichever application has focus — verified on GNOME 45"
            ),
        );
    }
    if let Some(Err(error)) = &runtime.tray {
        capabilities.tray = cap(
            Unsupported,
            &format!("the tray icon could not be created: {error}"),
        );
    }
    capabilities
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linux(session: Option<&str>, wayland: bool, x11: bool, gdk: Option<&str>) -> SessionFacts {
        SessionFacts {
            os: "linux".into(),
            xdg_session_type: session.map(String::from),
            wayland_display: wayland.then(|| "wayland-0".to_string()),
            display: x11.then(|| ":0".to_string()),
            gdk_backend: gdk.map(String::from),
        }
    }

    #[test]
    fn gtk_backend_detection_follows_gtk_rules() {
        assert_eq!(
            detect_display_server(&linux(Some("wayland"), true, true, None)),
            DisplayServer::Wayland
        );
        assert_eq!(
            detect_display_server(&linux(Some("wayland"), true, true, Some("x11"))),
            DisplayServer::XWayland
        );
        assert_eq!(
            detect_display_server(&linux(Some("x11"), false, true, None)),
            DisplayServer::X11
        );
        assert_eq!(
            detect_display_server(&linux(Some("wayland"), true, true, Some("wayland,x11"))),
            DisplayServer::Wayland
        );
        assert_eq!(
            detect_display_server(&linux(None, false, false, None)),
            DisplayServer::Other
        );
        let windows = SessionFacts {
            os: "windows".into(),
            ..SessionFacts::default()
        };
        assert_eq!(detect_display_server(&windows), DisplayServer::Windows);
        let mac = SessionFacts {
            os: "macos".into(),
            ..SessionFacts::default()
        };
        assert_eq!(detect_display_server(&mac), DisplayServer::Other);
    }

    #[test]
    fn windows_expects_every_capability() {
        let windows = expected_capabilities(DisplayServer::Windows);
        for capability in [
            &windows.always_on_top,
            &windows.click_through,
            &windows.positioning,
            &windows.transparent_window,
            &windows.global_hotkey,
            &windows.multi_monitor_positioning,
        ] {
            assert_eq!(capability.status, Supported);
            assert!(
                capability.reason.contains("not yet verified"),
                "{}",
                capability.reason
            );
        }
    }

    #[test]
    fn x11_supports_stacking_click_through_and_placement() {
        let x11 = expected_capabilities(DisplayServer::X11);
        assert_eq!(x11.always_on_top.status, Supported);
        assert_eq!(x11.click_through.status, Supported);
        assert_eq!(x11.positioning.status, Supported);
        assert_eq!(x11.global_hotkey.status, Supported);
    }

    #[test]
    fn wayland_never_claims_what_the_protocol_does_not_allow() {
        let wayland = expected_capabilities(DisplayServer::Wayland);
        assert_ne!(wayland.always_on_top.status, Supported);
        assert_eq!(wayland.positioning.status, Unsupported);
        assert_eq!(wayland.multi_monitor_positioning.status, Unsupported);
        assert_ne!(wayland.global_hotkey.status, Supported);
        assert_ne!(
            wayland.click_through.status, Supported,
            "not claimed without runtime evidence"
        );
        assert_eq!(wayland.transparent_window.status, Supported);
    }

    #[test]
    fn xwayland_hotkeys_are_limited() {
        assert_eq!(
            expected_capabilities(DisplayServer::XWayland)
                .global_hotkey
                .status,
            Limited
        );
    }

    #[test]
    fn an_unsupported_platform_claims_nothing() {
        let other = expected_capabilities(DisplayServer::Other);
        assert_eq!(other.always_on_top.status, Unsupported);
        assert_eq!(other.click_through.status, Unsupported);
    }

    #[test]
    fn runtime_failures_override_expectations() {
        let facts = RuntimeFacts {
            hotkey: Some(Err("Ctrl+Shift+F12 is already taken".into())),
            tray: Some(Err("no StatusNotifier host".into())),
            hotkey_backend: Some(ShortcutBackend::Plugin),
            ..RuntimeFacts::default()
        };
        let result = capabilities(DisplayServer::X11, &facts);
        assert_eq!(result.global_hotkey.status, Unsupported);
        assert!(result.global_hotkey.reason.contains("already taken"));
        assert_eq!(result.tray.status, Unsupported);

        let ok = capabilities(
            DisplayServer::X11,
            &RuntimeFacts {
                hotkey: Some(Ok("Ctrl+Shift+F12".into())),
                tray: Some(Ok(())),
                hotkey_backend: Some(ShortcutBackend::Plugin),
                ..RuntimeFacts::default()
            },
        );
        assert!(ok
            .global_hotkey
            .reason
            .starts_with("Ctrl+Shift+F12 is registered"));
    }

    #[test]
    fn wayland_shortcut_is_supported_only_through_a_bound_portal() {
        let portal = |hotkey| RuntimeFacts {
            hotkey,
            hotkey_backend: Some(ShortcutBackend::Portal { version: 1 }),
            ..RuntimeFacts::default()
        };
        let bound = capabilities(
            DisplayServer::Wayland,
            &portal(Some(Ok("Ctrl+Shift+F12".into()))),
        );
        assert_eq!(bound.global_hotkey.status, Supported);
        assert!(bound
            .global_hotkey
            .reason
            .starts_with("Ctrl+Shift+F12 is bound"));
        assert!(bound.global_hotkey.reason.contains("XDG Desktop Portal"));
        // Always-on-top is a separate capability: the portal changes nothing there.
        assert_eq!(bound.always_on_top.status, Limited);

        let declined = capabilities(
            DisplayServer::Wayland,
            &portal(Some(Err("declined".into()))),
        );
        assert_eq!(declined.global_hotkey.status, Unsupported);
        assert!(declined.global_hotkey.reason.contains("not bound"));

        let missing = capabilities(
            DisplayServer::Wayland,
            &RuntimeFacts {
                hotkey_backend: Some(ShortcutBackend::Unavailable {
                    reason: "the portal has no GlobalShortcuts interface".into(),
                }),
                ..RuntimeFacts::default()
            },
        );
        assert_eq!(missing.global_hotkey.status, Unsupported);
        assert!(missing.global_hotkey.reason.contains("no GlobalShortcuts"));
        assert_eq!(
            capabilities(DisplayServer::Wayland, &RuntimeFacts::default())
                .global_hotkey
                .status,
            Unsupported,
            "never claimed before the portal is found"
        );
    }

    #[test]
    fn gnome_click_through_is_supported_and_other_compositors_stay_limited() {
        let gnome = capabilities(
            DisplayServer::Wayland,
            &RuntimeFacts {
                gnome_desktop: true,
                ..RuntimeFacts::default()
            },
        );
        assert_eq!(gnome.click_through.status, Supported);
        assert!(gnome.click_through.reason.contains("GNOME 45"));
        // Without the bridge, stacking stays the compositor's decision.
        assert_eq!(gnome.always_on_top.status, Limited);

        let other = capabilities(DisplayServer::Wayland, &RuntimeFacts::default());
        assert_eq!(other.click_through.status, Limited);
    }

    #[test]
    fn the_active_gnome_bridge_unlocks_stacking_and_the_shortcut() {
        let facts = RuntimeFacts {
            gnome_desktop: true,
            gnome_bridge_active: true,
            gnome_bridge_hotkey: Some("Ctrl+Shift+F12".into()),
            // The portal is missing on GNOME 45; the bridge replaces it.
            hotkey_backend: Some(ShortcutBackend::Unavailable {
                reason: "no GlobalShortcuts".into(),
            }),
            ..RuntimeFacts::default()
        };
        let result = capabilities(DisplayServer::Wayland, &facts);
        assert_eq!(result.always_on_top.status, Supported);
        assert!(result.always_on_top.reason.contains("GNOME bridge"));
        assert_eq!(result.global_hotkey.status, Supported);
        assert!(result.global_hotkey.reason.starts_with("Ctrl+Shift+F12"));
        // Placement is still the compositor's: the bridge does not move windows.
        assert_eq!(result.positioning.status, Unsupported);

        // The bridge never changes an X11 session.
        let x11 = capabilities(DisplayServer::XWayland, &facts);
        assert_ne!(x11.global_hotkey.status, Supported);
        assert!(!x11.always_on_top.reason.contains("GNOME bridge"));
    }

    #[test]
    fn serialises_in_camel_case() {
        let json =
            serde_json::to_value(expected_capabilities(DisplayServer::Wayland)).expect("json");
        assert_eq!(json["displayServer"], "wayland");
        assert_eq!(json["clickThrough"]["status"], "limited");
        assert!(json.get("multiMonitorPositioning").is_some());
    }
}
