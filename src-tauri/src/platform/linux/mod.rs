//! Linux / Fedora platform implementation.
//!
//! Compiled only on Linux. Future phases read `/proc`, `/sys` and `hwmon` from
//! here; nothing outside this module may do so.

pub mod cpu;
pub mod cpu_list;
pub mod cpu_sysfs;
pub mod cpu_thermal;
pub mod gpu;
pub mod hwmon;
pub mod memory;
pub mod network;
mod os_release;
pub mod processes;
pub mod storage;

#[cfg(any(target_os = "linux", test))]
use std::path::Path;
use std::sync::Arc;

use super::{HostPlatform, PlatformKind};
use crate::metrics::providers::MetricProvider;

/// Selects a working transparent renderer before GTK/WebKit initialize.
/// Nouveau's hardware DMA-BUF path crashes; disabling the renderer entirely
/// instead hits WebKitGTK's Skia backing-store bug (319864): transparent
/// updates use SrcOver instead of Copy and accumulate previous frames.
/// Software GL + shared-memory transport avoids both paths, retaining alpha.
/// These variables affect this process and its children only, never the session.
#[cfg(target_os = "linux")]
pub fn prepare_runtime_environment() {
    let changes = renderer_defaults(has_nouveau_drm_card(Path::new("/sys/class/drm")), |key| {
        std::env::var_os(key).is_some()
    });
    for (key, value) in &changes {
        std::env::set_var(key, value);
    }
    if !changes.is_empty() {
        eprintln!(
            "PULSE: nouveau DRM driver detected; using software GL with shared-memory WebKit transport to preserve transparent repaint"
        );
    }
}

/// An explicit renderer choice is kept as a whole: mixing half of an override
/// with our fallback can select another broken path. Test without mutating the
/// process environment (Rust tests run concurrently).
#[cfg(any(target_os = "linux", test))]
fn renderer_defaults(
    nouveau: bool,
    is_set: impl Fn(&str) -> bool,
) -> Vec<(&'static str, &'static str)> {
    const CHOICES: [&str; 4] = [
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_DMABUF_RENDERER_FORCE_SHM",
        "LIBGL_ALWAYS_SOFTWARE",
        "WEBKIT_DISABLE_COMPOSITING_MODE",
    ];
    if !nouveau || CHOICES.iter().any(|key| is_set(key)) {
        return Vec::new();
    }
    vec![
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1"),
    ]
}

#[cfg(target_os = "linux")]
fn has_nouveau_drm_card(drm_class: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(drm_class) else {
        return false;
    };

    entries.filter_map(Result::ok).any(|entry| {
        entry
            .file_name()
            .to_str()
            .map(is_drm_card_name)
            .unwrap_or(false)
            && std::fs::read_link(entry.path().join("device/driver"))
                .map(|driver| is_nouveau_driver_path(&driver))
                .unwrap_or(false)
    })
}

#[cfg(any(target_os = "linux", test))]
fn is_drm_card_name(name: &str) -> bool {
    let Some(index) = name.strip_prefix("card") else {
        return false;
    };

    !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(any(target_os = "linux", test))]
fn is_nouveau_driver_path(path: &Path) -> bool {
    path.file_name()
        .map(|name| name == "nouveau")
        .unwrap_or(false)
}

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
    /// CPU from `/proc/stat` and `/sys/devices/system/cpu/`, memory from
    /// `/proc/meminfo`, GPUs from `/sys/class/drm` and NVML, storage from
    /// `/sys/class/block`, `/proc/diskstats`, `/proc/self/mountinfo` and
    /// `statvfs`, network from `rtnetlink`, `nl80211` and `/sys/class/net`.
    ///
    /// All are world-readable, so none needs root. The one interface that
    /// does — the NVMe health log — is read opportunistically by the storage
    /// provider and reports `permissionDenied` when it is refused, which
    /// affects six metrics per NVMe device and nothing else.
    fn metric_providers(&self) -> Vec<Arc<dyn MetricProvider>> {
        vec![
            cpu::provider(),
            memory::provider(),
            gpu::provider(),
            storage::provider(),
            network::provider(),
            processes::provider(),
        ]
    }

    /// Processes from `/proc`, read directly. PULSE never runs `ps`, `top`,
    /// `pgrep` or `pidstat`.
    fn process_collector(&self) -> Option<Arc<dyn crate::processes::ProcessCollector>> {
        Some(processes::collector())
    }

    #[cfg(target_os = "linux")]
    fn process_inspector(&self) -> Option<Arc<dyn crate::processes::ProcessInspectorBackend>> {
        Some(processes::control::inspector_backend())
    }

    #[cfg(target_os = "linux")]
    fn process_control(&self) -> Option<Arc<dyn crate::processes::ProcessControlBackend>> {
        Some(processes::control::control_backend())
    }

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
    fn nouveau_keeps_compositing_with_software_gl_and_shared_memory() {
        assert_eq!(
            renderer_defaults(true, |_| false),
            vec![
                ("LIBGL_ALWAYS_SOFTWARE", "1"),
                ("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1"),
            ]
        );
        assert!(renderer_defaults(false, |_| false).is_empty());
    }

    #[test]
    fn explicit_renderer_choices_are_not_partially_overwritten() {
        for explicit in [
            "WEBKIT_DISABLE_DMABUF_RENDERER",
            "WEBKIT_DMABUF_RENDERER_FORCE_SHM",
            "LIBGL_ALWAYS_SOFTWARE",
            "WEBKIT_DISABLE_COMPOSITING_MODE",
        ] {
            assert!(renderer_defaults(true, |key| key == explicit).is_empty());
        }
    }

    #[test]
    fn recognises_drm_card_names() {
        assert!(is_drm_card_name("card0"));
        assert!(is_drm_card_name("card12"));
    }

    #[test]
    fn rejects_non_card_drm_names() {
        assert!(!is_drm_card_name("card"));
        assert!(!is_drm_card_name("card0-DP-1"));
        assert!(!is_drm_card_name("renderD128"));
        assert!(!is_drm_card_name("controlD64"));
    }

    #[test]
    fn recognises_nouveau_driver_paths() {
        assert!(is_nouveau_driver_path(Path::new(
            "../../../bus/pci/drivers/nouveau"
        )));
    }

    #[test]
    fn rejects_other_driver_paths() {
        assert!(!is_nouveau_driver_path(Path::new(
            "../../../bus/pci/drivers/nvidia"
        )));
        assert!(!is_nouveau_driver_path(Path::new(
            "../../../bus/pci/drivers/amdgpu"
        )));
    }

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
