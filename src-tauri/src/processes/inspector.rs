//! [`ProcessInspectorService`] — everything PULSE can say about **one**
//! process, read only when the user selects it.
//!
//! # Lazy by construction
//!
//! The snapshot service answers "what is running" for several hundred
//! processes in a few milliseconds, and must stay that fast. Everything here
//! is either more expensive or only interesting for one process at a time:
//!
//! | Datum | Cost | When |
//! |---|---|---|
//! | owner, start time, architecture, priority, affinity | a few syscalls | inspector opened |
//! | Windows version resource | one file read | inspector opened |
//! | Fedora package / Windows signature | `rpm -qf` / `WinVerifyTrust` | inspector opened, separate call |
//! | SHA-256 | the whole executable | only on click |
//!
//! None of it is ever computed on Refresh.
//!
//! # Provenance, not a verdict
//!
//! A package owner, a signature and a hash are **facts about where a file came
//! from**. None of them means "safe" and their absence does not mean
//! "malware"; PULSE shows them and draws no conclusion. See
//! `docs/processes/provenance.md`.
//!
//! # Identity, again
//!
//! Every query names a [`ProcessInstanceId`] and every answer is checked
//! against it. A details read whose start token no longer matches is
//! reported as stale and its data discarded — it described another process.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::metrics::model::{Availability, TimestampMs};

use super::action::{ProcessActionResult, ProcessQuery};
use super::control::{
    ControlError, PriorityKind, ProcessAffinity, ProcessPriority, SuspensionLedger,
};
use super::field::Field;
use super::hash::{sha256_stream, FileHash};
use super::identity::ProcessInstanceId;
use super::state::{ProcessClass, ProcessState};

/// The executable file behind a process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableInfo {
    pub path: String,
    pub file_name: String,
    pub size_bytes: Field<u64>,
    pub modified_at: Field<TimestampMs>,
    /// Linux: the file the process was started from has since been deleted
    /// or replaced on disk (`/proc/<pid>/exe` reads `… (deleted)`).
    pub replaced_on_disk: bool,
}

/// Who a process runs as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessOwner {
    /// Linux UID or Windows SID string — the identifier the OS trusts.
    pub id: String,
    /// The resolved account name, when it resolves.
    pub name: Option<String>,
}

/// A Windows PE version resource. Descriptive, **not** evidence: any program
/// can claim any product name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub file_description: Option<String>,
    pub product_name: Option<String>,
    pub company_name: Option<String>,
    pub file_version: Option<String>,
    pub product_version: Option<String>,
}

impl VersionInfo {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// Whether one action is offered, and if not, why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl Capability {
    pub fn allowed() -> Self {
        Self {
            allowed: true,
            reason: None,
        }
    }

    pub fn denied(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
        }
    }
}

/// What the platform itself reports the user may do to a process — before
/// PULSE's own session rules (suspension ledger, self-protection) apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformAccess {
    pub terminate: Capability,
    pub suspend: Capability,
    pub set_priority: Capability,
    pub set_affinity: Capability,
}

impl PlatformAccess {
    pub fn all(capability: Capability) -> Self {
        Self {
            terminate: capability.clone(),
            suspend: capability.clone(),
            set_priority: capability.clone(),
            set_affinity: capability,
        }
    }
}

/// Every action the inspector and context menu offer, each with its reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessCapabilities {
    pub terminate: Capability,
    pub terminate_tree: Capability,
    pub force_kill: Capability,
    pub suspend: Capability,
    pub resume: Capability,
    pub set_priority: Capability,
    pub set_affinity: Capability,
    pub open_location: Capability,
    pub compute_hash: Capability,
}

/// What a platform read about one process.
#[derive(Debug, Clone, PartialEq)]
pub struct RawDetails {
    /// Read in the same pass as everything else, for the identity check.
    pub start_token: u64,
    pub parent_pid: Option<u32>,
    pub parent_name: Field<String>,
    pub name: String,
    pub state: ProcessState,
    pub state_availability: Availability,
    pub category: ProcessClass,
    pub started_at: Field<TimestampMs>,
    pub executable: Field<ExecutableInfo>,
    pub owner: Field<ProcessOwner>,
    pub architecture: Field<String>,
    pub priority: Field<ProcessPriority>,
    pub affinity: Field<ProcessAffinity>,
    pub version_info: Field<VersionInfo>,
    pub access: PlatformAccess,
}

/// The inspector's answer about one process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessDetails {
    pub instance_id: String,
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub parent_name: Field<String>,
    pub name: String,
    pub state: ProcessState,
    pub state_availability: Availability,
    pub category: ProcessClass,
    /// Wall-clock start, Unix epoch milliseconds. The identity token is kept
    /// separately, inside `instance_id`, and is never shown as a date.
    pub started_at: Field<TimestampMs>,
    pub executable: Field<ExecutableInfo>,
    pub owner: Field<ProcessOwner>,
    pub architecture: Field<String>,
    pub priority: Field<ProcessPriority>,
    pub affinity: Field<ProcessAffinity>,
    pub version_info: Field<VersionInfo>,
    pub capabilities: ProcessCapabilities,
    /// This is PULSE itself.
    pub is_self: bool,
    /// PULSE suspended this instance during this session.
    pub suspended_by_pulse: bool,
    /// Which priority vocabulary this platform speaks, so the interface can
    /// offer the right presets even when the current value was refused.
    pub priority_kind: Option<PriorityVocabulary>,
    /// Whether this platform has a separate *Force kill* (Linux `SIGKILL`).
    pub force_kill_supported: bool,
}

/// [`PriorityKind`] on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PriorityVocabulary {
    Nice,
    WindowsClass,
}

impl From<PriorityKind> for PriorityVocabulary {
    fn from(kind: PriorityKind) -> Self {
        match kind {
            PriorityKind::Nice => PriorityVocabulary::Nice,
            PriorityKind::WindowsClass => PriorityVocabulary::WindowsClass,
        }
    }
}

/// What the control side of this platform supports, told to the inspector
/// once at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ControlSupport {
    pub priority_kind: Option<PriorityKind>,
    pub force_kill: bool,
}

/// One RPM package that owns a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageRef {
    pub name: String,
    /// `version-release`, e.g. `5.2.26-1.fc39`.
    pub version: String,
    pub arch: String,
}

/// Where an unpackaged executable lives, as a hint for the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LocationHint {
    /// Under the user's home directory.
    UserHome,
    /// `/usr/local`, `/opt` — installed outside the package manager.
    LocalInstall,
    /// A system path that no package claims.
    Other,
}

/// What `WinVerifyTrust` concluded about a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrustStatus {
    /// A valid signature chaining to a trusted root.
    Trusted,
    /// A structurally valid signature Windows does not trust (untrusted root,
    /// expired, explicitly distrusted…).
    SignedButUntrusted,
    /// A signature that does not match the file.
    Invalid,
    Unsigned,
    Unavailable,
    PermissionDenied,
}

/// Where a Windows signature was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SignatureSource {
    /// Embedded in the PE file.
    Embedded,
    /// In a system security catalog (how most of Windows itself is signed).
    Catalog,
}

/// A Windows Authenticode verdict and the signer it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInfo {
    pub trust: TrustStatus,
    pub source: Option<SignatureSource>,
    /// The signing certificate's subject display name. For an untrusted
    /// signature, a *claim*.
    pub publisher: Option<String>,
    pub detail: String,
}

/// Where an executable comes from. Evidence, never a verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Provenance {
    /// Fedora: owned by one or more installed RPM packages.
    #[serde(rename_all = "camelCase")]
    RpmPackage { packages: Vec<PackageRef> },
    /// Fedora: no installed package owns the file.
    #[serde(rename_all = "camelCase")]
    NotPackaged { location: LocationHint },
    /// Windows: an Authenticode verdict.
    #[serde(rename_all = "camelCase")]
    Signature { signature: SignatureInfo },
    /// No provenance could be established, and why.
    #[serde(rename_all = "camelCase")]
    Unavailable { reason: String },
}

/// An executable opened for hashing, with the start token observed after
/// opening it.
pub struct OpenedExecutable {
    pub file: std::fs::File,
    pub start_token: u64,
}

/// What a platform provides so PULSE can inspect one process.
pub trait ProcessInspectorBackend: Send + Sync + std::fmt::Debug {
    /// Reads everything cheap about `pid`.
    fn inspect(&self, pid: u32) -> Result<RawDetails, ControlError>;

    /// Establishes provenance for `pid`'s executable. Possibly slow.
    fn provenance(&self, pid: u32) -> Result<(u64, Provenance), ControlError>;

    /// Opens the executable `pid` is running — on Linux the mapped image
    /// through `/proc/<pid>/exe`, which is the running file even if the path
    /// has since been replaced.
    fn open_executable(&self, pid: u32) -> Result<OpenedExecutable, ControlError>;

    /// The executable path, for *Open file location*.
    fn executable_path(&self, pid: u32) -> Result<(u64, String), ControlError>;
}

/// Reads one process in depth.
#[derive(Debug)]
pub struct ProcessInspectorService {
    backend: Option<Arc<dyn ProcessInspectorBackend>>,
    ledger: Arc<SuspensionLedger>,
    self_pid: u32,
    support: ControlSupport,
}

impl ProcessInspectorService {
    pub fn new(
        backend: Option<Arc<dyn ProcessInspectorBackend>>,
        ledger: Arc<SuspensionLedger>,
        self_pid: u32,
        support: ControlSupport,
    ) -> Self {
        Self {
            backend,
            ledger,
            self_pid,
            support,
        }
    }

    fn backend(&self) -> Result<&Arc<dyn ProcessInspectorBackend>, ProcessActionResult> {
        self.backend.as_ref().ok_or_else(|| {
            ProcessActionResult::unsupported("PULSE cannot inspect processes on this platform.")
        })
    }

    /// Everything cheap about exactly this instance.
    pub fn details(&self, id: ProcessInstanceId) -> ProcessQuery<ProcessDetails> {
        let backend = match self.backend() {
            Ok(backend) => backend,
            Err(result) => return ProcessQuery::failed(result),
        };

        let raw = match backend.inspect(id.pid) {
            Ok(raw) => raw,
            Err(error) => return ProcessQuery::failed(error.into_result(id.pid)),
        };

        if raw.start_token != id.start_token {
            return ProcessQuery::failed(ProcessActionResult::stale(id.pid));
        }

        let suspended_by_pulse = self.ledger.contains(&id);
        let is_self = id.pid == self.self_pid;
        let capabilities = capabilities(&raw, is_self, suspended_by_pulse, self.support.force_kill);

        ProcessQuery::ok(ProcessDetails {
            instance_id: id.canonical_string(),
            pid: id.pid,
            parent_pid: raw.parent_pid,
            parent_name: raw.parent_name,
            name: raw.name,
            state: raw.state,
            state_availability: raw.state_availability,
            category: raw.category,
            started_at: raw.started_at,
            executable: raw.executable,
            owner: raw.owner,
            architecture: raw.architecture,
            priority: raw.priority,
            affinity: raw.affinity,
            version_info: raw.version_info,
            capabilities,
            is_self,
            suspended_by_pulse,
            priority_kind: self.support.priority_kind.map(PriorityVocabulary::from),
            force_kill_supported: self.support.force_kill,
        })
    }

    /// The package or signature behind exactly this instance's executable.
    pub fn provenance(&self, id: ProcessInstanceId) -> ProcessQuery<Provenance> {
        let backend = match self.backend() {
            Ok(backend) => backend,
            Err(result) => return ProcessQuery::failed(result),
        };

        match backend.provenance(id.pid) {
            Ok((token, _)) if token != id.start_token => {
                ProcessQuery::failed(ProcessActionResult::stale(id.pid))
            }
            Ok((_, provenance)) => ProcessQuery::ok(provenance),
            Err(error) => ProcessQuery::failed(error.into_result(id.pid)),
        }
    }

    /// SHA-256 of exactly this instance's executable.
    ///
    /// The backend reads the start token on both sides of opening the file,
    /// so the file handle provably belongs to the selected instance; the
    /// read itself is then guarded against the file changing underneath.
    pub fn sha256(&self, id: ProcessInstanceId) -> ProcessQuery<FileHash> {
        let backend = match self.backend() {
            Ok(backend) => backend,
            Err(result) => return ProcessQuery::failed(result),
        };

        let mut opened = match backend.open_executable(id.pid) {
            Ok(opened) => opened,
            Err(error) => return ProcessQuery::failed(error.into_result(id.pid)),
        };

        if opened.start_token != id.start_token {
            return ProcessQuery::failed(ProcessActionResult::stale(id.pid));
        }

        let hash = sha256_stream(&mut opened.file);
        ProcessQuery::ok(hash)
    }

    /// The executable path of exactly this instance, re-resolved now.
    pub fn executable_path(&self, id: ProcessInstanceId) -> ProcessQuery<String> {
        let backend = match self.backend() {
            Ok(backend) => backend,
            Err(result) => return ProcessQuery::failed(result),
        };

        match backend.executable_path(id.pid) {
            Ok((token, _)) if token != id.start_token => {
                ProcessQuery::failed(ProcessActionResult::stale(id.pid))
            }
            Ok((_, path)) => ProcessQuery::ok(path),
            Err(error) => ProcessQuery::failed(error.into_result(id.pid)),
        }
    }
}

/// Combines what the platform allows with PULSE's own session rules.
///
/// Pure, so every rule is unit-tested: a zombie cannot be controlled, a
/// kernel thread is never offered an action, *Resume* exists only for what
/// PULSE suspended, and PULSE never offers to suspend itself.
pub fn capabilities(
    raw: &RawDetails,
    is_self: bool,
    suspended_by_pulse: bool,
    force_kill_supported: bool,
) -> ProcessCapabilities {
    let exited = raw.state == ProcessState::Zombie;
    let kernel = raw.category == ProcessClass::KernelThread;

    let gate = |platform: &Capability| -> Capability {
        if exited {
            Capability::denied("Process already exited (zombie awaiting its parent).")
        } else if kernel {
            Capability::denied("Kernel thread — PULSE does not control kernel threads.")
        } else {
            platform.clone()
        }
    };

    let terminate = gate(&raw.access.terminate);

    let suspend = if is_self {
        Capability::denied("Suspending PULSE would freeze the window needed to resume it.")
    } else if suspended_by_pulse {
        Capability::denied("Already suspended by PULSE.")
    } else if raw.state == ProcessState::Stopped {
        Capability::denied("Already stopped by something other than PULSE.")
    } else {
        gate(&raw.access.suspend)
    };

    let resume = if suspended_by_pulse {
        gate(&raw.access.suspend)
    } else {
        Capability::denied("PULSE did not suspend this process.")
    };

    let force_kill = if force_kill_supported {
        terminate.clone()
    } else {
        Capability::denied("End process already terminates immediately on this platform.")
    };

    let file = match &raw.executable {
        Field {
            value: Some(executable),
            ..
        } if executable.replaced_on_disk => Capability::denied(
            "The executable was deleted or replaced on disk after this process started.",
        ),
        Field { value: Some(_), .. } => Capability::allowed(),
        Field { availability, .. } => Capability::denied(match availability {
            Availability::PermissionDenied { .. } => "Executable unavailable: permission denied.",
            _ if kernel => "Kernel threads have no executable.",
            _ => "Executable unavailable.",
        }),
    };

    // The running image can be hashed through `/proc/<pid>/exe` even when
    // the path on disk was replaced — that is exactly when a hash matters.
    let compute_hash = match &raw.executable {
        Field { value: Some(_), .. } => Capability::allowed(),
        _ => file.clone(),
    };

    ProcessCapabilities {
        terminate_tree: terminate.clone(),
        terminate,
        force_kill,
        suspend,
        resume,
        set_priority: gate(&raw.access.set_priority),
        set_affinity: gate(&raw.access.set_affinity),
        open_location: file,
        compute_hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processes::control::ProcessAffinity;
    use std::sync::Mutex;

    fn executable(path: &str) -> ExecutableInfo {
        ExecutableInfo {
            path: path.to_string(),
            file_name: path.rsplit('/').next().unwrap_or(path).to_string(),
            size_bytes: Field::available(1_000),
            modified_at: Field::available(1_700_000_000_000),
            replaced_on_disk: false,
        }
    }

    fn raw(token: u64) -> RawDetails {
        RawDetails {
            start_token: token,
            parent_pid: Some(1),
            parent_name: Field::available("systemd".to_string()),
            name: "bash".to_string(),
            state: ProcessState::SleepingOrWaiting,
            state_availability: Availability::Available,
            category: ProcessClass::UserApplication,
            started_at: Field::available(1_700_000_000_000),
            executable: Field::available(executable("/usr/bin/bash")),
            owner: Field::available(ProcessOwner {
                id: "1000".to_string(),
                name: Some("alice".to_string()),
            }),
            architecture: Field::available("x86_64".to_string()),
            priority: Field::available(ProcessPriority::Nice { value: 0 }),
            affinity: Field::available(ProcessAffinity {
                cpus: vec![0, 1],
                available: vec![0, 1],
                limitation: None,
            }),
            version_info: Field::missing(Availability::unsupported("Linux")),
            access: PlatformAccess::all(Capability::allowed()),
        }
    }

    #[derive(Debug)]
    struct Fake {
        raw: Mutex<Result<RawDetails, ControlError>>,
    }

    impl ProcessInspectorBackend for Fake {
        fn inspect(&self, _pid: u32) -> Result<RawDetails, ControlError> {
            self.raw.lock().unwrap().clone()
        }
        fn provenance(&self, _pid: u32) -> Result<(u64, Provenance), ControlError> {
            let raw = self.raw.lock().unwrap().clone()?;
            Ok((
                raw.start_token,
                Provenance::RpmPackage {
                    packages: vec![PackageRef {
                        name: "bash".into(),
                        version: "5.2.26-1.fc39".into(),
                        arch: "x86_64".into(),
                    }],
                },
            ))
        }
        fn open_executable(&self, _pid: u32) -> Result<OpenedExecutable, ControlError> {
            Err(ControlError::PermissionDenied("denied".into()))
        }
        fn executable_path(&self, _pid: u32) -> Result<(u64, String), ControlError> {
            let raw = self.raw.lock().unwrap().clone()?;
            Ok((raw.start_token, "/usr/bin/bash".into()))
        }
    }

    fn service(result: Result<RawDetails, ControlError>) -> ProcessInspectorService {
        ProcessInspectorService::new(
            Some(Arc::new(Fake {
                raw: Mutex::new(result),
            })),
            Arc::new(SuspensionLedger::default()),
            4242,
            ControlSupport {
                priority_kind: Some(PriorityKind::Nice),
                force_kill: true,
            },
        )
    }

    #[test]
    fn details_of_the_selected_instance_are_returned() {
        let query = service(Ok(raw(7))).details(ProcessInstanceId::new(55, 7));
        let details = query.value.expect("details");

        assert_eq!(details.instance_id, "process:55-7");
        assert_eq!(details.name, "bash");
        assert!(details.capabilities.terminate.allowed);
        assert!(!details.capabilities.resume.allowed);
        assert!(!details.is_self);
    }

    #[test]
    fn details_of_a_recycled_pid_are_discarded_as_stale() {
        let query = service(Ok(raw(8))).details(ProcessInstanceId::new(55, 7));

        assert!(query.value.is_none());
        assert_eq!(query.outcome.status.as_str(), "staleProcess");
    }

    #[test]
    fn a_vanished_process_is_gone() {
        let query = service(Err(ControlError::Gone)).details(ProcessInstanceId::new(55, 7));
        assert_eq!(query.outcome.status.as_str(), "processGone");
    }

    #[test]
    fn provenance_and_paths_are_identity_checked_too() {
        let inspector = service(Ok(raw(8)));
        assert_eq!(
            inspector
                .provenance(ProcessInstanceId::new(55, 7))
                .outcome
                .status
                .as_str(),
            "staleProcess"
        );
        assert_eq!(
            inspector
                .executable_path(ProcessInstanceId::new(55, 7))
                .outcome
                .status
                .as_str(),
            "staleProcess"
        );
        assert!(inspector
            .provenance(ProcessInstanceId::new(55, 8))
            .value
            .is_some());
    }

    #[test]
    fn a_refused_executable_is_permission_denied_not_unsupported() {
        let query = service(Ok(raw(7))).sha256(ProcessInstanceId::new(55, 7));
        assert_eq!(query.outcome.status.as_str(), "permissionDenied");
    }

    #[test]
    fn a_kernel_thread_is_offered_nothing() {
        let mut kernel = raw(1);
        kernel.category = ProcessClass::KernelThread;
        kernel.executable = Field::missing(Availability::not_detected("kernel thread"));

        let capabilities = capabilities(&kernel, false, false, true);
        for capability in [
            &capabilities.terminate,
            &capabilities.terminate_tree,
            &capabilities.force_kill,
            &capabilities.suspend,
            &capabilities.set_priority,
            &capabilities.set_affinity,
            &capabilities.open_location,
            &capabilities.compute_hash,
        ] {
            assert!(!capability.allowed);
            assert!(capability.reason.is_some(), "a disabled action says why");
        }
    }

    #[test]
    fn a_zombie_cannot_be_controlled() {
        let mut zombie = raw(1);
        zombie.state = ProcessState::Zombie;

        let capabilities = capabilities(&zombie, false, false, true);
        assert!(!capabilities.terminate.allowed);
        assert!(capabilities
            .terminate
            .reason
            .as_deref()
            .unwrap()
            .contains("exited"));
    }

    #[test]
    fn resume_is_offered_only_for_what_pulse_suspended() {
        let mut stopped = raw(1);
        stopped.state = ProcessState::Stopped;

        let foreign = capabilities(&stopped, false, false, true);
        assert!(!foreign.resume.allowed);
        assert!(!foreign.suspend.allowed);

        let ours = capabilities(&stopped, false, true, true);
        assert!(ours.resume.allowed);
        assert!(!ours.suspend.allowed);
    }

    #[test]
    fn pulse_is_never_offered_suspend_on_itself() {
        let capabilities = capabilities(&raw(1), true, false, true);
        assert!(!capabilities.suspend.allowed);
        assert!(capabilities.terminate.allowed, "manual, with a warning");
    }

    #[test]
    fn platform_refusals_carry_through_with_their_reason() {
        let mut other_user = raw(1);
        other_user.access = PlatformAccess::all(Capability::denied("Permission denied"));

        let capabilities = capabilities(&other_user, false, false, true);
        assert_eq!(
            capabilities.terminate.reason.as_deref(),
            Some("Permission denied")
        );
        assert!(!capabilities.set_priority.allowed);
    }

    #[test]
    fn force_kill_is_denied_where_unsupported() {
        let capabilities = capabilities(&raw(1), false, false, false);
        assert!(!capabilities.force_kill.allowed);
    }

    #[test]
    fn a_replaced_executable_can_be_hashed_but_not_located() {
        let mut replaced = raw(1);
        let mut info = executable("/usr/bin/bash");
        info.replaced_on_disk = true;
        replaced.executable = Field::available(info);

        let capabilities = capabilities(&replaced, false, false, true);
        assert!(!capabilities.open_location.allowed);
        assert!(capabilities.compute_hash.allowed);
    }

    #[test]
    fn details_serialise_without_a_command_line() {
        let details = service(Ok(raw(7)))
            .details(ProcessInstanceId::new(55, 7))
            .value
            .expect("details");
        let json = serde_json::to_value(&details).expect("serialise");

        assert_eq!(json["capabilities"]["terminateTree"]["allowed"], true);
        assert_eq!(json["executable"]["value"]["fileName"], "bash");
        assert_eq!(json["priority"]["value"]["kind"], "nice");
        assert_eq!(json["priorityKind"], "nice");
        assert_eq!(json["forceKillSupported"], true);
        for forbidden in ["commandLine", "arguments", "environment", "cmdline", "argv"] {
            assert!(json.get(forbidden).is_none());
        }
    }

    #[test]
    fn provenance_serialises_with_its_kind() {
        let json = serde_json::to_value(Provenance::NotPackaged {
            location: LocationHint::UserHome,
        })
        .expect("serialise");
        assert_eq!(json["kind"], "notPackaged");
        assert_eq!(json["location"], "userHome");

        let json = serde_json::to_value(Provenance::Signature {
            signature: SignatureInfo {
                trust: TrustStatus::SignedButUntrusted,
                source: Some(SignatureSource::Catalog),
                publisher: Some("Contoso".into()),
                detail: "untrusted root".into(),
            },
        })
        .expect("serialise");
        assert_eq!(json["signature"]["trust"], "signedButUntrusted");
        assert_eq!(json["signature"]["source"], "catalog");
    }
}
