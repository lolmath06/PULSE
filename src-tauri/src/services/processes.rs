//! Application-level access to the process snapshot service.
//!
//! The composition point for the *other* half of PULSE's process support: the
//! platform layer decides whether this host has a process collector, and
//! [`ProcessSnapshotService`] knows how to turn one into rows, rates and
//! applications. Neither depends on the other.
//!
//! Note what is **not** here: no path through this module reaches the metrics
//! engine, and no path through `services::metrics` reaches a process row. The
//! two are separate because their lifetimes are opposite — a metric reference
//! is a promise kept for months, a process is gone in seconds. See
//! `docs/metrics/processes.md`.

use crate::platform;
use crate::processes::{
    ProcessControlService, ProcessInspectorService, ProcessSnapshot, ProcessSnapshotService,
};

/// Builds the process snapshot service PULSE runs with.
///
/// Takes a baseline immediately, so the user's first *Refresh* already has a
/// real interval to divide by. The first snapshot still reports rates as
/// waiting, honestly, rather than inventing zeros.
pub fn build_service() -> ProcessSnapshotService {
    ProcessSnapshotService::new(platform::host().process_collector())
}

/// Builds the service that acts on processes, and only when asked.
///
/// Separate from the snapshot service by construction: the snapshot service
/// holds no reference to it, and nothing in the metrics engine does either.
pub fn build_control() -> ProcessControlService {
    ProcessControlService::new(platform::host().process_control())
}

/// Builds the lazy, per-process inspector.
///
/// It shares the control service's suspension ledger — read-only — so it can
/// say whether *Resume* applies to what it is showing.
pub fn build_inspector(control: &ProcessControlService) -> ProcessInspectorService {
    ProcessInspectorService::new(
        platform::host().process_inspector(),
        control.ledger(),
        control.self_pid(),
        control.support(),
    )
}

/// Walks the process table once.
pub fn snapshot(service: &ProcessSnapshotService) -> ProcessSnapshot {
    service.snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::PlatformKind;

    #[test]
    fn the_host_platform_decides_whether_processes_can_be_read() {
        let snapshot = snapshot(&build_service());

        if PlatformKind::current().is_supported() {
            assert!(
                snapshot.unsupported_reason.is_none(),
                "both first-class platforms have a process collector"
            );
            assert!(snapshot.counts.total > 0);
            assert_eq!(snapshot.counts.total as usize, snapshot.processes.len());
        } else {
            assert!(snapshot.unsupported_reason.is_some());
            assert!(snapshot.processes.is_empty());
        }
    }

    #[test]
    fn inspecting_and_controlling_never_register_anything_in_the_metric_catalog() {
        let engine = crate::services::metrics::build_engine();
        let before = engine.status().metric_count;

        let control = build_control();
        let inspector = build_inspector(&control);
        let me = crate::processes::ProcessInstanceId::new(std::process::id(), 0);
        let _ = inspector.details(me);
        let _ = control.priority(me);

        assert_eq!(engine.status().metric_count, before);
        if PlatformKind::current().is_supported() {
            assert_eq!(
                engine.status().provider_count,
                6,
                "inspector and control are not providers"
            );
        }
    }

    #[test]
    fn inspection_control_and_provenance_contain_no_network_client() {
        // The inspector, hashing, provenance and control code must never talk
        // to the network. Only the command layer may hand a URL to the
        // browser, and only on an explicit click. This scans the sources, so
        // adding a socket or an HTTP client here fails the build's tests.
        let sources = [
            include_str!("../processes/inspector.rs"),
            include_str!("../processes/control.rs"),
            include_str!("../processes/hash.rs"),
            include_str!("../processes/search.rs"),
            include_str!("../platform/linux/processes/control.rs"),
            include_str!("../platform/linux/processes/rpm.rs"),
            include_str!("../platform/windows/processes/control.rs"),
            include_str!("../platform/windows/processes/authenticode.rs"),
            include_str!("../platform/windows/processes/version.rs"),
        ];
        for source in sources {
            for forbidden in [
                "std::net",
                "TcpStream",
                "UdpSocket",
                "reqwest",
                "hyper::",
                "ureq",
                "WinHttp",
                "URLDownloadToFile",
                "InternetOpen",
                "open_url(",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "process inspection/control code must not use '{forbidden}'"
                );
            }
        }
    }

    #[test]
    fn both_first_class_platforms_can_inspect_and_control() {
        if PlatformKind::current().is_supported() {
            assert!(platform::host().process_inspector().is_some());
            assert!(platform::host().process_control().is_some());
        }
    }

    #[test]
    fn a_snapshot_never_registers_anything_in_the_metric_catalog() {
        // The architectural property of this phase, asserted rather than
        // merely documented: taking process snapshots must not grow the
        // catalog by a single definition.
        let engine = crate::services::metrics::build_engine();
        let before = engine.status().metric_count;

        let service = build_service();
        service.snapshot();
        service.snapshot();

        assert_eq!(engine.status().metric_count, before);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_second_snapshot_produces_real_rates_for_at_least_one_process() {
        let service = build_service();
        service.snapshot();
        let snapshot = service.snapshot();

        assert!(
            snapshot
                .processes
                .iter()
                .any(|process| process.cpu_percent.value.is_some()),
            "after a baseline, at least one process must have a measurable CPU share"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn every_cpu_share_is_a_share_of_the_whole_machine() {
        let service = build_service();
        service.snapshot();
        let snapshot = service.snapshot();

        let total: f64 = snapshot
            .processes
            .iter()
            .filter_map(|process| process.cpu_percent.value)
            .sum();

        for process in &snapshot.processes {
            if let Some(cpu) = process.cpu_percent.value {
                assert!(
                    (0.0..=100.0).contains(&cpu),
                    "{} reported {cpu} %, which is not a share of the machine",
                    process.name
                );
            }
        }

        // The convention's whole purpose: the column adds up to something a
        // system gauge could plausibly show, not to 3200.
        assert!(
            total <= 100.0 * 1.5,
            "the process CPU column summed to {total} %, which no machine-wide gauge could match"
        );
    }
}
