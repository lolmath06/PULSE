//! The GNOME Shell overlay bridge, as PULSE sees it: is the companion
//! extension installed, enabled, running, talking to PULSE — and if not, why.
//!
//! The extension (`integrations/gnome-shell/pulse-overlay@jamby/`) runs inside
//! GNOME Shell and does, compositor-side, what a Wayland client cannot: keep
//! PULSE's overlays above other windows (`Meta.Window.make_above`) and deliver
//! the overlay shortcut through Mutter. Both are physically verified on
//! Fedora 39 / GNOME 45 (`docs/overlay/gnome-bridge.md`).
//!
//! Everything here is a pure decision from plain facts, so every state is
//! tested without GNOME. The facts come from three read-only places
//! (`crate::gnome_bridge` gathers them): GNOME Shell's own
//! `org.gnome.Shell.Extensions` D-Bus API (authoritative for "enabled" and
//! "errored"), the extension's `metadata.json` on disk (what a new login
//! would load), and the extension's `Hello` to PULSE's bridge (proof that the
//! running copy reaches this PULSE).

use serde::{Deserialize, Serialize};

/// The extension's UUID. Unchanged since the prototype, so an existing
/// installation is recognised and updated in place.
pub const EXTENSION_UUID: &str = "pulse-overlay@jamby";

/// The extension version this PULSE ships (`metadata.json` → `version`).
pub const BUNDLED_EXTENSION_VERSION: u32 = 2;

/// The GNOME Shell major versions the extension declares and was verified on.
pub const SUPPORTED_SHELL_MAJORS: &[u32] = &[45];

/// GNOME Shell's `ExtensionState` (js/misc/extensionUtils.js, GNOME 45).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShellExtensionState {
    Enabled,
    Disabled,
    Error,
    OutOfDate,
    Downloading,
    Initialized,
    Disabling,
    Enabling,
    Uninstalled,
    Unknown,
}

impl ShellExtensionState {
    /// GNOME sends the state as a D-Bus double.
    pub fn from_code(code: f64) -> Self {
        match code as i64 {
            1 => Self::Enabled,
            2 => Self::Disabled,
            3 => Self::Error,
            4 => Self::OutOfDate,
            5 => Self::Downloading,
            6 => Self::Initialized,
            7 => Self::Disabling,
            8 => Self::Enabling,
            99 => Self::Uninstalled,
            _ => Self::Unknown,
        }
    }
}

/// What GNOME Shell reports about the extension (`GetExtensionInfo`).
#[derive(Debug, Clone, PartialEq)]
pub struct ShellExtensionInfo {
    pub state: ShellExtensionState,
    /// The `version` of the copy GNOME Shell **loaded** (at login).
    pub version: Option<u32>,
    pub error: String,
    /// Where GNOME Shell loaded it from.
    pub path: Option<String>,
}

/// What GNOME Shell's Extensions service reports, when it answered.
#[derive(Debug, Clone, PartialEq)]
pub struct ShellFacts {
    pub shell_version: String,
    pub user_extensions_enabled: bool,
    /// `None`: GNOME Shell does not know the extension (not installed, or
    /// installed since login — GNOME 45 discovers extensions only at login).
    pub extension: Option<ShellExtensionInfo>,
}

/// The extension's files in the user's extensions directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskExtension {
    pub path: String,
    pub version: Option<u32>,
    /// Installed by PULSE's `install.sh` (its marker file is present).
    pub installed_by_pulse: bool,
}

/// The running extension greeted this PULSE over the bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Handshake {
    pub version: u32,
    /// Seconds since the Unix epoch.
    pub at: u64,
}

/// Everything the status is decided from.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GnomeFacts {
    /// `XDG_CURRENT_DESKTOP` names GNOME.
    pub gnome_desktop: bool,
    /// The session is native Wayland (the only place the bridge is needed).
    pub wayland: bool,
    /// `Err`: the Extensions service did not answer (not GNOME Shell, or no
    /// session bus). `None`: not asked (not a GNOME Wayland session).
    pub shell: Option<Result<ShellFacts, String>>,
    pub disk: Option<DiskExtension>,
    pub handshake: Option<Handshake>,
}

/// The one-word state the UI shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BridgeState {
    /// Not a GNOME Wayland session: the bridge does not apply here.
    NotApplicable,
    /// GNOME's Extensions service did not answer.
    Unavailable,
    /// This GNOME Shell version is not one the extension supports.
    Incompatible,
    NotInstalled,
    /// Files are in place but GNOME Shell has not loaded them yet (log out).
    InstalledNeedsLogin,
    /// User extensions are switched off for the whole session.
    ExtensionsOff,
    Disabled,
    /// GNOME Shell reported an error while enabling it.
    Error,
    /// Enabled and running.
    Active,
    /// Enabling, disabling, initialising: GNOME is in between.
    Changing,
}

/// The bridge's status for the UI and the capability model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GnomeBridgeStatus {
    pub state: BridgeState,
    /// One sentence for a chip or a heading.
    pub summary: String,
    /// A stable identifier of `summary`, so the interface can say it in the
    /// user's language without reading the English. Never shown.
    #[serde(default)]
    pub summary_code: String,
    /// What to do next, if anything.
    pub guidance: Option<String>,
    /// A stable identifier of `guidance`, like `summary_code`.
    #[serde(default)]
    pub guidance_code: Option<String>,
    pub shell_version: Option<String>,
    /// The copy GNOME Shell loaded at login.
    pub running_version: Option<u32>,
    /// The copy on disk (what the next login loads).
    pub installed_version: Option<u32>,
    pub bundled_version: u32,
    pub install_path: Option<String>,
    pub installed_by_pulse: bool,
    /// The running extension reached this PULSE (`Hello`).
    pub connected: bool,
    /// A newer copy is on disk than the one running: log out to load it.
    pub restart_pending: bool,
    /// This PULSE ships a newer extension than the one installed.
    pub update_available: bool,
    pub can_enable: bool,
    pub can_disable: bool,
    pub error: Option<String>,
}

impl GnomeBridgeStatus {
    /// Whether the compositor-side features are in force right now.
    pub fn is_active(&self) -> bool {
        self.state == BridgeState::Active
    }
}

/// The major version of a GNOME Shell version string (`"45.10"` → 45).
pub fn shell_major(version: &str) -> Option<u32> {
    version.split('.').next()?.trim().parse().ok()
}

impl GnomeBridgeStatus {
    fn say(&mut self, code: &str, summary: impl Into<String>) {
        self.summary_code = code.to_string();
        self.summary = summary.into();
    }

    fn advise(&mut self, code: &str, guidance: impl Into<String>) {
        self.guidance_code = Some(code.to_string());
        self.guidance = Some(guidance.into());
    }
}

/// Decides the bridge status from `facts`.
pub fn bridge_status(facts: &GnomeFacts) -> GnomeBridgeStatus {
    let mut status = GnomeBridgeStatus {
        state: BridgeState::NotApplicable,
        summary: String::new(),
        summary_code: String::new(),
        guidance: None,
        guidance_code: None,
        shell_version: None,
        running_version: None,
        installed_version: facts.disk.as_ref().and_then(|disk| disk.version),
        bundled_version: BUNDLED_EXTENSION_VERSION,
        install_path: facts.disk.as_ref().map(|disk| disk.path.clone()),
        installed_by_pulse: facts.disk.as_ref().is_some_and(|d| d.installed_by_pulse),
        connected: false,
        restart_pending: false,
        update_available: false,
        can_enable: false,
        can_disable: false,
        error: None,
    };

    if !(facts.gnome_desktop && facts.wayland) {
        if facts.wayland {
            status.say(
                "notGnome",
                "Not a GNOME session — the GNOME bridge does not apply here",
            );
        } else {
            status.say(
                "notWayland",
                "Not needed on this session — the bridge is for GNOME on Wayland",
            );
        }
        return status;
    }

    let shell = match &facts.shell {
        Some(Ok(shell)) => shell,
        Some(Err(error)) => {
            status.state = BridgeState::Unavailable;
            status.say(
                "shellNoAnswer",
                "GNOME Shell's extension service did not answer",
            );
            status.error = Some(error.clone());
            status.advise(
                "checkShell",
                "Check that GNOME Shell is running this session, then refresh.",
            );
            return status;
        }
        None => {
            status.state = BridgeState::Unavailable;
            status.say("notChecked", "Not checked yet");
            return status;
        }
    };
    status.shell_version = Some(shell.shell_version.clone());

    let supported = shell_major(&shell.shell_version)
        .is_some_and(|major| SUPPORTED_SHELL_MAJORS.contains(&major));
    let running = shell.extension.as_ref();
    status.running_version = running.and_then(|info| info.version);
    status.restart_pending = match (status.installed_version, status.running_version) {
        (Some(disk), Some(loaded)) => disk != loaded,
        _ => false,
    };
    status.update_available = status
        .installed_version
        .is_some_and(|installed| installed < BUNDLED_EXTENSION_VERSION);
    status.connected = facts.handshake.is_some();

    if !supported {
        status.state = BridgeState::Incompatible;
        status.say(
            "shellUnsupported",
            format!(
                "GNOME Shell {} is not supported by the bridge (verified on GNOME 45)",
                shell.shell_version
            ),
        );
        status.advise(
            "standardWayland",
            "Overlays still work as standard Wayland windows: click-through when locked, but the compositor decides their stacking.",
        );
        return status;
    }

    let Some(info) = running else {
        if facts.disk.is_some() {
            status.state = BridgeState::InstalledNeedsLogin;
            status.say(
                "installedNeedsLogin",
                "Installed — log out and back in once to load it",
            );
            status.advise(
                "discoverAtLogin",
                "GNOME Shell 45 discovers new extensions only at login. After logging back in, enable it here.",
            );
        } else {
            status.state = BridgeState::NotInstalled;
            status.say("notInstalled", "Not installed");
            status.advise(
                "installSteps",
                "Install the extension with the command below, log out and back in once, then enable it.",
            );
        }
        return status;
    };

    if !shell.user_extensions_enabled {
        status.state = BridgeState::ExtensionsOff;
        status.say(
            "extensionsOff",
            "User extensions are switched off for this session",
        );
        status.advise(
            "extensionsOn",
            "Turn user extensions back on (Extensions app, or gsettings set org.gnome.shell disable-user-extensions false).",
        );
        return status;
    }

    match info.state {
        ShellExtensionState::Enabled => {
            status.state = BridgeState::Active;
            status.can_disable = true;
            if status.connected {
                status.say(
                    "activeConnected",
                    "Active — keeping overlays above and delivering the shortcut",
                );
            } else {
                status.say("active", "Active");
            }
            if status.restart_pending {
                status.advise(
                    "newerCopyInstalled",
                    "A newer copy is installed. Log out and back in to load it.",
                );
            } else if status.update_available {
                status.advise(
                    "reinstallNewer",
                    "This PULSE ships a newer bridge. Reinstall it with the command below, then log out and back in.",
                );
            } else if !status.connected {
                status.advise(
                    "notGreeted",
                    "Running. It has not greeted this PULSE yet: extension versions before 2 never do; newer ones do as soon as PULSE starts.",
                );
            }
        }
        ShellExtensionState::Disabled | ShellExtensionState::Initialized => {
            status.state = BridgeState::Disabled;
            status.can_enable = true;
            status.say("disabled", "Installed but disabled");
            status.advise(
                "enableIt",
                "Enable it to keep overlays above other windows.",
            );
        }
        ShellExtensionState::Error => {
            status.state = BridgeState::Error;
            status.can_disable = true;
            status.say(
                "extensionError",
                "GNOME Shell reported an error in the extension",
            );
            status.error = Some(info.error.clone()).filter(|error| !error.is_empty());
            status.advise(
                "reinstallAfterError",
                "Reinstall it, log out and back in. The error is also in journalctl --user -b /usr/bin/gnome-shell.",
            );
        }
        ShellExtensionState::OutOfDate => {
            status.state = BridgeState::Incompatible;
            status.say(
                "outOfDate",
                format!(
                    "GNOME Shell {} considers the installed extension out of date",
                    shell.shell_version
                ),
            );
            status.advise(
                "reinstallShipped",
                "Reinstall the version this PULSE ships.",
            );
        }
        ShellExtensionState::Uninstalled => {
            status.state = BridgeState::NotInstalled;
            status.say("uninstalled", "Uninstalled");
        }
        ShellExtensionState::Enabling
        | ShellExtensionState::Disabling
        | ShellExtensionState::Downloading
        | ShellExtensionState::Unknown => {
            status.state = BridgeState::Changing;
            status.say("changing", "GNOME Shell is changing the extension's state");
            status.advise("refreshSoon", "Refresh in a moment.");
        }
    }
    status
}

/// Converts PULSE's shortcut notation (`Ctrl+Shift+F12`) to a GTK accelerator
/// (`<Control><Shift>F12`), the format of the extension's GSettings key.
/// `None` when it is not a shortcut PULSE can express.
pub fn to_gtk_accelerator(shortcut: &str) -> Option<String> {
    let parts: Vec<&str> = shortcut.split('+').map(str::trim).collect();
    let (key, modifiers) = parts.split_last()?;
    if key.is_empty() {
        return None;
    }
    let mut out = String::new();
    for modifier in modifiers {
        let gtk = match modifier.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => "<Control>",
            "shift" => "<Shift>",
            "alt" => "<Alt>",
            "super" | "meta" | "cmd" | "command" => "<Super>",
            _ => return None,
        };
        if !out.contains(gtk) {
            out.push_str(gtk);
        }
    }
    let valid_key = key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && key.len() <= 16
        && !out.is_empty();
    valid_key.then(|| format!("{out}{key}"))
}

/// Converts a GTK accelerator back to PULSE's notation, for display.
pub fn from_gtk_accelerator(accelerator: &str) -> Option<String> {
    let mut rest = accelerator.trim();
    let mut parts = Vec::new();
    while let Some(stripped) = rest.strip_prefix('<') {
        let end = stripped.find('>')?;
        let name = match stripped[..end].to_ascii_lowercase().as_str() {
            "control" | "ctrl" | "primary" => "Ctrl",
            "shift" => "Shift",
            "alt" | "mod1" => "Alt",
            "super" | "meta" => "Super",
            _ => return None,
        };
        parts.push(name.to_string());
        rest = &stripped[end + 1..];
    }
    if rest.is_empty() {
        return None;
    }
    parts.push(rest.to_string());
    Some(parts.join("+"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(state: ShellExtensionState, version: u32) -> ShellFacts {
        ShellFacts {
            shell_version: "45.10".into(),
            user_extensions_enabled: true,
            extension: Some(ShellExtensionInfo {
                state,
                version: Some(version),
                error: String::new(),
                path: None,
            }),
        }
    }

    fn disk(version: u32) -> DiskExtension {
        DiskExtension {
            path: "/home/u/.local/share/gnome-shell/extensions/pulse-overlay@jamby".into(),
            version: Some(version),
            installed_by_pulse: true,
        }
    }

    fn gnome(shell: Option<Result<ShellFacts, String>>, disk: Option<DiskExtension>) -> GnomeFacts {
        GnomeFacts {
            gnome_desktop: true,
            wayland: true,
            shell,
            disk,
            handshake: None,
        }
    }

    #[test]
    fn outside_gnome_wayland_the_bridge_does_not_apply() {
        for (gnome_desktop, wayland) in [(false, true), (true, false), (false, false)] {
            let status = bridge_status(&GnomeFacts {
                gnome_desktop,
                wayland,
                ..GnomeFacts::default()
            });
            assert_eq!(status.state, BridgeState::NotApplicable);
            assert!(!status.is_active());
        }
    }

    #[test]
    fn an_unreachable_extension_service_is_unavailable_with_its_error() {
        let status = bridge_status(&gnome(Some(Err("no such name".into())), None));
        assert_eq!(status.state, BridgeState::Unavailable);
        assert_eq!(status.error.as_deref(), Some("no such name"));
    }

    #[test]
    fn not_installed_and_installed_since_login_are_told_apart() {
        let unknown = ShellFacts {
            extension: None,
            ..shell(ShellExtensionState::Enabled, 2)
        };
        let status = bridge_status(&gnome(Some(Ok(unknown.clone())), None));
        assert_eq!(status.state, BridgeState::NotInstalled);
        let status = bridge_status(&gnome(Some(Ok(unknown)), Some(disk(2))));
        assert_eq!(status.state, BridgeState::InstalledNeedsLogin);
        assert!(status.guidance.unwrap().contains("login"));
    }

    #[test]
    fn disabled_can_be_enabled_and_active_can_be_disabled() {
        let status = bridge_status(&gnome(
            Some(Ok(shell(ShellExtensionState::Disabled, 2))),
            Some(disk(2)),
        ));
        assert_eq!(status.state, BridgeState::Disabled);
        assert!(status.can_enable && !status.can_disable);

        let status = bridge_status(&gnome(
            Some(Ok(shell(ShellExtensionState::Enabled, 2))),
            Some(disk(2)),
        ));
        assert_eq!(status.state, BridgeState::Active);
        assert!(status.is_active() && status.can_disable && !status.can_enable);
        assert!(!status.restart_pending && !status.update_available);
    }

    #[test]
    fn the_handshake_marks_the_bridge_connected() {
        let mut facts = gnome(
            Some(Ok(shell(ShellExtensionState::Enabled, 2))),
            Some(disk(2)),
        );
        assert!(!bridge_status(&facts).connected);
        facts.handshake = Some(Handshake { version: 2, at: 1 });
        let status = bridge_status(&facts);
        assert!(status.connected);
        assert!(status.guidance.is_none());
    }

    #[test]
    fn a_newer_copy_on_disk_needs_a_login_and_an_older_one_an_update() {
        // Running v1 (the prototype), v2 installed since login.
        let status = bridge_status(&gnome(
            Some(Ok(shell(ShellExtensionState::Enabled, 1))),
            Some(disk(2)),
        ));
        assert!(status.restart_pending);
        assert!(!status.update_available);
        assert_eq!(status.state, BridgeState::Active);

        // Running and installed v1: this PULSE ships v2.
        let status = bridge_status(&gnome(
            Some(Ok(shell(ShellExtensionState::Enabled, 1))),
            Some(disk(1)),
        ));
        assert!(status.update_available && !status.restart_pending);
    }

    #[test]
    fn errors_out_of_date_and_other_shells_are_reported_honestly() {
        let mut errored = shell(ShellExtensionState::Error, 2);
        errored.extension.as_mut().unwrap().error = "TypeError: x is undefined".into();
        let status = bridge_status(&gnome(Some(Ok(errored)), Some(disk(2))));
        assert_eq!(status.state, BridgeState::Error);
        assert_eq!(status.error.as_deref(), Some("TypeError: x is undefined"));

        let status = bridge_status(&gnome(
            Some(Ok(shell(ShellExtensionState::OutOfDate, 2))),
            Some(disk(2)),
        ));
        assert_eq!(status.state, BridgeState::Incompatible);

        let newer = ShellFacts {
            shell_version: "47.1".into(),
            ..shell(ShellExtensionState::Enabled, 2)
        };
        let status = bridge_status(&gnome(Some(Ok(newer)), Some(disk(2))));
        assert_eq!(status.state, BridgeState::Incompatible);
        assert!(!status.is_active());
    }

    #[test]
    fn switched_off_user_extensions_win_over_the_extension_state() {
        let off = ShellFacts {
            user_extensions_enabled: false,
            ..shell(ShellExtensionState::Enabled, 2)
        };
        let status = bridge_status(&gnome(Some(Ok(off)), Some(disk(2))));
        assert_eq!(status.state, BridgeState::ExtensionsOff);
        assert!(!status.is_active());
    }

    /// Every status the decision can produce, for the code tests below.
    fn every_status() -> Vec<GnomeBridgeStatus> {
        let mut facts = vec![
            GnomeFacts {
                wayland: true,
                ..GnomeFacts::default()
            },
            GnomeFacts::default(),
            gnome(None, None),
            gnome(Some(Err("no answer".into())), None),
            gnome(
                Some(Ok(ShellFacts {
                    shell_version: "46.0".into(),
                    ..shell(ShellExtensionState::Enabled, 2)
                })),
                Some(disk(2)),
            ),
            gnome(
                Some(Ok(ShellFacts {
                    extension: None,
                    ..shell(ShellExtensionState::Enabled, 2)
                })),
                Some(disk(2)),
            ),
            gnome(
                Some(Ok(ShellFacts {
                    extension: None,
                    ..shell(ShellExtensionState::Enabled, 2)
                })),
                None,
            ),
            gnome(
                Some(Ok(ShellFacts {
                    user_extensions_enabled: false,
                    ..shell(ShellExtensionState::Enabled, 2)
                })),
                Some(disk(2)),
            ),
            gnome(
                Some(Ok(shell(ShellExtensionState::Enabled, 1))),
                Some(disk(2)),
            ),
            gnome(
                Some(Ok(shell(ShellExtensionState::Enabled, 1))),
                Some(disk(1)),
            ),
            gnome(
                Some(Ok(shell(ShellExtensionState::Enabled, 2))),
                Some(disk(2)),
            ),
        ];
        let mut connected = gnome(
            Some(Ok(shell(ShellExtensionState::Enabled, 2))),
            Some(disk(2)),
        );
        connected.handshake = Some(Handshake { version: 2, at: 0 });
        facts.push(connected);
        for state in [
            ShellExtensionState::Disabled,
            ShellExtensionState::Initialized,
            ShellExtensionState::Error,
            ShellExtensionState::OutOfDate,
            ShellExtensionState::Uninstalled,
            ShellExtensionState::Enabling,
            ShellExtensionState::Unknown,
        ] {
            facts.push(gnome(Some(Ok(shell(state, 2))), Some(disk(2))));
        }
        facts.iter().map(bridge_status).collect()
    }

    #[test]
    fn every_summary_and_guidance_carries_a_stable_code() {
        for status in every_status() {
            assert!(!status.summary_code.is_empty(), "{:?}", status.summary);
            assert_eq!(
                status.guidance.is_some(),
                status.guidance_code.is_some(),
                "{:?}",
                status.guidance
            );
        }
    }

    /// The interface words each code through `src/i18n/locales/en.json`; a
    /// code without a translation would fall back to the English sentence.
    #[test]
    fn every_code_has_an_interface_translation() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/i18n/locales/en.json");
        let english: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("en.json")).expect("json");
        let bridge = &english["overlays"]["bridge"];
        for status in every_status() {
            assert!(
                bridge["summaries"][&status.summary_code].is_string(),
                "summary code {} has no translation",
                status.summary_code
            );
            if let Some(code) = &status.guidance_code {
                assert!(
                    bridge["guidance"][code].is_string(),
                    "guidance code {code} has no translation"
                );
            }
        }
    }

    #[test]
    fn gnome_state_codes_map_like_gnome_45() {
        assert_eq!(
            ShellExtensionState::from_code(1.0),
            ShellExtensionState::Enabled
        );
        assert_eq!(
            ShellExtensionState::from_code(2.0),
            ShellExtensionState::Disabled
        );
        assert_eq!(
            ShellExtensionState::from_code(3.0),
            ShellExtensionState::Error
        );
        assert_eq!(
            ShellExtensionState::from_code(4.0),
            ShellExtensionState::OutOfDate
        );
        assert_eq!(
            ShellExtensionState::from_code(6.0),
            ShellExtensionState::Initialized
        );
        assert_eq!(
            ShellExtensionState::from_code(42.0),
            ShellExtensionState::Unknown
        );
    }

    #[test]
    fn shortcuts_convert_to_and_from_gtk_accelerators() {
        assert_eq!(
            to_gtk_accelerator("Ctrl+Shift+F12").as_deref(),
            Some("<Control><Shift>F12")
        );
        assert_eq!(
            to_gtk_accelerator("Alt+Super+H").as_deref(),
            Some("<Alt><Super>H")
        );
        assert_eq!(to_gtk_accelerator("F12"), None, "a bare key is refused");
        assert_eq!(to_gtk_accelerator("Ctrl+Hyper+X"), None);
        assert_eq!(to_gtk_accelerator("Ctrl+<script>"), None);
        assert_eq!(
            from_gtk_accelerator("<Control><Shift>F12").as_deref(),
            Some("Ctrl+Shift+F12")
        );
        assert_eq!(
            from_gtk_accelerator("<Primary>q").as_deref(),
            Some("Ctrl+q")
        );
        assert_eq!(from_gtk_accelerator("<Bogus>x"), None);
        assert_eq!(from_gtk_accelerator("<Control>"), None);
    }
}
