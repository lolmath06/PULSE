//! Platform abstraction layer.
//!
//! This module is the single boundary where Windows and Linux differ. Higher
//! layers (`services`, `commands`) depend only on the [`HostPlatform`] trait and
//! never on `#[cfg(target_os = ...)]`.
//!
//! Adding a capability means:
//! 1. adding a method to [`HostPlatform`] (with a safe default when sensible),
//! 2. implementing it in `linux/` and `windows/`,
//! 3. exposing it through a service and a command.
//!
//! Only this file and its submodules may use `cfg(target_os)` for OS
//! selection. Nothing above this layer — not `services`, not `commands`, and
//! certainly not `metrics` — contains a single one.
//!
//! Within the layer, the gating is deliberately narrow: both `linux` and
//! `windows` are compiled on every host, and only the parts that actually touch
//! the operating system (FFI calls, the `HostPlatform` implementations, tests
//! that read the real `/proc`) are gated. The pure logic — `/proc` parsing,
//! FILETIME arithmetic, the memory convention — compiles and is unit-tested
//! everywhere, so a Fedora test run catches a broken Windows formula and a
//! Windows run catches a broken `/proc` parser. No OS-specific *dependency* is
//! ever pulled into the other platform's build: those are declared under
//! `[target.'cfg(target_os = "...")'.dependencies]` in `Cargo.toml`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::metrics::providers::MetricProvider;
use crate::processes::ProcessCollector;

// Both platform modules are compiled on every host. Their OS-specific parts
// (FFI calls, `HostPlatform` implementations) are gated internally, while the
// pure logic — `/proc` parsing, FILETIME arithmetic, the memory convention —
// compiles and is tested everywhere. That is what lets Fedora CI catch a
// broken Windows formula and vice versa.
pub mod linux;
// NVIDIA telemetry is the same library on both operating systems, so it lives
// beside them rather than inside either. Loaded at runtime; see its docs.
pub mod gpu;
pub mod nvml;
pub mod windows;

/// Platform families PULSE ships a backend implementation for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlatformKind {
    Windows,
    Linux,
    /// Compiles and runs, but no system integration is available.
    Unsupported,
}

impl PlatformKind {
    /// The platform this binary was compiled for.
    pub const fn current() -> Self {
        #[cfg(target_os = "windows")]
        {
            PlatformKind::Windows
        }
        #[cfg(target_os = "linux")]
        {
            PlatformKind::Linux
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            PlatformKind::Unsupported
        }
    }

    /// Whether PULSE has a real system integration for this platform.
    pub const fn is_supported(self) -> bool {
        !matches!(self, PlatformKind::Unsupported)
    }
}

/// Structured description of the host, sent to the frontend.
///
/// Mirrored by `src/types/platform.ts`; the two must stay in sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub platform: PlatformKind,
    pub os: String,
    pub arch: String,
    pub os_version: Option<String>,
    pub display_server: Option<String>,
    pub app_version: String,
}

/// Capabilities every supported platform must provide.
///
/// Phase 0 defines the seam with a minimal surface; metrics, sensors and the
/// Mini overlay's window handling will extend this trait in later phases.
pub trait HostPlatform: Send + Sync {
    /// Which platform family this implementation serves.
    fn kind(&self) -> PlatformKind;

    /// Human readable OS/distribution name, when detectable.
    fn os_version(&self) -> Option<String>;

    /// Display server in use. Meaningful on Linux (`wayland` / `x11`); `None`
    /// elsewhere.
    fn display_server(&self) -> Option<String> {
        None
    }

    /// The metric providers available on this platform.
    ///
    /// **This is where PULSE decides which data sources exist**, and it is the
    /// reason `MetricsEngine` needs no `cfg(target_os)` of its own: the engine
    /// is handed a list of providers and never asks where they came from.
    ///
    /// Returning an empty list is valid — it yields an engine with an empty
    /// catalog rather than a failure.
    fn metric_providers(&self) -> Vec<Arc<dyn MetricProvider>> {
        Vec::new()
    }

    /// The reader for this host's process table, when PULSE has one.
    ///
    /// Deliberately **not** a metric provider. Processes are high-cardinality
    /// and ephemeral, so they are served by [`ProcessSnapshotService`] through
    /// its own command rather than by the metrics engine — see
    /// `docs/metrics/processes.md`. The three low-cardinality *counts* are a
    /// separate, ordinary provider.
    ///
    /// `None` means PULSE has no implementation here, which the interface
    /// reports as such rather than as an empty process list.
    ///
    /// [`ProcessSnapshotService`]: crate::processes::ProcessSnapshotService
    fn process_collector(&self) -> Option<Arc<dyn ProcessCollector>> {
        None
    }
}

/// Fallback used when PULSE is built for a platform it has no integration for.
///
/// It keeps the crate compiling and testable everywhere instead of failing the
/// build, while [`PlatformKind::is_supported`] lets callers react.
#[derive(Debug, Default)]
pub struct UnsupportedPlatform;

impl HostPlatform for UnsupportedPlatform {
    fn kind(&self) -> PlatformKind {
        PlatformKind::Unsupported
    }

    fn os_version(&self) -> Option<String> {
        None
    }
}

/// Returns the platform implementation for the current host.
///
/// This is the only function that selects an implementation; everything above
/// it is platform-agnostic.
pub fn host() -> Box<dyn HostPlatform> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxPlatform::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsPlatform::new())
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Box::new(UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_kind_matches_the_compilation_target() {
        let expected = match std::env::consts::OS {
            "linux" => PlatformKind::Linux,
            "windows" => PlatformKind::Windows,
            _ => PlatformKind::Unsupported,
        };

        assert_eq!(PlatformKind::current(), expected);
    }

    #[test]
    fn host_implementation_agrees_with_current_kind() {
        assert_eq!(host().kind(), PlatformKind::current());
    }

    #[test]
    fn platform_kind_serialises_as_lowercase() {
        let json = serde_json::to_string(&PlatformKind::Linux).expect("serialise");
        assert_eq!(json, "\"linux\"");
    }

    #[test]
    fn platform_info_uses_camel_case_for_the_frontend() {
        let info = PlatformInfo {
            platform: PlatformKind::Windows,
            os: "windows".into(),
            arch: "x86_64".into(),
            os_version: Some("Windows 11".into()),
            display_server: None,
            app_version: "0.1.0-dev".into(),
        };

        let json = serde_json::to_value(&info).expect("serialise");

        assert!(json.get("osVersion").is_some());
        assert!(json.get("appVersion").is_some());
        assert!(json.get("displayServer").is_some());
        assert!(json.get("os_version").is_none());
    }

    #[test]
    fn unsupported_platform_is_flagged_as_such() {
        assert!(!PlatformKind::Unsupported.is_supported());
        assert!(PlatformKind::Linux.is_supported());
        assert!(PlatformKind::Windows.is_supported());
        assert_eq!(UnsupportedPlatform.kind(), PlatformKind::Unsupported);
        assert!(UnsupportedPlatform.os_version().is_none());
        assert!(UnsupportedPlatform.display_server().is_none());
    }
}
