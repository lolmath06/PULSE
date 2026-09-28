//! Desktop settings the backend acts on: what closing the main window does,
//! and the global shortcut.

use serde_json::Value;

/// The default global shortcut: toggles every overlay between Edit and Locked.
pub const DEFAULT_HOTKEY: &str = "Ctrl+Shift+F12";

/// What closing the main window does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseBehavior {
    /// Quit PULSE: overlays close, schedulers stop, SQLite is checkpointed.
    /// The default — nothing keeps running unless the user asked for it.
    Quit,
    /// Hide the main window while at least one overlay is visible; PULSE keeps
    /// running and is reopened from the tray or an overlay's *Open PULSE*.
    KeepRunningWithOverlays,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopSettings {
    pub close_behavior: CloseBehavior,
    /// `None` disables the shortcut.
    pub hotkey: Option<String>,
}

impl Default for DesktopSettings {
    fn default() -> Self {
        Self {
            close_behavior: CloseBehavior::Quit,
            hotkey: Some(DEFAULT_HOTKEY.to_string()),
        }
    }
}

/// Reads the `settings` section; anything missing or invalid is the default.
pub fn parse_settings(section: Option<&Value>) -> DesktopSettings {
    let defaults = DesktopSettings::default();
    let Some(section) = section else {
        return defaults;
    };
    let close_behavior = match section.get("closeBehavior").and_then(Value::as_str) {
        Some("keep-running") => CloseBehavior::KeepRunningWithOverlays,
        _ => CloseBehavior::Quit,
    };
    let hotkey = match section.get("overlayHotkey") {
        Some(Value::Null) => None,
        Some(Value::String(text)) if !text.trim().is_empty() && text.len() <= 64 => {
            Some(text.trim().to_string())
        }
        _ => defaults.hotkey,
    };
    DesktopSettings {
        close_behavior,
        hotkey,
    }
}

/// Registers and unregisters one global shortcut; implemented by the Tauri
/// plugin in the app and by a fake in tests.
pub trait ShortcutRegistrar {
    /// Registers `shortcut`. `Err` carries why: invalid, or taken by another
    /// application.
    fn register(&mut self, shortcut: &str) -> Result<(), String>;
    fn unregister(&mut self, shortcut: &str);
}

/// The shortcut currently in force, and how changing it behaves.
///
/// Changing the shortcut registers the **new** one first; only if that
/// succeeds is the old one released. A conflict therefore leaves the previous
/// shortcut working, and PULSE never claims to hold a shortcut it does not.
#[derive(Debug, Default)]
pub struct HotkeyManager {
    current: Option<String>,
    last_error: Option<String>,
}

impl HotkeyManager {
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Makes `wanted` the shortcut (`None` disables it).
    pub fn apply(
        &mut self,
        registrar: &mut dyn ShortcutRegistrar,
        wanted: Option<&str>,
    ) -> Result<(), String> {
        if wanted == self.current.as_deref() {
            return Ok(());
        }
        match wanted {
            None => {
                if let Some(old) = self.current.take() {
                    registrar.unregister(&old);
                }
                self.last_error = None;
                Ok(())
            }
            Some(next) => match registrar.register(next) {
                Ok(()) => {
                    if let Some(old) = self.current.replace(next.to_string()) {
                        registrar.unregister(&old);
                    }
                    self.last_error = None;
                    Ok(())
                }
                Err(error) => {
                    let message = format!("{next}: {error}");
                    self.last_error = Some(message.clone());
                    Err(message)
                }
            },
        }
    }

    /// Whether a pressed shortcut is ours.
    pub fn matches(&self, pressed: &str) -> bool {
        self.current
            .as_deref()
            .is_some_and(|current| current.eq_ignore_ascii_case(pressed))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[derive(Default)]
    struct Fake {
        held: Vec<String>,
        taken: Vec<String>,
    }

    impl ShortcutRegistrar for Fake {
        fn register(&mut self, shortcut: &str) -> Result<(), String> {
            if self.taken.iter().any(|t| t == shortcut) {
                return Err("already registered by another application".into());
            }
            if !shortcut.contains('+') {
                return Err("not a valid shortcut".into());
            }
            self.held.push(shortcut.to_string());
            Ok(())
        }
        fn unregister(&mut self, shortcut: &str) {
            self.held.retain(|held| held != shortcut);
        }
    }

    #[test]
    fn settings_default_to_quit_and_ctrl_shift_f12() {
        let settings = parse_settings(None);
        assert_eq!(settings.close_behavior, CloseBehavior::Quit);
        assert_eq!(settings.hotkey.as_deref(), Some(DEFAULT_HOTKEY));
        let custom = parse_settings(Some(
            &json!({ "closeBehavior": "keep-running", "overlayHotkey": null }),
        ));
        assert_eq!(
            custom.close_behavior,
            CloseBehavior::KeepRunningWithOverlays
        );
        assert_eq!(custom.hotkey, None);
        let junk = parse_settings(Some(
            &json!({ "closeBehavior": 3, "overlayHotkey": "x".repeat(100) }),
        ));
        assert_eq!(junk, DesktopSettings::default());
    }

    #[test]
    fn register_change_and_unregister() {
        let mut fake = Fake::default();
        let mut manager = HotkeyManager::default();
        manager
            .apply(&mut fake, Some("Ctrl+Shift+F12"))
            .expect("registers");
        assert_eq!(fake.held, ["Ctrl+Shift+F12"]);
        assert!(manager.matches("ctrl+shift+f12"));

        manager.apply(&mut fake, Some("Alt+F10")).expect("changes");
        assert_eq!(fake.held, ["Alt+F10"], "the old one is released only after");
        manager.apply(&mut fake, None).expect("disables");
        assert!(fake.held.is_empty());
        assert_eq!(manager.current(), None);
    }

    #[test]
    fn a_conflict_keeps_the_previous_shortcut_and_says_so() {
        let mut fake = Fake {
            taken: vec!["Ctrl+Alt+T".into()],
            ..Fake::default()
        };
        let mut manager = HotkeyManager::default();
        manager
            .apply(&mut fake, Some("Ctrl+Shift+F12"))
            .expect("registers");

        let error = manager
            .apply(&mut fake, Some("Ctrl+Alt+T"))
            .expect_err("conflict");
        assert!(error.contains("already registered"));
        assert_eq!(manager.current(), Some("Ctrl+Shift+F12"), "not claimed");
        assert_eq!(
            fake.held,
            ["Ctrl+Shift+F12"],
            "the previous one still works"
        );
        assert!(manager.last_error().is_some());
    }

    #[test]
    fn an_invalid_shortcut_is_refused() {
        let mut manager = HotkeyManager::default();
        assert!(manager.apply(&mut Fake::default(), Some("banana")).is_err());
        assert_eq!(manager.current(), None);
    }

    #[test]
    fn a_restart_registers_the_saved_shortcut() {
        let saved = parse_settings(Some(&json!({ "overlayHotkey": "Alt+Shift+O" })));
        let mut fake = Fake::default();
        let mut manager = HotkeyManager::default();
        manager
            .apply(&mut fake, saved.hotkey.as_deref())
            .expect("restored");
        assert_eq!(manager.current(), Some("Alt+Shift+O"));
    }
}
