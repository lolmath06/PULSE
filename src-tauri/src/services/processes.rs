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
use crate::processes::{ProcessSnapshot, ProcessSnapshotService};

/// Builds the process snapshot service PULSE runs with.
///
/// Takes a baseline immediately, so the user's first *Refresh* already has a
/// real interval to divide by. The first snapshot still reports rates as
/// waiting, honestly, rather than inventing zeros.
pub fn build_service() -> ProcessSnapshotService {
    ProcessSnapshotService::new(platform::host().process_collector())
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
