//! Assembles the platform description reported to the UI.

use crate::platform::{self, PlatformInfo};

/// PULSE version, taken from `Cargo.toml` at compile time.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Collects host information through the platform abstraction.
pub fn collect() -> PlatformInfo {
    let host = platform::host();

    PlatformInfo {
        platform: host.kind(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        os_version: host.os_version(),
        display_server: host.display_server(),
        app_version: APP_VERSION.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::PlatformKind;

    #[test]
    fn collect_reports_the_compiled_target() {
        let info = collect();

        assert_eq!(info.os, std::env::consts::OS);
        assert_eq!(info.arch, std::env::consts::ARCH);
        assert_eq!(info.app_version, APP_VERSION);
        assert!(!info.app_version.is_empty());
    }

    #[test]
    fn display_server_is_linux_only() {
        let info = collect();

        if info.platform != PlatformKind::Linux {
            assert!(
                info.display_server.is_none(),
                "display server must only be reported on Linux"
            );
        }
    }
}
