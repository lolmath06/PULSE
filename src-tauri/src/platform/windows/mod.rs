//! Windows platform implementation.
//!
//! Compiled only on Windows. Future phases reach WMI, PDH and vendor SDKs from
//! here; nothing outside this module may do so.

use super::{HostPlatform, PlatformKind};

/// Windows implementation of [`HostPlatform`].
#[derive(Debug, Default)]
pub struct WindowsPlatform;

impl WindowsPlatform {
    pub fn new() -> Self {
        Self
    }
}

impl HostPlatform for WindowsPlatform {
    fn kind(&self) -> PlatformKind {
        PlatformKind::Windows
    }

    fn os_version(&self) -> Option<String> {
        let version = windows_version::OsVersion::current();
        Some(format_os_version(
            version.major,
            version.minor,
            version.build,
        ))
    }

    /// Windows has a single compositor (DWM), so there is nothing analogous to
    /// the Wayland/X11 distinction to report.
    fn display_server(&self) -> Option<String> {
        None
    }
}

/// Windows 11 kept the major version 10; the build number is what separates
/// the two product generations.
const WINDOWS_11_MIN_BUILD: u32 = 22000;

fn format_os_version(major: u32, minor: u32, build: u32) -> String {
    let product = match (major, build) {
        (10, b) if b >= WINDOWS_11_MIN_BUILD => "Windows 11".to_string(),
        (10, _) => "Windows 10".to_string(),
        (major, _) => format!("Windows {major}.{minor}"),
    };

    format!("{product} (build {build})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_windows_10_from_11_by_build() {
        assert_eq!(format_os_version(10, 0, 19045), "Windows 10 (build 19045)");
        assert_eq!(format_os_version(10, 0, 22631), "Windows 11 (build 22631)");
        assert_eq!(format_os_version(10, 0, 26100), "Windows 11 (build 26100)");
    }

    #[test]
    fn falls_back_to_raw_numbers_for_unknown_majors() {
        assert_eq!(format_os_version(6, 1, 7601), "Windows 6.1 (build 7601)");
    }

    #[test]
    fn kind_is_windows() {
        assert_eq!(WindowsPlatform::new().kind(), PlatformKind::Windows);
    }

    #[test]
    fn display_server_is_not_reported() {
        assert!(WindowsPlatform::new().display_server().is_none());
    }
}
