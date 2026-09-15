//! Linux / Fedora platform implementation.
//!
//! Compiled only on Linux. Future phases read `/proc`, `/sys` and `hwmon` from
//! here; nothing outside this module may do so.

mod os_release;

use super::{HostPlatform, PlatformKind};

/// Linux implementation of [`HostPlatform`].
#[derive(Debug, Default)]
pub struct LinuxPlatform;

impl LinuxPlatform {
    pub fn new() -> Self {
        Self
    }
}

impl HostPlatform for LinuxPlatform {
    fn kind(&self) -> PlatformKind {
        PlatformKind::Linux
    }

    fn os_version(&self) -> Option<String> {
        os_release::pretty_name()
    }

    /// Detects Wayland vs X11.
    ///
    /// This matters well beyond cosmetics: the future Mini overlay has very
    /// different positioning capabilities on each (see
    /// `docs/architecture/mini-overlay.md`).
    fn display_server(&self) -> Option<String> {
        detect_display_server(
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var("WAYLAND_DISPLAY").is_ok(),
            std::env::var("DISPLAY").is_ok(),
        )
    }
}

/// Pure detection logic, kept separate from the environment so it is testable.
fn detect_display_server(
    session_type: Option<&str>,
    has_wayland_display: bool,
    has_x_display: bool,
) -> Option<String> {
    match session_type
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("wayland") => return Some("wayland".to_string()),
        Some("x11") => return Some("x11".to_string()),
        _ => {}
    }

    if has_wayland_display {
        Some("wayland".to_string())
    } else if has_x_display {
        Some("x11".to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_type_wins_over_environment_sockets() {
        assert_eq!(
            detect_display_server(Some("wayland"), false, true).as_deref(),
            Some("wayland")
        );
        assert_eq!(
            detect_display_server(Some("X11"), true, false).as_deref(),
            Some("x11")
        );
    }

    #[test]
    fn falls_back_to_socket_variables() {
        assert_eq!(
            detect_display_server(None, true, true).as_deref(),
            Some("wayland")
        );
        assert_eq!(
            detect_display_server(Some("tty"), false, true).as_deref(),
            Some("x11")
        );
    }

    #[test]
    fn headless_session_reports_nothing() {
        assert_eq!(detect_display_server(None, false, false), None);
        assert_eq!(detect_display_server(Some("tty"), false, false), None);
    }

    #[test]
    fn kind_is_linux() {
        assert_eq!(LinuxPlatform::new().kind(), PlatformKind::Linux);
    }
}
