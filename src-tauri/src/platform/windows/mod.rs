//! Windows platform implementation.
//!
//! Future phases reach WMI, PDH and vendor SDKs from here; nothing outside this
//! module may do so.
//!
//! The module is compiled on **every** platform, but anything touching the
//! Windows API is gated behind `#[cfg(target_os = "windows")]`. The pure
//! arithmetic — FILETIME recombination, CPU counter semantics, the memory
//! convention, OS version formatting — therefore compiles and is unit-tested
//! on Fedora too. There is no reason for Windows maths to be untestable from a
//! Linux machine, and plenty of reason for it not to be.

#[cfg(target_os = "windows")]
use std::sync::Arc;

#[cfg(target_os = "windows")]
use super::{HostPlatform, PlatformKind};
#[cfg(target_os = "windows")]
use crate::metrics::providers::MetricProvider;

pub mod cpu;
pub mod memory;

/// Windows implementation of [`HostPlatform`].
#[cfg(target_os = "windows")]
#[derive(Debug, Default)]
pub struct WindowsPlatform;

#[cfg(target_os = "windows")]
impl WindowsPlatform {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "windows")]
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

    /// CPU from `GetSystemTimes` and memory from `GlobalMemoryStatusEx`.
    ///
    /// Both are available to any process, so neither needs administrator
    /// rights.
    fn metric_providers(&self) -> Vec<Arc<dyn MetricProvider>> {
        vec![cpu::provider(), memory::provider()]
    }

    /// Windows has a single compositor (DWM), so there is nothing analogous to
    /// the Wayland/X11 distinction to report.
    fn display_server(&self) -> Option<String> {
        None
    }
}

/// Windows 11 kept the major version 10; the build number is what separates
/// the two product generations.
// Compiled everywhere so its tests run on Fedora, but only *called* from the
// Windows-gated `HostPlatform` implementation.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const WINDOWS_11_MIN_BUILD: u32 = 22000;

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
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

    #[cfg(target_os = "windows")]
    #[test]
    fn kind_is_windows() {
        assert_eq!(WindowsPlatform::new().kind(), PlatformKind::Windows);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn display_server_is_not_reported() {
        assert!(WindowsPlatform::new().display_server().is_none());
    }
}
