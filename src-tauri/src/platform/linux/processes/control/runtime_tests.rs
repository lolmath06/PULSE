//! The safe runtime harness: real control calls, against processes these
//! tests created themselves, and nothing else.
//!
//! Every target here is a `sleep` this test process spawned. An unreaped
//! child's PID cannot be recycled, so for as long as the [`Owned`] guard holds
//! it, the PID provably names that child. The guard kills and reaps it on drop,
//! so a failing assertion never leaves a process behind.
//!
//! No test in this file names a PID it did not spawn.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::processes::control::{ProcessAffinity, ProcessControlService, ProcessPriority};
use crate::processes::control::{SuspensionLedger, TerminateMode};
use crate::processes::hash::HashStatus;
use crate::processes::inspector::ProcessInspectorService;
use crate::processes::{ProcessActionStatus, ProcessClass, ProcessInstanceId};

use super::super::stat;
use super::{control_backend, inspector_backend};

/// A child process this test owns, killed and reaped on drop.
struct Owned {
    child: Child,
}

impl Owned {
    fn sleep() -> Self {
        let child = Command::new("sleep")
            .arg("60")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sleep");
        let owned = Self { child };
        // `spawn` can return while the child is still the forked copy of this
        // test binary. Wait until it has really become `sleep`, so every test
        // inspects the program it thinks it does.
        let pid = owned.pid();
        assert!(
            wait_until(|| std::fs::read_link(format!("/proc/{pid}/exe"))
                .is_ok_and(|path| path.file_name().is_some_and(|name| name == "sleep"))),
            "the child exec'd into sleep"
        );
        owned
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn instance(&self) -> ProcessInstanceId {
        ProcessInstanceId::new(
            self.pid(),
            token(self.pid()).expect("a live child has a token"),
        )
    }

    fn state(&self) -> Option<char> {
        read(self.pid()).map(|stat| stat.state)
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // Only ever our own, unreaped child.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read(pid: u32) -> Option<stat::ProcStat> {
    stat::parse(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

fn token(pid: u32) -> Option<u64> {
    read(pid).map(|stat| stat.start_ticks)
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    condition()
}

fn control() -> ProcessControlService {
    ProcessControlService::new(Some(control_backend()))
}

fn inspector(control: &ProcessControlService) -> ProcessInspectorService {
    ProcessInspectorService::new(
        Some(inspector_backend()),
        control.ledger(),
        std::process::id(),
        control.support(),
    )
}

#[test]
fn inspects_an_owned_child() {
    let child = Owned::sleep();
    let control = control();

    let details = inspector(&control)
        .details(child.instance())
        .value
        .expect("details of our own child");

    assert_eq!(details.pid, child.pid());
    assert_eq!(details.parent_pid, Some(std::process::id()));
    assert_eq!(details.category, ProcessClass::UserApplication);
    assert!(details.started_at.value.is_some());
    // SAFETY: `geteuid` has no arguments and no error path.
    let uid = unsafe { libc::geteuid() };
    assert_eq!(
        details.owner.value.as_ref().map(|owner| owner.id.clone()),
        Some(uid.to_string())
    );
    let executable = details
        .executable
        .value
        .expect("our own child's executable");
    assert_eq!(executable.file_name, "sleep");
    assert!(executable.size_bytes.value.is_some_and(|size| size > 0));
    assert_eq!(
        details.architecture.value.as_deref(),
        Some(std::env::consts::ARCH)
    );
    assert!(matches!(
        details.priority.value,
        Some(ProcessPriority::Nice { .. })
    ));
    assert!(details
        .affinity
        .value
        .is_some_and(|affinity| !affinity.cpus.is_empty()));
    assert!(details.capabilities.terminate.allowed);
    assert!(details.capabilities.suspend.allowed);
    assert!(!details.capabilities.resume.allowed);
}

#[test]
fn hashes_an_owned_childs_executable() {
    use sha2::{Digest, Sha256};

    let child = Owned::sleep();
    let control = control();
    let hash = inspector(&control)
        .sha256(child.instance())
        .value
        .expect("a hash outcome");

    assert_eq!(hash.status, HashStatus::Computed);

    let path = std::fs::read_link(format!("/proc/{}/exe", child.pid())).expect("link");
    let expected = Sha256::digest(std::fs::read(path).expect("read sleep"));
    let expected: String = expected.iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(hash.sha256.as_deref(), Some(expected.as_str()));
}

#[test]
fn a_wrong_start_token_is_stale_and_the_child_survives() {
    let child = Owned::sleep();
    let control = control();
    let wrong = ProcessInstanceId::new(child.pid(), child.instance().start_token + 1);

    for result in [
        control.terminate(wrong, TerminateMode::Force),
        control.suspend(wrong),
        control.set_priority(wrong, ProcessPriority::Nice { value: 19 }, false),
        control.set_affinity(wrong, &[0]),
    ] {
        assert_eq!(result.status, ProcessActionStatus::StaleProcess);
    }
    assert_eq!(
        inspector(&control).details(wrong).outcome.status,
        ProcessActionStatus::StaleProcess
    );

    assert!(token(child.pid()).is_some(), "the child was not touched");
    assert_ne!(child.state(), Some('T'));
}

#[test]
fn suspends_and_resumes_an_owned_child() {
    let child = Owned::sleep();
    let control = control();
    let id = child.instance();

    assert!(control.suspend(id).is_success());
    assert!(wait_until(|| child.state() == Some('T')), "stopped");
    assert!(
        inspector(&control)
            .details(id)
            .value
            .expect("details")
            .capabilities
            .resume
            .allowed
    );

    assert!(control.resume(id).is_success());
    assert!(wait_until(|| child.state() != Some('T')), "running again");
    assert!(!SuspensionLedger::contains(&control.ledger(), &id));
}

#[test]
fn a_child_stopped_outside_pulse_is_not_claimed() {
    let child = Owned::sleep();
    // SAFETY: signalling our own unreaped child, whose PID cannot be recycled.
    unsafe { libc::kill(child.pid() as libc::pid_t, libc::SIGSTOP) };
    assert!(wait_until(|| child.state() == Some('T')));

    let control = control();
    let id = child.instance();
    assert_eq!(
        control.suspend(id).status,
        ProcessActionStatus::InvalidRequest
    );
    assert_eq!(
        control.resume(id).status,
        ProcessActionStatus::InvalidRequest
    );
    assert_eq!(child.state(), Some('T'), "PULSE did not resume it");
}

#[test]
fn lowers_an_owned_childs_priority() {
    let child = Owned::sleep();
    let control = control();
    let id = child.instance();

    let result = control.set_priority(id, ProcessPriority::Nice { value: 10 }, false);
    assert!(result.is_success(), "{result:?}");
    assert_eq!(
        control.priority(id).expect("read back"),
        ProcessPriority::Nice { value: 10 }
    );

    // Raising it again below the original needs privileges PULSE lacks,
    // unless the tests run as root.
    // SAFETY: `geteuid` has no arguments and no error path.
    if unsafe { libc::geteuid() } != 0 {
        let raised = control.set_priority(id, ProcessPriority::Nice { value: -5 }, false);
        assert_eq!(raised.status, ProcessActionStatus::PermissionDenied);
    }
}

#[test]
fn pins_an_owned_child_to_one_processor() {
    let child = Owned::sleep();
    let control = control();
    let id = child.instance();

    let ProcessAffinity { cpus, .. } = control.affinity(id).expect("affinity");
    let first = cpus[0];

    let result = control.set_affinity(id, &[first]);
    assert!(result.is_success(), "{result:?}");
    assert_eq!(control.affinity(id).expect("read back").cpus, vec![first]);

    assert_eq!(
        control.set_affinity(id, &[]).status,
        ProcessActionStatus::InvalidRequest
    );
    assert_eq!(
        control.set_affinity(id, &[u32::MAX]).status,
        ProcessActionStatus::InvalidRequest
    );
}

#[test]
fn terminates_an_owned_child_with_sigterm() {
    use std::os::unix::process::ExitStatusExt;

    let mut child = Owned::sleep();
    let control = control();
    let id = child.instance();

    assert!(control.terminate(id, TerminateMode::Graceful).is_success());
    let status = child.child.wait().expect("reap");
    assert_eq!(status.signal(), Some(libc::SIGTERM));

    // Reaped: the instance is gone, whatever the PID holds now.
    let again = control.terminate(id, TerminateMode::Graceful);
    assert!(matches!(
        again.status,
        ProcessActionStatus::ProcessGone | ProcessActionStatus::StaleProcess
    ));
}

#[test]
fn ends_an_owned_tree_children_first_and_spares_a_sibling() {
    // parent sh → child sh → grandchild sleep, plus a second direct sleep.
    // Both shells explicitly wait on background children, so neither can
    // exec into `sleep` and the three-level hierarchy stays in place. A fixed
    // literal script — nothing is interpolated.
    let mut root = Owned {
        child: Command::new("sh")
            .args(["-c", "sh -c 'sleep 60 & wait' & sleep 60 & wait"])
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn tree"),
    };
    let sibling = Owned::sleep();
    let root_pid = root.pid();

    let children_of = |processes: &[crate::processes::RawProcess], pid: u32| -> Vec<u32> {
        processes
            .iter()
            .filter(|process| process.parent_pid == Some(pid))
            .map(|process| process.instance.pid)
            .collect()
    };
    let mut tree = None;
    assert!(
        wait_until(|| {
            let processes = crate::platform::linux::processes::scan(
                crate::platform::linux::processes::Depth::Counts,
            )
            .processes;
            let children = children_of(&processes, root_pid);
            let grandchildren: Vec<u32> = children
                .iter()
                .flat_map(|pid| children_of(&processes, *pid))
                .collect();

            if children.len() == 2 && grandchildren.len() == 1 {
                let mut observed = children;
                observed.extend(grandchildren);
                tree = Some(observed);
                true
            } else {
                false
            }
        }),
        "the three-level tree formed"
    );
    let tree = tree.expect("the complete tree was observed in one snapshot");

    let control = control();
    let result = control.terminate_tree(root.instance());
    let summary = result.tree.expect("a tree summary");

    assert!(result.is_success(), "{result:?}");
    assert_eq!(summary.requested, 4, "root, two children, one grandchild");
    assert_eq!(summary.terminated, 4);
    assert!(summary.is_consistent());

    root.child.wait().expect("reap the root");
    for pid in tree {
        assert!(
            wait_until(|| read(pid).map_or(true, |stat| stat.state == 'Z')),
            "descendant {pid} ended"
        );
    }
    assert!(token(sibling.pid()).is_some(), "the sibling is untouched");
    assert_ne!(sibling.state(), Some('T'));
}

#[test]
fn pulse_itself_reports_no_suspend_and_is_marked_as_self() {
    let me = std::process::id();
    let id = ProcessInstanceId::new(me, token(me).expect("self"));
    let control = control();

    let details = inspector(&control).details(id).value.expect("self details");
    assert!(details.is_self);
    assert!(!details.capabilities.suspend.allowed);
    // Read-only checks only: this test never acts on itself.
}

#[test]
fn a_kernel_thread_degrades_cleanly() {
    // PID 2 is kthreadd on every Linux system PULSE supports. Read-only.
    let Some(kthreadd) = token(2) else {
        return;
    };
    let control = control();
    let details = inspector(&control)
        .details(ProcessInstanceId::new(2, kthreadd))
        .value
        .expect("kthreadd details");

    assert_eq!(details.category, ProcessClass::KernelThread);
    assert!(details.executable.value.is_none());
    assert!(details.architecture.value.is_none());
    assert!(!details.capabilities.terminate.allowed);
    assert!(!details.capabilities.open_location.allowed);
    assert!(!details.capabilities.compute_hash.allowed);
}
