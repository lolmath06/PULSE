//! [`ProcessControlService`] — the only code in PULSE that changes a process.
//!
//! ```text
//! ProcessSnapshotService   reads the table            every Refresh
//! ProcessInspectorService  reads one process in depth when a row is inspected
//! ProcessControlService    acts on one process        only when the user clicks
//! ```
//!
//! The three are separate on purpose. Nothing in the metrics engine, in a
//! provider or in the snapshot service can reach this module, and nothing
//! here is ever triggered by a metric, a threshold or a heuristic: every call
//! starts with a click.
//!
//! # A PID is a handle, the instance is the target
//!
//! Every request names a [`ProcessInstanceId`] — PID **and** start token.
//! Immediately before acting, the service asks the platform to *pin* whatever
//! currently holds that PID and reports its start token:
//!
//! ```text
//! user selected   PID 5000, start A
//! process A exits; PID 5000 is recycled by process B
//! user clicks End process
//! pin(5000)      -> start B
//! B != A         -> staleProcess, and B is never touched
//! ```
//!
//! *Pinning* is what closes the gap between that check and the action:
//!
//! * **Linux** opens a `pidfd` first and reads the start token after. A pidfd
//!   refers to one process for its whole life — if that process exits and the
//!   PID is recycled, signals sent through the pidfd fail with `ESRCH` rather
//!   than reaching the newcomer.
//! * **Windows** opens a process `HANDLE` and reads the creation time *through
//!   that handle*. Windows does not recycle a PID while any handle to the
//!   process object is open, so the handle and the check refer to the same
//!   process for as long as the action lasts.
//!
//! See `docs/processes/controls.md`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::action::{ProcessActionResult, ProcessActionStatus, TreeTerminationSummary};
use super::identity::ProcessInstanceId;

// --- the shared vocabulary ---------------------------------------------------

/// The least a platform must be allowed to do for one action.
///
/// Windows maps each to the minimum `OpenProcess` access mask; Linux uses the
/// ordinary permissions of the user PULSE runs as and ignores the distinction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Query,
    Terminate,
    SuspendResume,
    SetPriority,
    SetAffinity,
}

/// How *End process* ends a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminateMode {
    /// Linux `SIGTERM` — the process may clean up. Windows `TerminateProcess`.
    Graceful,
    /// Linux `SIGKILL`. A separate, explicitly named action; never an automatic
    /// escalation of `Graceful`. Not offered on Windows, whose only
    /// termination is already immediate.
    Force,
}

/// Why a platform call did not do what was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlError {
    /// The process no longer exists (or has exited and is only a zombie).
    Gone,
    PermissionDenied(String),
    Unsupported(String),
    Invalid(String),
    Platform(String),
}

impl ControlError {
    /// The action result this error means, for process `pid`.
    pub fn into_result(self, pid: u32) -> ProcessActionResult {
        match self {
            ControlError::Gone => ProcessActionResult::gone(pid),
            ControlError::PermissionDenied(reason) => {
                ProcessActionResult::new(ProcessActionStatus::PermissionDenied, reason)
            }
            ControlError::Unsupported(reason) => ProcessActionResult::unsupported(reason),
            ControlError::Invalid(reason) => ProcessActionResult::invalid(reason),
            ControlError::Platform(reason) => {
                ProcessActionResult::new(ProcessActionStatus::PlatformError, reason)
            }
        }
    }
}

/// A Windows priority class, as `SetPriorityClass` names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowsPriorityClass {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
    Realtime,
}

/// A process's scheduling priority, in the platform's own terms.
///
/// Deliberately **not** normalised onto one scale. A Linux nice value and a
/// Windows priority class are different mechanisms; the interface shows the
/// real one and offers presets beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProcessPriority {
    /// Linux nice value, `-20` (most favoured) to `19` (least).
    Nice { value: i32 },
    /// Windows priority class.
    WindowsClass { class: WindowsPriorityClass },
}

/// The nice range Linux accepts.
pub const NICE_MIN: i32 = -20;
pub const NICE_MAX: i32 = 19;

/// The Linux presets the interface offers, and the nice value each sets.
///
/// Documented in `docs/processes/controls.md`. `Low` is the bottom of the
/// range, as Windows' *Low* (`IDLE_PRIORITY_CLASS`) is the bottom of its own.
pub const NICE_PRESETS: [(&str, i32); 5] = [
    ("High", -10),
    ("Above normal", -5),
    ("Normal", 0),
    ("Below normal", 5),
    ("Low", 19),
];

/// Which priority vocabulary a platform speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorityKind {
    Nice,
    WindowsClass,
}

/// Which logical processors a process may run on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessAffinity {
    /// The processors the process is currently allowed on, ascending.
    pub cpus: Vec<u32>,
    /// Every processor it could be allowed on, ascending.
    pub available: Vec<u32>,
    /// Set when PULSE can read but not faithfully change this affinity —
    /// Windows machines with more than one processor group.
    pub limitation: Option<String>,
}

/// How many threads a per-thread setting reached.
///
/// Linux nice values and CPU affinities belong to **threads**; PULSE applies
/// the change to every thread the process has at that moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThreadApply {
    pub applied: u32,
    pub failed: u32,
}

/// What PULSE did when it suspended one process — and therefore exactly what
/// it may undo.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SuspendRecord {
    /// Windows: the threads PULSE added one suspension to. Empty on Linux,
    /// where one `SIGSTOP` suspends the whole process.
    pub thread_ids: Vec<u32>,
    /// Windows: threads that refused the suspension.
    pub failed_threads: u32,
}

/// What a resume reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResumeReport {
    pub resumed: u32,
    /// Threads that exited while suspended — nothing to resume.
    pub gone: u32,
    pub failed: u32,
}

/// One process as the tree planner sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessNode {
    pub instance: ProcessInstanceId,
    pub parent_pid: Option<u32>,
}

// --- the platform seam ------------------------------------------------------

/// One process, held so that it cannot be confused with a successor.
///
/// Lives only for the duration of one action: no pinned process is ever
/// stored, so no handle or descriptor outlives the click that opened it.
pub trait PinnedProcess: Send {
    /// The start token of the process this pin refers to, read after pinning.
    fn start_token(&self) -> u64;
    /// Whether the process is currently stopped (Linux `T`). Always `false`
    /// on Windows, which has no process-level state.
    fn is_stopped(&self) -> bool;
    fn terminate(&self, mode: TerminateMode) -> Result<(), ControlError>;
    fn suspend(&self) -> Result<SuspendRecord, ControlError>;
    fn resume(&self, record: &SuspendRecord) -> Result<ResumeReport, ControlError>;
    fn priority(&self) -> Result<ProcessPriority, ControlError>;
    fn set_priority(&self, priority: ProcessPriority) -> Result<ThreadApply, ControlError>;
    fn affinity(&self) -> Result<ProcessAffinity, ControlError>;
    fn set_affinity(&self, cpus: &[u32]) -> Result<ThreadApply, ControlError>;
}

/// What a platform provides so PULSE can act on processes.
pub trait ProcessControlBackend: Send + Sync + std::fmt::Debug {
    /// Pins whatever process holds `pid` right now, with at least `access`.
    fn pin(&self, pid: u32, access: Access) -> Result<Box<dyn PinnedProcess>, ControlError>;
    /// A fresh, minimal view of the whole table, for *End process tree*.
    fn process_nodes(&self) -> Vec<ProcessNode>;
    fn priority_kind(&self) -> PriorityKind;
    fn supports_force_kill(&self) -> bool;
}

// --- what PULSE has suspended ----------------------------------------------

/// The processes PULSE itself suspended during this session.
///
/// *Resume* is offered for these and nothing else. A process that was
/// already stopped — by a shell's `Ctrl+Z`, by a debugger, by another tool —
/// is not PULSE's to resume, and on Windows resuming threads PULSE did not
/// suspend would silently undo someone else's suspension.
#[derive(Debug, Default)]
pub struct SuspensionLedger {
    entries: Mutex<HashMap<ProcessInstanceId, SuspendRecord>>,
}

impl SuspensionLedger {
    pub fn contains(&self, id: &ProcessInstanceId) -> bool {
        self.lock().contains_key(id)
    }

    fn insert(&self, id: ProcessInstanceId, record: SuspendRecord) {
        self.lock().insert(id, record);
    }

    fn get(&self, id: &ProcessInstanceId) -> Option<SuspendRecord> {
        self.lock().get(id).cloned()
    }

    fn remove(&self, id: &ProcessInstanceId) {
        self.lock().remove(id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<ProcessInstanceId, SuspendRecord>> {
        // A poisoned ledger still holds correct entries: every mutation is a
        // single insert or remove.
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

// --- the tree planner -------------------------------------------------------

/// The most descendants one *End process tree* will plan. A bound, so that a
/// corrupted or cyclic parent table can never become an unbounded walk.
pub const TREE_LIMIT: usize = 4_096;

/// Plans an *End process tree*: every descendant of `root`, deepest first.
///
/// A node is a child of a parent only when its parent PID matches **and** it
/// started no earlier than that parent. The second condition matters on
/// Windows, whose parent PID is never updated: a process whose parent has
/// exited can point at a recycled PID now belonging to an unrelated,
/// *younger* process, and without the check that unrelated process's children
/// would be swept into the tree.
///
/// The root itself is not in the plan — it is ended last, by the caller.
/// Order is by depth descending, then PID, so the plan is deterministic.
pub fn plan_tree(
    root: ProcessInstanceId,
    nodes: &[ProcessNode],
    limit: usize,
) -> Vec<ProcessInstanceId> {
    let mut children: HashMap<u32, Vec<ProcessInstanceId>> = HashMap::new();
    for node in nodes {
        if let Some(parent) = node.parent_pid {
            if parent != node.instance.pid {
                children.entry(parent).or_default().push(node.instance);
            }
        }
    }

    let mut seen: HashSet<ProcessInstanceId> = HashSet::from([root]);
    let mut planned: Vec<(usize, ProcessInstanceId)> = Vec::new();
    let mut frontier = vec![(0_usize, root)];

    while let Some((depth, parent)) = frontier.pop() {
        let Some(candidates) = children.get(&parent.pid) else {
            continue;
        };
        for child in candidates {
            if planned.len() >= limit {
                break;
            }
            if child.start_token < parent.start_token || !seen.insert(*child) {
                continue;
            }
            planned.push((depth + 1, *child));
            frontier.push((depth + 1, *child));
        }
    }

    planned.sort_by(|(left_depth, left), (right_depth, right)| {
        right_depth
            .cmp(left_depth)
            .then_with(|| left.pid.cmp(&right.pid))
    });
    planned.into_iter().map(|(_, id)| id).collect()
}

// --- the service ------------------------------------------------------------

/// Acts on processes, only when asked, only on the instance named.
#[derive(Debug)]
pub struct ProcessControlService {
    backend: Option<Arc<dyn ProcessControlBackend>>,
    ledger: Arc<SuspensionLedger>,
    /// PULSE's own PID, so PULSE never suspends itself and never ends itself
    /// as a side effect of ending a tree.
    self_pid: u32,
}

impl ProcessControlService {
    pub fn new(backend: Option<Arc<dyn ProcessControlBackend>>) -> Self {
        Self::with_self_pid(backend, std::process::id())
    }

    /// For tests, which must be able to name a PID as "PULSE itself".
    pub fn with_self_pid(backend: Option<Arc<dyn ProcessControlBackend>>, self_pid: u32) -> Self {
        Self {
            backend,
            ledger: Arc::new(SuspensionLedger::default()),
            self_pid,
        }
    }

    pub fn unsupported() -> Self {
        Self::new(None)
    }

    /// The ledger, shared with the inspector so it can say whether *Resume*
    /// applies.
    pub fn ledger(&self) -> Arc<SuspensionLedger> {
        Arc::clone(&self.ledger)
    }

    pub fn self_pid(&self) -> u32 {
        self.self_pid
    }

    /// What the inspector should know about this platform's controls.
    pub fn support(&self) -> super::inspector::ControlSupport {
        super::inspector::ControlSupport {
            priority_kind: self.backend.as_ref().map(|backend| backend.priority_kind()),
            force_kill: self.supports_force_kill(),
        }
    }

    pub fn supports_force_kill(&self) -> bool {
        self.backend
            .as_ref()
            .is_some_and(|backend| backend.supports_force_kill())
    }

    /// Pins `id.pid` and proves it is still `id`.
    ///
    /// The **only** way any action in this file reaches a process.
    fn target(
        &self,
        id: ProcessInstanceId,
        access: Access,
    ) -> Result<Box<dyn PinnedProcess>, ProcessActionResult> {
        let backend = self.backend.as_ref().ok_or_else(|| {
            ProcessActionResult::unsupported("PULSE cannot control processes on this platform.")
        })?;

        if id.pid == 0 {
            return Err(ProcessActionResult::invalid(
                "PID 0 is not a process PULSE can address.",
            ));
        }

        let pinned = backend
            .pin(id.pid, access)
            .map_err(|error| error.into_result(id.pid))?;

        if pinned.start_token() != id.start_token {
            return Err(ProcessActionResult::stale(id.pid));
        }

        Ok(pinned)
    }

    /// *End process* — `SIGTERM`/`TerminateProcess`, or `SIGKILL` for *Force
    /// kill*.
    pub fn terminate(&self, id: ProcessInstanceId, mode: TerminateMode) -> ProcessActionResult {
        if mode == TerminateMode::Force && !self.supports_force_kill() {
            return ProcessActionResult::unsupported(
                "Force kill is Linux-only; End process already terminates immediately here.",
            );
        }

        let pinned = match self.target(id, Access::Terminate) {
            Ok(pinned) => pinned,
            Err(result) => return result,
        };

        let result = match pinned.terminate(mode) {
            Ok(()) => ProcessActionResult::success(match mode {
                TerminateMode::Graceful => format!("Asked process {} to end.", id.pid),
                TerminateMode::Force => format!("Force-killed process {}.", id.pid),
            }),
            // The goal of ending a process is met by its having ended.
            Err(error) => error.into_result(id.pid),
        };

        if result.status == ProcessActionStatus::ProcessGone || result.is_success() {
            self.ledger.remove(&id);
        }
        result
    }

    /// *End process tree*: descendants deepest first, the root last, each one
    /// re-validated immediately before it is ended.
    ///
    /// One bounded pass over a snapshot taken at the moment of the click.
    /// Processes spawned after that snapshot are not chased — a loop that
    /// kept re-scanning until the tree was empty could race a fork loop
    /// forever.
    pub fn terminate_tree(&self, id: ProcessInstanceId) -> ProcessActionResult {
        let Some(backend) = self.backend.clone() else {
            return ProcessActionResult::unsupported(
                "PULSE cannot control processes on this platform.",
            );
        };

        // Validate the root before planning anything.
        let root = match self.target(id, Access::Terminate) {
            Ok(pinned) => pinned,
            Err(result) => return result,
        };

        let plan = plan_tree(id, &backend.process_nodes(), TREE_LIMIT);
        let mut summary = TreeTerminationSummary {
            requested: plan.len() as u32 + 1,
            ..TreeTerminationSummary::default()
        };

        for child in plan {
            if child.pid == self.self_pid {
                summary.skipped_self += 1;
                continue;
            }

            let pinned = match backend.pin(child.pid, Access::Terminate) {
                Ok(pinned) => pinned,
                Err(error) => {
                    tally(&mut summary, error);
                    continue;
                }
            };

            if pinned.start_token() != child.start_token {
                summary.stale_skipped += 1;
                continue;
            }

            match pinned.terminate(TerminateMode::Graceful) {
                Ok(()) => {
                    summary.terminated += 1;
                    self.ledger.remove(&child);
                }
                Err(error) => tally(&mut summary, error),
            }
        }

        let root_status = match root.terminate(TerminateMode::Graceful) {
            Ok(()) => {
                summary.terminated += 1;
                self.ledger.remove(&id);
                ProcessActionStatus::Success
            }
            Err(error) => {
                let status = error.clone().into_result(id.pid).status;
                tally(&mut summary, error);
                status
            }
        };

        debug_assert!(summary.is_consistent());

        let descendants_ok = summary.permission_denied == 0 && summary.failed == 0;
        let status = match root_status {
            ProcessActionStatus::Success if descendants_ok => ProcessActionStatus::Success,
            ProcessActionStatus::Success | ProcessActionStatus::ProcessGone => {
                ProcessActionStatus::PartialFailure
            }
            other => other,
        };

        let mut result = ProcessActionResult::new(
            status,
            format!(
                "{} of {} processes ended; {} had already exited, {} refused, {} skipped as \
                 stale, {} failed.",
                summary.terminated,
                summary.requested,
                summary.already_gone,
                summary.permission_denied,
                summary.stale_skipped,
                summary.failed
            ),
        );
        result.affected_count = Some(summary.terminated);
        result.failed_count = Some(summary.permission_denied + summary.failed);
        result.tree = Some(summary);
        result
    }

    /// *Suspend*. Recorded, so that only PULSE's own suspension can be undone.
    pub fn suspend(&self, id: ProcessInstanceId) -> ProcessActionResult {
        if id.pid == self.self_pid {
            return ProcessActionResult::invalid(
                "Suspending PULSE would freeze the window needed to resume it.",
            );
        }
        if self.ledger.contains(&id) {
            return ProcessActionResult::invalid("PULSE has already suspended this process.");
        }

        let pinned = match self.target(id, Access::SuspendResume) {
            Ok(pinned) => pinned,
            Err(result) => return result,
        };

        if pinned.is_stopped() {
            return ProcessActionResult::invalid(
                "This process is already stopped by something other than PULSE, so PULSE will \
                 neither suspend it again nor claim that stop as its own.",
            );
        }

        match pinned.suspend() {
            Ok(record) => {
                let suspended = record.thread_ids.len() as u32;
                let failed = record.failed_threads;
                self.ledger.insert(id, record);

                if failed > 0 {
                    ProcessActionResult::new(
                        ProcessActionStatus::PartialFailure,
                        format!(
                            "Suspended {suspended} threads; {failed} refused and keep running."
                        ),
                    )
                    .with_counts(suspended, failed)
                } else {
                    ProcessActionResult::success(format!("Suspended process {}.", id.pid))
                }
            }
            Err(error) => error.into_result(id.pid),
        }
    }

    /// *Resume* — only what PULSE itself suspended.
    pub fn resume(&self, id: ProcessInstanceId) -> ProcessActionResult {
        let Some(record) = self.ledger.get(&id) else {
            return ProcessActionResult::invalid(
                "PULSE did not suspend this process, so it will not resume it.",
            );
        };

        let pinned = match self.target(id, Access::SuspendResume) {
            Ok(pinned) => pinned,
            Err(result) => {
                // The suspended instance no longer exists under this PID:
                // there is nothing left to resume, now or later.
                if matches!(
                    result.status,
                    ProcessActionStatus::StaleProcess | ProcessActionStatus::ProcessGone
                ) {
                    self.ledger.remove(&id);
                }
                return result;
            }
        };

        match pinned.resume(&record) {
            Ok(report) => {
                self.ledger.remove(&id);
                if report.failed > 0 {
                    ProcessActionResult::new(
                        ProcessActionStatus::PartialFailure,
                        format!(
                            "Resumed {} threads; {} refused.",
                            report.resumed, report.failed
                        ),
                    )
                    .with_counts(report.resumed, report.failed)
                } else {
                    ProcessActionResult::success(format!("Resumed process {}.", id.pid))
                }
            }
            Err(error) => {
                if error == ControlError::Gone {
                    self.ledger.remove(&id);
                }
                error.into_result(id.pid)
            }
        }
    }

    /// Reads the priority of exactly this instance.
    pub fn priority(&self, id: ProcessInstanceId) -> Result<ProcessPriority, ProcessActionResult> {
        let pinned = self.target(id, Access::Query)?;
        pinned.priority().map_err(|error| error.into_result(id.pid))
    }

    /// Changes the priority of exactly this instance.
    ///
    /// `confirm_realtime` is enforced here as well as in the interface: a
    /// Windows *Realtime* request without it is refused before any handle is
    /// opened.
    pub fn set_priority(
        &self,
        id: ProcessInstanceId,
        priority: ProcessPriority,
        confirm_realtime: bool,
    ) -> ProcessActionResult {
        let Some(backend) = &self.backend else {
            return ProcessActionResult::unsupported(
                "PULSE cannot control processes on this platform.",
            );
        };

        if let Err(reason) = validate_priority(priority, backend.priority_kind(), confirm_realtime)
        {
            return ProcessActionResult::invalid(reason);
        }

        let pinned = match self.target(id, Access::SetPriority) {
            Ok(pinned) => pinned,
            Err(result) => return result,
        };

        match pinned.set_priority(priority) {
            Ok(apply) => applied(apply, "Priority changed", id.pid),
            Err(error) => error.into_result(id.pid),
        }
    }

    /// Reads the CPU affinity of exactly this instance.
    pub fn affinity(&self, id: ProcessInstanceId) -> Result<ProcessAffinity, ProcessActionResult> {
        let pinned = self.target(id, Access::Query)?;
        pinned.affinity().map_err(|error| error.into_result(id.pid))
    }

    /// Changes the CPU affinity of exactly this instance.
    ///
    /// An empty set is refused: a process allowed on no processor cannot run.
    pub fn set_affinity(&self, id: ProcessInstanceId, cpus: &[u32]) -> ProcessActionResult {
        if cpus.is_empty() {
            return ProcessActionResult::invalid("At least one logical processor must be kept.");
        }
        if cpus.len() > 8_192 {
            return ProcessActionResult::invalid("Too many logical processors in the request.");
        }

        let pinned = match self.target(id, Access::SetAffinity) {
            Ok(pinned) => pinned,
            Err(result) => return result,
        };

        match pinned.set_affinity(cpus) {
            Ok(apply) => applied(apply, "Affinity changed", id.pid),
            Err(error) => error.into_result(id.pid),
        }
    }
}

/// Files one failed tree target in the right bucket.
fn tally(summary: &mut TreeTerminationSummary, error: ControlError) {
    match error {
        ControlError::Gone => summary.already_gone += 1,
        ControlError::PermissionDenied(_) => summary.permission_denied += 1,
        _ => summary.failed += 1,
    }
}

/// The result of a per-thread setting.
fn applied(apply: ThreadApply, what: &str, pid: u32) -> ProcessActionResult {
    if apply.failed > 0 {
        ProcessActionResult::new(
            ProcessActionStatus::PartialFailure,
            format!(
                "{what} on {} threads of process {pid}; {} threads refused.",
                apply.applied, apply.failed
            ),
        )
        .with_counts(apply.applied, apply.failed)
    } else {
        ProcessActionResult::success(format!("{what} for process {pid}."))
            .with_counts(apply.applied, 0)
    }
}

/// Checks a priority request against the platform's vocabulary.
pub fn validate_priority(
    priority: ProcessPriority,
    kind: PriorityKind,
    confirm_realtime: bool,
) -> Result<(), String> {
    match (priority, kind) {
        (ProcessPriority::Nice { value }, PriorityKind::Nice) => {
            if (NICE_MIN..=NICE_MAX).contains(&value) {
                Ok(())
            } else {
                Err(format!(
                    "Nice values run from {NICE_MIN} to {NICE_MAX}; {value} is outside that range."
                ))
            }
        }
        (ProcessPriority::WindowsClass { class }, PriorityKind::WindowsClass) => {
            if class == WindowsPriorityClass::Realtime && !confirm_realtime {
                Err(
                    "Realtime priority can make the system unresponsive and needs an explicit \
                     confirmation."
                        .to_string(),
                )
            } else {
                Ok(())
            }
        }
        (ProcessPriority::Nice { .. }, PriorityKind::WindowsClass) => {
            Err("Windows uses priority classes, not nice values.".to_string())
        }
        (ProcessPriority::WindowsClass { .. }, PriorityKind::Nice) => {
            Err("Linux uses nice values, not Windows priority classes.".to_string())
        }
    }
}

#[cfg(test)]
mod tests;
