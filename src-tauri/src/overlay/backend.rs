//! Overlay backends: which native path puts PULSE's overlay windows on screen
//! in this session, and the one Edit / Locked contract they all honour.
//!
//! | backend            | where                    | above others              | click-through            |
//! | ------------------ | ------------------------ | ------------------------- | ------------------------ |
//! | Windows native     | Windows                  | `HWND_TOPMOST`            | `WS_EX_TRANSPARENT`      |
//! | GNOME bridge       | GNOME on Wayland + ext.  | Mutter `make_above`       | GTK input shape (empty)  |
//! | Standard Wayland   | other Wayland sessions   | the compositor decides    | GTK input shape (empty)  |
//! | X11                | X11, XWayland            | `_NET_WM_STATE_ABOVE`     | X Shape input (empty)    |
//! | Unsupported        | anything else            | —                         | —                        |
//!
//! The rest of PULSE asks for a state — [`window_policy`] — and never for a
//! mechanism. The mechanisms live in `crate::overlay_native` (window side)
//! and in the GNOME Shell extension (compositor side).

use serde::{Deserialize, Serialize};

use super::capabilities::DisplayServer;
use super::input::OverlayInputMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OverlayBackendKind {
    WindowsNative,
    GnomeBridge,
    StandardWayland,
    X11,
    Unsupported,
}

/// Which backend this session uses. The GNOME bridge is chosen only while
/// GNOME Shell reports the extension enabled; otherwise a Wayland session is
/// a standard Wayland window, whatever is installed.
pub fn select_backend(server: DisplayServer, gnome_bridge_active: bool) -> OverlayBackendKind {
    match server {
        DisplayServer::Windows => OverlayBackendKind::WindowsNative,
        DisplayServer::Wayland if gnome_bridge_active => OverlayBackendKind::GnomeBridge,
        DisplayServer::Wayland => OverlayBackendKind::StandardWayland,
        DisplayServer::X11 | DisplayServer::XWayland => OverlayBackendKind::X11,
        DisplayServer::Other => OverlayBackendKind::Unsupported,
    }
}

/// Where a backend's behaviour stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verification {
    /// Seen working on a real machine.
    PhysicallyVerified,
    /// Implemented and compiled; not yet seen on a real machine.
    Implemented,
    /// Works as far as the platform allows; see the capabilities.
    BestEffort,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayBackendInfo {
    pub kind: OverlayBackendKind,
    pub label: String,
    pub detail: String,
    pub verification: Verification,
}

pub fn backend_info(kind: OverlayBackendKind) -> OverlayBackendInfo {
    let (label, detail, verification) = match kind {
        OverlayBackendKind::WindowsNative => (
            "Windows native overlay",
            "A layered, topmost, no-activate tool window: HWND_TOPMOST keeps it above, WS_EX_NOACTIVATE keeps focus where it is, and WS_EX_TRANSPARENT passes clicks through while locked. Implemented and compiled; not yet verified on a Windows machine.",
            Verification::Implemented,
        ),
        OverlayBackendKind::GnomeBridge => (
            "GNOME native bridge",
            "The PULSE extension inside GNOME Shell keeps overlays above other windows (Mutter make_above) and delivers the overlay shortcut through Mutter; PULSE keeps locked overlays click-through. Verified on Fedora 39 / GNOME 45.",
            Verification::PhysicallyVerified,
        ),
        OverlayBackendKind::StandardWayland => (
            "Standard Wayland window",
            "An ordinary Wayland window: transparent, click-through when locked, but the compositor decides whether it stays above a focused application and where it goes.",
            Verification::BestEffort,
        ),
        OverlayBackendKind::X11 => (
            "X11 window",
            "An always-on-top X11 window (_NET_WM_STATE_ABOVE) with an empty input shape when locked, placed where it asks.",
            Verification::BestEffort,
        ),
        OverlayBackendKind::Unsupported => (
            "Unavailable",
            "Desktop overlays are not supported on this platform.",
            Verification::Unsupported,
        ),
    };
    OverlayBackendInfo {
        kind,
        label: label.into(),
        detail: detail.into(),
        verification,
    }
}

/// The window state for Edit or Locked, the same meaning on every backend:
///
/// - **Edit**: takes the pointer (drag, resize, controls) and may take focus;
/// - **Locked**: click-through, never takes focus;
/// - **both**: asks to stay above other windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayWindowPolicy {
    pub input: OverlayInputMode,
    pub focusable: bool,
    pub keep_above: bool,
}

pub fn window_policy(locked: bool) -> OverlayWindowPolicy {
    OverlayWindowPolicy {
        input: OverlayInputMode::for_locked(locked),
        focusable: !locked,
        keep_above: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [OverlayBackendKind; 5] = [
        OverlayBackendKind::WindowsNative,
        OverlayBackendKind::GnomeBridge,
        OverlayBackendKind::StandardWayland,
        OverlayBackendKind::X11,
        OverlayBackendKind::Unsupported,
    ];

    #[test]
    fn each_session_gets_exactly_one_backend() {
        use DisplayServer::*;
        assert_eq!(
            select_backend(Windows, false),
            OverlayBackendKind::WindowsNative
        );
        assert_eq!(
            select_backend(Windows, true),
            OverlayBackendKind::WindowsNative
        );
        assert_eq!(
            select_backend(Wayland, true),
            OverlayBackendKind::GnomeBridge
        );
        assert_eq!(
            select_backend(Wayland, false),
            OverlayBackendKind::StandardWayland
        );
        assert_eq!(select_backend(X11, true), OverlayBackendKind::X11);
        assert_eq!(select_backend(XWayland, true), OverlayBackendKind::X11);
        assert_eq!(select_backend(Other, true), OverlayBackendKind::Unsupported);
    }

    #[test]
    fn edit_and_locked_mean_the_same_thing_everywhere() {
        let edit = window_policy(false);
        assert_eq!(edit.input, OverlayInputMode::Interactive);
        assert!(edit.focusable && edit.keep_above);
        let locked = window_policy(true);
        assert_eq!(locked.input, OverlayInputMode::ClickThrough);
        assert!(!locked.focusable && locked.keep_above);
    }

    #[test]
    fn every_backend_is_described_honestly() {
        for kind in ALL {
            let info = backend_info(kind);
            assert_eq!(info.kind, kind);
            assert!(!info.label.is_empty() && !info.detail.is_empty());
        }
        assert_eq!(
            backend_info(OverlayBackendKind::GnomeBridge).verification,
            Verification::PhysicallyVerified
        );
        // Nothing on Windows is claimed as seen working.
        let windows = backend_info(OverlayBackendKind::WindowsNative);
        assert_eq!(windows.verification, Verification::Implemented);
        assert!(windows.detail.contains("not yet verified"));
    }
}
