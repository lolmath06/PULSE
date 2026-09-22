//! Service-level tests for [`ProcessControlService`].
//!
//! Every test runs against a scripted backend that records each call it
//! receives. The property that matters most — *a recycled PID is never
//! acted on* — is asserted as "the backend recorded no action against the
//! newcomer", not merely as "the result said stale".

use super::*;

/// One call the fake backend received.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Pin(u32),
    Terminate(u32, TerminateMode),
    Suspend(u32),
    Resume(u32),
    SetPriority(u32, ProcessPriority),
    SetAffinity(u32, Vec<u32>),
    Priority(u32),
    Affinity(u32),
}

#[derive(Debug, Clone)]
struct FakeProcess {
    token: u64,
    parent: Option<u32>,
    stopped: bool,
    deny: bool,
    /// Exits between being pinned and being acted on.
    vanishes_on_action: bool,
}

impl FakeProcess {
    fn new(token: u64) -> Self {
        Self {
            token,
            parent: None,
            stopped: false,
            deny: false,
            vanishes_on_action: false,
        }
    }

    fn child_of(mut self, parent: u32) -> Self {
        self.parent = Some(parent);
        self
    }
}

#[derive(Debug, Default)]
struct World {
    processes: HashMap<u32, FakeProcess>,
    calls: Vec<Call>,
    /// Applied between planning a tree and ending its members.
    after_plan: Vec<(u32, Option<FakeProcess>)>,
}

#[derive(Debug, Clone, Default)]
struct FakeBackend {
    world: Arc<Mutex<World>>,
    kind: Option<PriorityKind>,
}

impl FakeBackend {
    fn with(processes: Vec<(u32, FakeProcess)>) -> Self {
        let backend = Self::default();
        backend.world.lock().unwrap().processes = processes.into_iter().collect();
        backend
    }

    fn windows(self) -> Self {
        Self {
            kind: Some(PriorityKind::WindowsClass),
            ..self
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.world.lock().unwrap().calls.clone()
    }

    fn actions(&self) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|call| !matches!(call, Call::Pin(_)))
            .collect()
    }

    fn replace(&self, pid: u32, process: Option<FakeProcess>) {
        let mut world = self.world.lock().unwrap();
        match process {
            Some(process) => world.processes.insert(pid, process),
            None => world.processes.remove(&pid),
        };
    }

    fn service(&self) -> ProcessControlService {
        ProcessControlService::with_self_pid(Some(Arc::new(self.clone())), 999_999)
    }
}

struct FakePinned {
    pid: u32,
    token: u64,
    stopped: bool,
    world: Arc<Mutex<World>>,
}

impl FakePinned {
    fn act(&self, call: Call) -> Result<(), ControlError> {
        let mut world = self.world.lock().unwrap();
        // The pin refers to one incarnation: if it has gone, or the PID now
        // holds another one, the action fails like a pidfd would.
        match world.processes.get(&self.pid) {
            Some(process) if process.token == self.token => {
                if process.vanishes_on_action {
                    world.processes.remove(&self.pid);
                    return Err(ControlError::Gone);
                }
                if process.deny {
                    return Err(ControlError::PermissionDenied("denied".into()));
                }
            }
            _ => return Err(ControlError::Gone),
        }
        world.calls.push(call);
        Ok(())
    }
}

impl PinnedProcess for FakePinned {
    fn start_token(&self) -> u64 {
        self.token
    }
    fn is_stopped(&self) -> bool {
        self.stopped
    }
    fn terminate(&self, mode: TerminateMode) -> Result<(), ControlError> {
        self.act(Call::Terminate(self.pid, mode))?;
        self.world.lock().unwrap().processes.remove(&self.pid);
        Ok(())
    }
    fn suspend(&self) -> Result<SuspendRecord, ControlError> {
        self.act(Call::Suspend(self.pid))?;
        Ok(SuspendRecord::default())
    }
    fn resume(&self, _record: &SuspendRecord) -> Result<ResumeReport, ControlError> {
        self.act(Call::Resume(self.pid))?;
        Ok(ResumeReport {
            resumed: 1,
            ..ResumeReport::default()
        })
    }
    fn priority(&self) -> Result<ProcessPriority, ControlError> {
        self.act(Call::Priority(self.pid))?;
        Ok(ProcessPriority::Nice { value: 0 })
    }
    fn set_priority(&self, priority: ProcessPriority) -> Result<ThreadApply, ControlError> {
        self.act(Call::SetPriority(self.pid, priority))?;
        Ok(ThreadApply {
            applied: 1,
            failed: 0,
        })
    }
    fn affinity(&self) -> Result<ProcessAffinity, ControlError> {
        self.act(Call::Affinity(self.pid))?;
        Ok(ProcessAffinity {
            cpus: vec![0, 1],
            available: vec![0, 1],
            limitation: None,
        })
    }
    fn set_affinity(&self, cpus: &[u32]) -> Result<ThreadApply, ControlError> {
        self.act(Call::SetAffinity(self.pid, cpus.to_vec()))?;
        Ok(ThreadApply {
            applied: 1,
            failed: 0,
        })
    }
}

impl ProcessControlBackend for FakeBackend {
    fn pin(&self, pid: u32, _access: Access) -> Result<Box<dyn PinnedProcess>, ControlError> {
        let mut world = self.world.lock().unwrap();
        world.calls.push(Call::Pin(pid));
        let process = world
            .processes
            .get(&pid)
            .cloned()
            .ok_or(ControlError::Gone)?;
        if process.deny {
            return Err(ControlError::PermissionDenied("Access is denied.".into()));
        }
        Ok(Box::new(FakePinned {
            pid,
            token: process.token,
            stopped: process.stopped,
            world: Arc::clone(&self.world),
        }))
    }

    fn process_nodes(&self) -> Vec<ProcessNode> {
        let mut world = self.world.lock().unwrap();
        let nodes = world
            .processes
            .iter()
            .map(|(pid, process)| ProcessNode {
                instance: ProcessInstanceId::new(*pid, process.token),
                parent_pid: process.parent,
            })
            .collect();
        // Simulate the table changing between planning and acting.
        let changes = std::mem::take(&mut world.after_plan);
        for (pid, process) in changes {
            match process {
                Some(process) => world.processes.insert(pid, process),
                None => world.processes.remove(&pid),
            };
        }
        nodes
    }

    fn priority_kind(&self) -> PriorityKind {
        self.kind.unwrap_or(PriorityKind::Nice)
    }

    fn supports_force_kill(&self) -> bool {
        self.kind != Some(PriorityKind::WindowsClass)
    }
}

const A: u64 = 1_000;
const B: u64 = 2_000;

fn selected() -> ProcessInstanceId {
    ProcessInstanceId::new(123, A)
}

/// PID 123 was process A when selected and is process B now.
fn recycled() -> FakeBackend {
    FakeBackend::with(vec![(123, FakeProcess::new(B))])
}

// --- the PID-reuse guard, for every destructive family ------------------------

#[test]
fn terminate_refuses_a_recycled_pid_and_never_signals_the_newcomer() {
    let backend = recycled();
    let result = backend
        .service()
        .terminate(selected(), TerminateMode::Graceful);

    assert_eq!(result.status, ProcessActionStatus::StaleProcess);
    assert!(backend.actions().is_empty(), "{:?}", backend.calls());
    assert!(backend.world.lock().unwrap().processes.contains_key(&123));
}

#[test]
fn force_kill_refuses_a_recycled_pid() {
    let backend = recycled();
    let result = backend
        .service()
        .terminate(selected(), TerminateMode::Force);

    assert_eq!(result.status, ProcessActionStatus::StaleProcess);
    assert!(backend.actions().is_empty());
}

#[test]
fn suspend_refuses_a_recycled_pid() {
    let backend = recycled();
    let result = backend.service().suspend(selected());

    assert_eq!(result.status, ProcessActionStatus::StaleProcess);
    assert!(backend.actions().is_empty());
}

#[test]
fn resume_refuses_a_recycled_pid_even_after_pulse_suspended_the_original() {
    let backend = FakeBackend::with(vec![(123, FakeProcess::new(A))]);
    let service = backend.service();

    assert!(service.suspend(selected()).is_success());
    // A exits; B receives PID 123.
    backend.replace(123, Some(FakeProcess::new(B)));

    let result = service.resume(selected());
    assert_eq!(result.status, ProcessActionStatus::StaleProcess);
    assert_eq!(
        backend.actions(),
        vec![Call::Suspend(123)],
        "B never resumed"
    );
    assert!(
        !service.ledger().contains(&selected()),
        "the dead instance is forgotten"
    );
}

#[test]
fn set_priority_refuses_a_recycled_pid() {
    let backend = recycled();
    let result =
        backend
            .service()
            .set_priority(selected(), ProcessPriority::Nice { value: 10 }, false);

    assert_eq!(result.status, ProcessActionStatus::StaleProcess);
    assert!(backend.actions().is_empty());
}

#[test]
fn set_affinity_refuses_a_recycled_pid() {
    let backend = recycled();
    let result = backend.service().set_affinity(selected(), &[0]);

    assert_eq!(result.status, ProcessActionStatus::StaleProcess);
    assert!(backend.actions().is_empty());
}

#[test]
fn reads_also_refuse_a_recycled_pid() {
    let backend = recycled();
    let service = backend.service();

    assert_eq!(
        service.priority(selected()).unwrap_err().status,
        ProcessActionStatus::StaleProcess
    );
    assert_eq!(
        service.affinity(selected()).unwrap_err().status,
        ProcessActionStatus::StaleProcess
    );
    assert!(backend.actions().is_empty());
}

#[test]
fn a_process_that_exited_is_reported_gone_not_as_a_failure() {
    let backend = FakeBackend::with(vec![]);
    let result = backend
        .service()
        .terminate(selected(), TerminateMode::Graceful);

    assert_eq!(result.status, ProcessActionStatus::ProcessGone);
}

#[test]
fn a_process_exiting_between_pin_and_signal_is_gone() {
    let mut process = FakeProcess::new(A);
    process.vanishes_on_action = true;
    let backend = FakeBackend::with(vec![(123, process)]);

    let result = backend
        .service()
        .terminate(selected(), TerminateMode::Graceful);
    assert_eq!(result.status, ProcessActionStatus::ProcessGone);
}

#[test]
fn permission_denied_stays_permission_denied() {
    let mut process = FakeProcess::new(A);
    process.deny = true;
    let backend = FakeBackend::with(vec![(123, process)]);

    let result = backend
        .service()
        .terminate(selected(), TerminateMode::Graceful);
    assert_eq!(result.status, ProcessActionStatus::PermissionDenied);
    assert!(backend.actions().is_empty());
}

#[test]
fn the_matching_instance_is_acted_on() {
    let backend = FakeBackend::with(vec![(123, FakeProcess::new(A))]);
    let result = backend
        .service()
        .terminate(selected(), TerminateMode::Graceful);

    assert!(result.is_success(), "{result:?}");
    assert_eq!(
        backend.actions(),
        vec![Call::Terminate(123, TerminateMode::Graceful)]
    );
}

#[test]
fn a_platform_without_control_reports_unsupported() {
    let service = ProcessControlService::unsupported();
    assert_eq!(
        service
            .terminate(selected(), TerminateMode::Graceful)
            .status,
        ProcessActionStatus::Unsupported
    );
    assert_eq!(
        service.suspend(selected()).status,
        ProcessActionStatus::Unsupported
    );
}

#[test]
fn force_kill_is_unsupported_where_termination_is_already_immediate() {
    let backend = FakeBackend::with(vec![(123, FakeProcess::new(A))]).windows();
    let result = backend
        .service()
        .terminate(selected(), TerminateMode::Force);

    assert_eq!(result.status, ProcessActionStatus::Unsupported);
    assert!(backend.calls().is_empty(), "nothing was even pinned");
}

// --- suspend tracking ---------------------------------------------------------

#[test]
fn resume_is_allowed_only_after_pulse_suspended_the_instance() {
    let backend = FakeBackend::with(vec![(123, FakeProcess::new(A))]);
    let service = backend.service();

    let refused = service.resume(selected());
    assert_eq!(refused.status, ProcessActionStatus::InvalidRequest);
    assert!(backend.actions().is_empty());

    assert!(service.suspend(selected()).is_success());
    assert!(service.ledger().contains(&selected()));

    assert!(service.resume(selected()).is_success());
    assert!(!service.ledger().contains(&selected()));
    assert_eq!(
        backend.actions(),
        vec![Call::Suspend(123), Call::Resume(123)]
    );
}

#[test]
fn a_process_already_stopped_by_someone_else_is_not_claimed() {
    let mut process = FakeProcess::new(A);
    process.stopped = true;
    let backend = FakeBackend::with(vec![(123, process)]);
    let service = backend.service();

    let suspend = service.suspend(selected());
    assert_eq!(suspend.status, ProcessActionStatus::InvalidRequest);
    assert!(!service.ledger().contains(&selected()));

    let resume = service.resume(selected());
    assert_eq!(
        resume.status,
        ProcessActionStatus::InvalidRequest,
        "a stopped state PULSE did not cause is not PULSE's to undo"
    );
    assert!(backend.actions().is_empty());
}

#[test]
fn suspending_twice_is_refused_rather_than_stacking() {
    let backend = FakeBackend::with(vec![(123, FakeProcess::new(A))]);
    let service = backend.service();

    assert!(service.suspend(selected()).is_success());
    assert_eq!(
        service.suspend(selected()).status,
        ProcessActionStatus::InvalidRequest
    );
    assert_eq!(backend.actions(), vec![Call::Suspend(123)]);
}

#[test]
fn a_suspended_process_that_exits_is_forgotten_on_resume() {
    let backend = FakeBackend::with(vec![(123, FakeProcess::new(A))]);
    let service = backend.service();

    assert!(service.suspend(selected()).is_success());
    backend.replace(123, None);

    assert_eq!(
        service.resume(selected()).status,
        ProcessActionStatus::ProcessGone
    );
    assert!(!service.ledger().contains(&selected()));
}

#[test]
fn pulse_never_suspends_itself() {
    let backend = FakeBackend::with(vec![(999_999, FakeProcess::new(A))]);
    let result = backend
        .service()
        .suspend(ProcessInstanceId::new(999_999, A));

    assert_eq!(result.status, ProcessActionStatus::InvalidRequest);
    assert!(backend.calls().is_empty());
}

// --- priority and affinity validation -----------------------------------------

#[test]
fn nice_values_outside_the_kernel_range_are_refused_before_any_call() {
    let backend = FakeBackend::with(vec![(123, FakeProcess::new(A))]);
    let service = backend.service();

    for value in [-21, 20, i32::MIN, i32::MAX] {
        let result = service.set_priority(selected(), ProcessPriority::Nice { value }, false);
        assert_eq!(
            result.status,
            ProcessActionStatus::InvalidRequest,
            "{value}"
        );
    }
    assert!(backend.calls().is_empty());

    let result = service.set_priority(selected(), ProcessPriority::Nice { value: 19 }, false);
    assert!(result.is_success());
}

#[test]
fn realtime_needs_its_own_confirmation() {
    let backend = FakeBackend::with(vec![(123, FakeProcess::new(A))]).windows();
    let service = backend.service();
    let realtime = ProcessPriority::WindowsClass {
        class: WindowsPriorityClass::Realtime,
    };

    let refused = service.set_priority(selected(), realtime, false);
    assert_eq!(refused.status, ProcessActionStatus::InvalidRequest);
    assert!(refused.reason.contains("unresponsive"));
    assert!(backend.calls().is_empty());

    assert!(service
        .set_priority(selected(), realtime, true)
        .is_success());
}

#[test]
fn a_priority_in_the_other_platforms_vocabulary_is_refused() {
    assert!(validate_priority(
        ProcessPriority::Nice { value: 0 },
        PriorityKind::WindowsClass,
        false
    )
    .is_err());
    assert!(validate_priority(
        ProcessPriority::WindowsClass {
            class: WindowsPriorityClass::High
        },
        PriorityKind::Nice,
        false
    )
    .is_err());
}

#[test]
fn an_empty_affinity_is_refused() {
    let backend = FakeBackend::with(vec![(123, FakeProcess::new(A))]);
    let result = backend.service().set_affinity(selected(), &[]);

    assert_eq!(result.status, ProcessActionStatus::InvalidRequest);
    assert!(backend.calls().is_empty());
}

#[test]
fn the_nice_presets_are_inside_the_range_and_ordered() {
    let values: Vec<i32> = NICE_PRESETS.iter().map(|(_, value)| *value).collect();
    assert!(values
        .iter()
        .all(|value| (NICE_MIN..=NICE_MAX).contains(value)));
    assert!(values.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(NICE_PRESETS.contains(&("Normal", 0)));
}

#[test]
fn priorities_serialise_with_their_kind() {
    let json = serde_json::to_value(ProcessPriority::Nice { value: -5 }).expect("serialise");
    assert_eq!(json["kind"], "nice");
    assert_eq!(json["value"], -5);

    let json = serde_json::to_value(ProcessPriority::WindowsClass {
        class: WindowsPriorityClass::BelowNormal,
    })
    .expect("serialise");
    assert_eq!(json["kind"], "windowsClass");
    assert_eq!(json["class"], "belowNormal");
}

// --- the tree -------------------------------------------------------------------

fn node(pid: u32, token: u64, parent: Option<u32>) -> ProcessNode {
    ProcessNode {
        instance: ProcessInstanceId::new(pid, token),
        parent_pid: parent,
    }
}

#[test]
fn the_plan_is_every_descendant_deepest_first() {
    let root = ProcessInstanceId::new(10, 100);
    let nodes = [
        node(1, 1, None),
        node(10, 100, Some(1)),
        node(11, 110, Some(10)),
        node(12, 120, Some(10)),
        node(13, 130, Some(11)),
        node(14, 140, Some(13)),
        node(20, 105, Some(1)), // an unrelated sibling of the root
    ];

    let plan: Vec<u32> = plan_tree(root, &nodes, TREE_LIMIT)
        .iter()
        .map(|id| id.pid)
        .collect();

    assert_eq!(plan, vec![14, 13, 11, 12]);
}

#[test]
fn a_child_older_than_its_claimed_parent_is_not_its_child() {
    // Windows never rewrites a parent PID. Process 30 was started by an
    // earlier holder of PID 10, which exited; PID 10 was then recycled for
    // the root. 30 is older than the root, so it is not the root's child.
    let root = ProcessInstanceId::new(10, 500);
    let nodes = [node(10, 500, Some(1)), node(30, 200, Some(10))];

    assert!(plan_tree(root, &nodes, TREE_LIMIT).is_empty());
}

#[test]
fn a_cyclic_parent_table_terminates() {
    let root = ProcessInstanceId::new(10, 100);
    let nodes = [
        node(10, 100, Some(11)),
        node(11, 100, Some(10)),
        node(12, 100, Some(12)),
    ];

    let plan = plan_tree(root, &nodes, TREE_LIMIT);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].pid, 11);
}

#[test]
fn the_plan_is_bounded() {
    let root = ProcessInstanceId::new(1, 1);
    let nodes: Vec<ProcessNode> = (2..10_000).map(|pid| node(pid, 2, Some(1))).collect();

    assert_eq!(plan_tree(root, &nodes, 50).len(), 50);
}

fn tree_world() -> FakeBackend {
    FakeBackend::with(vec![
        (1, FakeProcess::new(1)),
        (10, FakeProcess::new(100).child_of(1)),
        (11, FakeProcess::new(110).child_of(10)),
        (12, FakeProcess::new(120).child_of(11)),
        (20, FakeProcess::new(105).child_of(1)),
    ])
}

#[test]
fn ending_a_tree_ends_children_first_and_the_root_last() {
    let backend = tree_world();
    let result = backend
        .service()
        .terminate_tree(ProcessInstanceId::new(10, 100));

    assert!(result.is_success(), "{result:?}");
    let ended: Vec<u32> = backend
        .actions()
        .iter()
        .filter_map(|call| match call {
            Call::Terminate(pid, _) => Some(*pid),
            _ => None,
        })
        .collect();
    assert_eq!(ended, vec![12, 11, 10], "grandchild, child, parent");

    let summary = result.tree.expect("a tree summary");
    assert_eq!(summary.requested, 3);
    assert_eq!(summary.terminated, 3);
    assert!(summary.is_consistent());

    let world = backend.world.lock().unwrap();
    assert!(
        world.processes.contains_key(&20),
        "the sibling is untouched"
    );
    assert!(
        world.processes.contains_key(&1),
        "the parent of the root too"
    );
}

#[test]
fn every_tree_member_is_revalidated_and_a_recycled_one_is_skipped() {
    let backend = tree_world();
    // After planning, child 11 exits and its PID is recycled; grandchild 12
    // exits on its own.
    backend.world.lock().unwrap().after_plan =
        vec![(11, Some(FakeProcess::new(999).child_of(1))), (12, None)];

    let result = backend
        .service()
        .terminate_tree(ProcessInstanceId::new(10, 100));

    let summary = result.tree.expect("a tree summary");
    assert_eq!(summary.requested, 3);
    assert_eq!(summary.stale_skipped, 1);
    assert_eq!(summary.already_gone, 1);
    assert_eq!(summary.terminated, 1);
    assert!(summary.is_consistent());
    assert_eq!(
        backend.actions(),
        vec![Call::Terminate(10, TerminateMode::Graceful)],
        "the recycled PID 11 must never be signalled"
    );
    // Already-exited members are the outcome the user wanted, not a failure.
    assert_eq!(result.status, ProcessActionStatus::Success);
}

#[test]
fn a_refused_child_makes_the_tree_a_partial_failure() {
    let backend = tree_world();
    backend
        .world
        .lock()
        .unwrap()
        .processes
        .get_mut(&12)
        .unwrap()
        .deny = true;

    let result = backend
        .service()
        .terminate_tree(ProcessInstanceId::new(10, 100));

    assert_eq!(result.status, ProcessActionStatus::PartialFailure);
    let summary = result.tree.expect("summary");
    assert_eq!(summary.permission_denied, 1);
    assert_eq!(summary.terminated, 2);
    assert_eq!(result.failed_count, Some(1));
}

#[test]
fn a_stale_root_ends_nothing_at_all() {
    let backend = tree_world();
    let result = backend
        .service()
        .terminate_tree(ProcessInstanceId::new(10, 1));

    assert_eq!(result.status, ProcessActionStatus::StaleProcess);
    assert!(backend.actions().is_empty());
}

#[test]
fn pulse_is_never_ended_as_a_side_effect_of_a_tree() {
    let backend = FakeBackend::with(vec![
        (10, FakeProcess::new(100)),
        (999_999, FakeProcess::new(110).child_of(10)),
    ]);
    let result = backend
        .service()
        .terminate_tree(ProcessInstanceId::new(10, 100));

    let summary = result.tree.expect("summary");
    assert_eq!(summary.skipped_self, 1);
    assert!(!backend
        .actions()
        .contains(&Call::Terminate(999_999, TerminateMode::Graceful)));
}
