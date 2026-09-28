//! The shared UI configuration: dashboards, widgets, overlays, visual presets
//! and settings, in **one** file owned by the backend.
//!
//! Phase 10 kept visual preferences in each webview's `localStorage`. That
//! stopped being good enough the moment PULSE had more than one window: an
//! overlay, the Mini window and the main window each have their own webview,
//! and a development build and a release build have different origins, so
//! each would have seen a different configuration. Here every window reads
//! and writes the same document through the backend, and every change is
//! broadcast to the other windows.
//!
//! ```text
//!  main window ─┐                       ┌─► ui-config-changed ─► every window
//!  overlay ─────┼─ set_ui_config_section ┤
//!  mini ────────┘   (validated)          └─► ConfigWriter ─► ui-config.json
//!                                            (coalesced, atomic, with .bak)
//! ```
//!
//! # Properties
//!
//! - **Versioned.** The document carries `"version": 1`. A newer file is never
//!   overwritten: PULSE runs on defaults in memory and says why.
//! - **Atomic.** A save writes `ui-config.json.tmp`, flushes it to disk, keeps
//!   the previous file as `ui-config.json.bak`, and renames the temporary file
//!   over the real one. A power cut leaves either the old or the new file.
//! - **Corruption-tolerant.** An unreadable file is moved aside as
//!   `ui-config.corrupt-<ms>.json` (never deleted), the backup is tried, and
//!   failing that PULSE starts from defaults. It always starts.
//! - **Bounded.** Only known top-level sections, each a JSON object, 4 MiB in
//!   total at most.
//!
//! The backend validates the *shape* of the document. The frontend normalises
//! the *contents* of each section field by field, exactly as Phase 10 did for a
//! visualization — so an unknown property or an invalid colour never crashes a
//! window. The overlay section is additionally read by the backend, which owns
//! the overlay windows (see `crate::overlay`).
//!
//! Tauri-free: the Windows harness type checks all of it.

pub mod writer;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The document version this build reads and writes.
pub const UI_CONFIG_VERSION: u64 = 1;

/// The file name inside the application's config directory.
pub const FILE_NAME: &str = "ui-config.json";

/// A document larger than this is refused rather than written.
pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

/// Every top-level section a window may write.
pub const SECTIONS: &[&str] = &[
    "visualization",
    "dashboards",
    "overlays",
    "templates",
    "settings",
];

/// Where the configuration lives, given the platform's config directory.
///
/// `$XDG_CONFIG_HOME/dev.pulse.app` on Fedora, `%APPDATA%\dev.pulse.app` on
/// Windows — resolved by Tauri, never guessed.
pub fn config_path(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE_NAME)
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// What happened when the file was read at startup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum LoadOutcome {
    /// No file yet: defaults.
    Fresh,
    /// The file was read.
    Loaded,
    /// The file was unreadable; the previous good copy was used.
    RecoveredFromBackup {
        reason: String,
        quarantined: Option<String>,
    },
    /// Neither the file nor its backup was usable: defaults.
    Reset {
        reason: String,
        quarantined: Option<String>,
    },
    /// Written by a newer PULSE. Left untouched; defaults in memory only.
    NewerVersion { found: u64 },
}

/// Why a change was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiConfigError {
    UnknownSection(String),
    NotAnObject(String),
    TooLarge { bytes: usize },
    ReadOnly(String),
    Io(String),
}

impl std::fmt::Display for UiConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiConfigError::UnknownSection(name) => {
                write!(f, "unknown configuration section '{name}'")
            }
            UiConfigError::NotAnObject(name) => write!(f, "section '{name}' must be a JSON object"),
            UiConfigError::TooLarge { bytes } => write!(
                f,
                "the configuration would be {bytes} bytes; at most {MAX_DOCUMENT_BYTES} are allowed"
            ),
            UiConfigError::ReadOnly(reason) => write!(f, "configuration is read-only: {reason}"),
            UiConfigError::Io(message) => write!(f, "could not save the configuration: {message}"),
        }
    }
}

impl std::error::Error for UiConfigError {}

/// The document as the frontend receives it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiConfigSnapshot {
    pub revision: u64,
    pub document: Value,
    pub load: LoadOutcome,
    /// Why saving is disabled, when it is.
    pub read_only: Option<String>,
    pub path: String,
}

struct Inner {
    document: Map<String, Value>,
    revision: u64,
    dirty: bool,
    read_only: Option<String>,
}

/// The configuration file and its in-memory copy.
///
/// Memory is authoritative while PULSE runs; the file follows through
/// [`writer::ConfigWriter`], which coalesces bursts (a colour picker being
/// dragged) into one atomic write.
pub struct UiConfigStore {
    path: PathBuf,
    load: LoadOutcome,
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for UiConfigStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiConfigStore")
            .field("path", &self.path)
            .field("load", &self.load)
            .finish()
    }
}

fn lock(mutex: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn empty_document() -> Map<String, Value> {
    let mut document = Map::new();
    document.insert("version".into(), Value::from(UI_CONFIG_VERSION));
    document
}

/// Parses a file's bytes. `Err` carries a human reason.
fn parse(bytes: &[u8]) -> Result<Parsed, String> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(format!(
            "the file is {} bytes, larger than allowed",
            bytes.len()
        ));
    }
    let value: Value =
        serde_json::from_slice(bytes).map_err(|error| format!("invalid JSON: {error}"))?;
    let Value::Object(mut object) = value else {
        return Err("the document is not a JSON object".into());
    };
    let version = object
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| "the document has no numeric version".to_string())?;
    if version > UI_CONFIG_VERSION {
        return Ok(Parsed::Newer(version));
    }
    // Keep only known sections, and only objects. Anything else is dropped
    // here rather than carried forward forever.
    object.retain(|key, value| {
        key == "version" || (SECTIONS.contains(&key.as_str()) && value.is_object())
    });
    object.insert("version".into(), Value::from(UI_CONFIG_VERSION));
    Ok(Parsed::Document(object))
}

enum Parsed {
    Document(Map<String, Value>),
    Newer(u64),
}

/// Writes `bytes` to `path` atomically, keeping the previous file as `.bak`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = sibling(path, ".tmp");
    {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if path.exists() {
        // A copy, not a rename: the real file must exist at every instant.
        std::fs::copy(path, sibling(path, ".bak"))?;
    }
    // Atomic on POSIX; `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` on Windows.
    std::fs::rename(&temporary, path)?;
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(directory) = std::fs::File::open(parent) {
            let _ = directory.sync_all();
        }
    }
    Ok(())
}

fn quarantine(path: &Path, now_ms: u128) -> Option<String> {
    let target = path.with_file_name(format!("ui-config.corrupt-{now_ms}.json"));
    std::fs::rename(path, &target)
        .ok()
        .map(|()| target.display().to_string())
}

impl UiConfigStore {
    /// Opens the configuration at `path`. **Never fails**: the worst case is
    /// defaults in memory, with [`LoadOutcome`] saying why.
    pub fn open(path: &Path) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis())
            .unwrap_or(0);
        let backup = sibling(path, ".bak");

        let (document, load, read_only) = match std::fs::read(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (empty_document(), LoadOutcome::Fresh, None)
            }
            Err(error) => (
                empty_document(),
                LoadOutcome::Reset {
                    reason: format!("the file could not be read: {error}"),
                    quarantined: None,
                },
                // Unreadable (permissions?): do not overwrite what we cannot see.
                Some(format!("{} could not be read: {error}", path.display())),
            ),
            Ok(bytes) => match parse(&bytes) {
                Ok(Parsed::Document(document)) => (document, LoadOutcome::Loaded, None),
                Ok(Parsed::Newer(found)) => (
                    empty_document(),
                    LoadOutcome::NewerVersion { found },
                    Some(format!(
                        "the configuration was written by a newer PULSE (version {found}); it is left untouched"
                    )),
                ),
                Err(reason) => {
                    let quarantined = quarantine(path, now_ms);
                    match std::fs::read(&backup).ok().map(|bytes| parse(&bytes)) {
                        Some(Ok(Parsed::Document(document))) => (
                            document,
                            LoadOutcome::RecoveredFromBackup { reason, quarantined },
                            None,
                        ),
                        _ => (
                            empty_document(),
                            LoadOutcome::Reset { reason, quarantined },
                            None,
                        ),
                    }
                }
            },
        };

        Self {
            path: path.to_path_buf(),
            load,
            inner: Mutex::new(Inner {
                document,
                revision: 1,
                dirty: false,
                read_only,
            }),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_outcome(&self) -> &LoadOutcome {
        &self.load
    }

    pub fn snapshot(&self) -> UiConfigSnapshot {
        let inner = lock(&self.inner);
        UiConfigSnapshot {
            revision: inner.revision,
            document: Value::Object(inner.document.clone()),
            load: self.load.clone(),
            read_only: inner.read_only.clone(),
            path: self.path.display().to_string(),
        }
    }

    /// One section's current value, if set.
    pub fn section(&self, name: &str) -> Option<Value> {
        lock(&self.inner).document.get(name).cloned()
    }

    pub fn revision(&self) -> u64 {
        lock(&self.inner).revision
    }

    /// Replaces a section. Returns the new revision.
    ///
    /// The change is in memory immediately and reaches the file at the next
    /// [`flush`](Self::flush). While the store is read-only, changes still
    /// apply for this session but are never written.
    pub fn set_section(&self, name: &str, value: Value) -> Result<u64, UiConfigError> {
        if !SECTIONS.contains(&name) {
            return Err(UiConfigError::UnknownSection(name.to_string()));
        }
        if !value.is_object() {
            return Err(UiConfigError::NotAnObject(name.to_string()));
        }
        let mut inner = lock(&self.inner);
        let previous = inner.document.insert(name.to_string(), value);
        let bytes = serde_json::to_vec(&inner.document)
            .map(|v| v.len())
            .unwrap_or(usize::MAX);
        if bytes > MAX_DOCUMENT_BYTES {
            match previous {
                Some(previous) => inner.document.insert(name.to_string(), previous),
                None => inner.document.remove(name),
            };
            return Err(UiConfigError::TooLarge { bytes });
        }
        inner.revision += 1;
        inner.dirty = true;
        Ok(inner.revision)
    }

    /// Changes a section in place — for backend-owned updates such as an
    /// overlay's position or the global hotkey toggling every overlay's lock.
    pub fn update_section(
        &self,
        name: &str,
        change: impl FnOnce(&mut Value),
    ) -> Result<u64, UiConfigError> {
        let mut value = self
            .section(name)
            .unwrap_or_else(|| Value::Object(Map::new()));
        change(&mut value);
        self.set_section(name, value)
    }

    pub fn is_dirty(&self) -> bool {
        lock(&self.inner).dirty
    }

    /// Writes the document if it changed. `Ok(true)` when a write happened.
    pub fn flush(&self) -> Result<bool, UiConfigError> {
        let (bytes, revision) = {
            let inner = lock(&self.inner);
            if !inner.dirty {
                return Ok(false);
            }
            if let Some(reason) = &inner.read_only {
                return Err(UiConfigError::ReadOnly(reason.clone()));
            }
            let bytes = serde_json::to_vec_pretty(&inner.document)
                .map_err(|error| UiConfigError::Io(error.to_string()))?;
            (bytes, inner.revision)
        };

        write_atomic(&self.path, &bytes).map_err(|error| UiConfigError::Io(error.to_string()))?;

        let mut inner = lock(&self.inner);
        // A change that landed while writing keeps the store dirty.
        if inner.revision == revision {
            inner.dirty = false;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
