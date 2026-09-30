//! Gathers the facts `overlay::gnome_bridge` decides the GNOME bridge's
//! status from, and runs the two actions PULSE offers on it.
//!
//! Read-only, except where the user explicitly asks:
//!
//! - GNOME Shell's `org.gnome.Shell.Extensions` service on the session bus:
//!   `GetExtensionInfo(uuid)`, `ShellVersion`, `UserExtensionsEnabled`;
//! - the extension's `metadata.json` in the user's extensions directory;
//! - the extension's own GSettings key for its shortcut (read for display;
//!   written only when the user applies a new shortcut);
//! - `EnableExtension` / `DisableExtension`, only from the Enable and Disable
//!   buttons.
//!
//! Nothing here installs, copies or deletes files; installation stays the
//! user's `install.sh` (see `docs/overlay/gnome-bridge.md`). Nothing polls:
//! the facts are gathered at launch and when the user refreshes or acts.

use crate::overlay::capabilities::DisplayServer;
use crate::overlay::gnome_bridge::{DiskExtension, GnomeFacts, EXTENSION_UUID};

/// The extension's schema id and shortcut key (`schemas/*.gschema.xml`).
pub const SCHEMA_ID: &str = "org.gnome.shell.extensions.pulse-overlay";
pub const HOTKEY_KEY: &str = "toggle-overlays";

/// Whether `XDG_CURRENT_DESKTOP` (a colon-separated list) names GNOME.
pub fn is_gnome_desktop(current_desktop: Option<&str>) -> bool {
    current_desktop
        .unwrap_or_default()
        .split(':')
        .any(|entry| entry.trim().eq_ignore_ascii_case("gnome"))
}

/// The user's extensions directory for the bridge.
pub fn user_extension_dir() -> Option<std::path::PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::Path::new(&home).join(".local/share"))
        })?;
    Some(data.join("gnome-shell/extensions").join(EXTENSION_UUID))
}

/// Reads the version out of a `metadata.json`.
pub fn metadata_version(text: &str) -> Option<u32> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let version = value.get("version")?.as_f64()?;
    (version.is_finite() && version >= 0.0 && version <= u32::MAX as f64).then_some(version as u32)
}

fn read_disk() -> Option<DiskExtension> {
    let dir = user_extension_dir()?;
    let metadata = std::fs::read_to_string(dir.join("metadata.json")).ok()?;
    Some(DiskExtension {
        path: dir.display().to_string(),
        version: metadata_version(&metadata),
        installed_by_pulse: dir.join(".installed-by-pulse").is_file(),
    })
}

/// The repository's copy of the extension and its installer, when this PULSE
/// runs from a source checkout (a packaged build has none).
pub fn source_dir() -> Option<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../integrations/gnome-shell");
    dir.join("install.sh")
        .is_file()
        .then(|| dir.canonicalize().unwrap_or(dir).display().to_string())
}

/// Gathers the facts. Talks to D-Bus only on a GNOME Wayland session, and
/// must not run on the main thread.
pub fn gather(display: DisplayServer) -> GnomeFacts {
    let gnome_desktop = is_gnome_desktop(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref());
    let wayland = display == DisplayServer::Wayland;
    let applicable = gnome_desktop && wayland;
    GnomeFacts {
        gnome_desktop,
        wayland,
        shell: applicable.then(native::query_shell),
        disk: if applicable { read_disk() } else { None },
        handshake: None,
    }
}

/// `EnableExtension` / `DisableExtension` through GNOME Shell's own API.
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    native::set_enabled(enabled)
}

/// The bridge's shortcut from its GSettings key, as PULSE writes shortcuts.
/// `loaded_from`: the directory GNOME Shell reports it loaded the extension
/// from, preferred over the user directory.
pub fn read_hotkey(loaded_from: Option<&str>) -> Option<String> {
    native::read_hotkey(loaded_from)
}

/// Writes the bridge's shortcut (a `Ctrl+Shift+F12`-style string), or clears
/// it with `None`. Mutter picks up the new binding at once.
pub fn write_hotkey(shortcut: Option<&str>) -> Result<(), String> {
    native::write_hotkey(shortcut)
}

#[cfg(target_os = "linux")]
mod native {
    use std::collections::HashMap;

    use gtk::gio;
    use gtk::gio::prelude::*;
    use zbus::zvariant::OwnedValue;

    use super::{user_extension_dir, HOTKEY_KEY, SCHEMA_ID};
    use crate::overlay::gnome_bridge::{
        from_gtk_accelerator, to_gtk_accelerator, ShellExtensionInfo, ShellExtensionState,
        ShellFacts, EXTENSION_UUID,
    };

    const DEST: &str = "org.gnome.Shell.Extensions";
    const PATH: &str = "/org/gnome/Shell/Extensions";

    fn proxy() -> Result<zbus::blocking::Proxy<'static>, String> {
        let connection = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
        zbus::blocking::Proxy::new_owned(connection, DEST, PATH, DEST).map_err(|e| e.to_string())
    }

    pub fn query_shell() -> Result<ShellFacts, String> {
        let proxy = proxy()?;
        let shell_version: String = proxy
            .get_property("ShellVersion")
            .map_err(|e| e.to_string())?;
        let user_extensions_enabled: bool =
            proxy.get_property("UserExtensionsEnabled").unwrap_or(true);
        let info: HashMap<String, OwnedValue> = proxy
            .call("GetExtensionInfo", &(EXTENSION_UUID,))
            .map_err(|e| e.to_string())?;
        let number = |key: &str| info.get(key).and_then(|v| f64::try_from(v).ok());
        let extension = number("state").map(|state| ShellExtensionInfo {
            state: ShellExtensionState::from_code(state),
            version: number("version")
                .filter(|v| v.is_finite() && *v >= 0.0 && *v <= u32::MAX as f64)
                .map(|v| v as u32),
            error: info
                .get("error")
                .and_then(|v| <&str>::try_from(v).ok())
                .unwrap_or_default()
                .chars()
                .take(500)
                .collect(),
            path: info
                .get("path")
                .and_then(|v| <&str>::try_from(v).ok())
                .filter(|path| !path.is_empty())
                .map(String::from),
        });
        Ok(ShellFacts {
            shell_version: shell_version.chars().take(32).collect(),
            user_extensions_enabled,
            extension,
        })
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        let method = if enabled {
            "EnableExtension"
        } else {
            "DisableExtension"
        };
        let done: bool = proxy()?
            .call(method, &(EXTENSION_UUID,))
            .map_err(|e| e.to_string())?;
        if done {
            Ok(())
        } else {
            Err(format!(
                "GNOME Shell declined to {} the extension",
                if enabled { "enable" } else { "disable" }
            ))
        }
    }

    /// The extension's settings, from the schema compiled in its own
    /// directory (`install.sh` compiles it there, as GNOME expects).
    fn settings(loaded_from: Option<&str>) -> Result<gio::Settings, String> {
        let dir = loaded_from
            .map(std::path::PathBuf::from)
            .or_else(user_extension_dir)
            .ok_or("no home directory")?
            .join("schemas");
        let source = gio::SettingsSchemaSource::from_directory(
            &dir,
            gio::SettingsSchemaSource::default().as_ref(),
            false,
        )
        .map_err(|e| e.to_string())?;
        let schema = source
            .lookup(SCHEMA_ID, false)
            .ok_or("the extension's schema is not installed")?;
        Ok(gio::Settings::new_full(
            &schema,
            None::<&gio::SettingsBackend>,
            None,
        ))
    }

    pub fn read_hotkey(loaded_from: Option<&str>) -> Option<String> {
        let settings = settings(loaded_from).ok()?;
        let first = settings.strv(HOTKEY_KEY).first()?.to_string();
        from_gtk_accelerator(&first)
    }

    pub fn write_hotkey(shortcut: Option<&str>) -> Result<(), String> {
        let accelerators: Vec<String> = match shortcut {
            Some(shortcut) => vec![to_gtk_accelerator(shortcut)
                .ok_or_else(|| format!("{shortcut} is not a shortcut GNOME can bind"))?],
            None => Vec::new(),
        };
        let settings = settings(None)?;
        settings
            .set_strv(
                HOTKEY_KEY,
                accelerators
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
            .map_err(|e| e.to_string())?;
        gio::Settings::sync();
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
mod native {
    use crate::overlay::gnome_bridge::ShellFacts;

    pub fn query_shell() -> Result<ShellFacts, String> {
        Err("GNOME Shell exists only on Linux".into())
    }
    pub fn set_enabled(_enabled: bool) -> Result<(), String> {
        Err("GNOME Shell exists only on Linux".into())
    }
    pub fn read_hotkey(_loaded_from: Option<&str>) -> Option<String> {
        None
    }
    pub fn write_hotkey(_shortcut: Option<&str>) -> Result<(), String> {
        Err("GNOME Shell exists only on Linux".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnome_is_recognised_in_the_desktop_list() {
        assert!(is_gnome_desktop(Some("GNOME")));
        assert!(is_gnome_desktop(Some("ubuntu:GNOME")));
        assert!(is_gnome_desktop(Some("gnome")));
        assert!(!is_gnome_desktop(Some("KDE")));
        assert!(!is_gnome_desktop(Some("GNOME-Flashback-ish")));
        assert!(!is_gnome_desktop(None));
    }

    #[test]
    fn the_metadata_version_is_read_defensively() {
        assert_eq!(
            metadata_version(r#"{ "uuid": "x", "version": 2 }"#),
            Some(2)
        );
        assert_eq!(metadata_version(r#"{ "version": 1.0 }"#), Some(1));
        assert_eq!(metadata_version(r#"{ "version": "2" }"#), None);
        assert_eq!(metadata_version(r#"{ "version": -1 }"#), None);
        assert_eq!(metadata_version("not json"), None);
    }

    #[test]
    fn outside_gnome_wayland_nothing_is_queried() {
        let facts = gather(DisplayServer::X11);
        assert!(!facts.wayland);
        assert!(facts.shell.is_none());
        assert!(facts.disk.is_none());
    }
}
