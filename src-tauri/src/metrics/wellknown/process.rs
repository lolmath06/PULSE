//! The `process.*` metrics: three numbers about the machine, and no more.
//!
//! # What is deliberately absent
//!
//! There is **no** `process.cpu@process:1234-9001` here, and there never will
//! be. A desktop runs three to five hundred processes; most live for less than
//! a second. Declaring six metrics for each would mean:
//!
//! ```text
//! ~350 processes × 6 metrics   ≈ 2 100 MetricDefinitions
//! replaced wholesale           every single refresh
//! ```
//!
//! and the catalog is the wrong container for that. A [`MetricDefinition`] is
//! a *promise*: a dashboard saves `cpu.usage.total@cpu:system` today and
//! expects it to resolve in six months. `process:1234-9001` stops resolving
//! the moment that process exits, which for most of them is before the user
//! has finished reading the row.
//!
//! So the engine gets the three figures that describe the machine and are
//! stable references forever, and everything per-process goes through
//! [`ProcessSnapshotService`] and its own command. The full argument is in
//! `docs/metrics/processes.md`.
//!
//! [`MetricDefinition`]: crate::metrics::model::MetricDefinition
//! [`ProcessSnapshotService`]: crate::processes::ProcessSnapshotService

use crate::metrics::model::{
    Availability, MetricCategory, MetricDefinition, MetricDefinitionBuilder, MetricKey, MetricKind,
    MetricRef, MetricUnit, ProviderId, SourceId,
};

/// `process.count.total` — how many processes exist right now.
pub const COUNT_TOTAL: &str = "process.count.total";
/// `process.count.running` — how many are on a processor or queued for one.
pub const COUNT_RUNNING: &str = "process.count.running";
/// `process.thread.count.total` — every thread of every visible process.
pub const THREAD_COUNT_TOTAL: &str = "process.thread.count.total";

/// The three keys this provider owns, in declaration order.
pub const MACHINE_KEYS: &[&str] = &[COUNT_TOTAL, COUNT_RUNNING, THREAD_COUNT_TOTAL];

/// The machine-wide process source.
///
/// A *logical* identifier meaning "this machine's processes taken together",
/// stable by construction and identical on both platforms — like `cpu:system`
/// and `network:system`. It is the **only** `process:` source PULSE ever
/// registers.
pub const SOURCE: &str = "process:system";

/// The user-facing label for `process:system`.
const SYSTEM_LABEL: &str = "Processes";

fn key(name: &str) -> MetricKey {
    MetricKey::new(name).expect("well-known process key must be valid")
}

fn system_source() -> SourceId {
    SourceId::new(SOURCE).expect("well-known process source must be valid")
}

/// Builds a reference for one of the three machine-wide keys.
pub fn process_ref(name: &str) -> MetricRef {
    MetricRef::new(key(name), system_source())
}

/// Builds the `process.count.total` reference.
pub fn count_total_ref() -> MetricRef {
    process_ref(COUNT_TOTAL)
}

/// Builds the `process.count.running` reference.
pub fn count_running_ref() -> MetricRef {
    process_ref(COUNT_RUNNING)
}

/// Builds the `process.thread.count.total` reference.
pub fn thread_count_total_ref() -> MetricRef {
    process_ref(THREAD_COUNT_TOTAL)
}

/// Declares every machine-wide process metric.
///
/// Both `linux.processes` and `windows.processes` call this, so the two
/// declarations differ only in `providerId` and in the availability each
/// platform genuinely discovered.
pub fn definitions(provider: &ProviderId, availability: Availability) -> Vec<MetricDefinition> {
    let build = |reference: MetricRef, display: &str, description: &str, kind: MetricKind| {
        MetricDefinitionBuilder::new(
            reference,
            provider.clone(),
            MetricCategory::Process,
            MetricUnit::Count,
            kind,
        )
        .source_label(SYSTEM_LABEL)
        .display_name(display)
        .description(description)
        .availability(availability.clone())
        .build()
    };

    let mut definitions = vec![
        build(
            count_total_ref(),
            "Processes",
            "How many processes exist on this machine right now, including the ones PULSE \
             cannot read the details of. A process that is refused still exists and is still \
             counted.",
            MetricKind::Gauge,
        ),
        build(
            count_running_ref(),
            "Running",
            "How many processes are executing on a processor or queued and ready to. Nearly \
             every process on an idle desktop is waiting instead — for a timer, for input, \
             for a disk — so a small number here is normal and not a sign of an idle machine. \
             Windows reports no process-level state, so this figure is unavailable there \
             rather than fabricated.",
            MetricKind::Gauge,
        ),
        build(
            thread_count_total_ref(),
            "Threads",
            "Every thread of every visible process, added up. Processes whose thread count \
             could not be read contribute nothing rather than a guess, so on a machine full \
             of protected processes this is an understatement rather than an invention.",
            MetricKind::Gauge,
        ),
    ];

    definitions.sort_by(|left, right| left.metric.cmp(&right.metric));
    definitions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::model::MetricValueType;

    fn provider() -> ProviderId {
        ProviderId::new("linux.processes").expect("valid")
    }

    #[test]
    fn declares_exactly_three_metrics_on_one_source() {
        let definitions = definitions(&provider(), Availability::Available);

        assert_eq!(definitions.len(), 3);
        assert_eq!(definitions.len(), MACHINE_KEYS.len());
        for definition in &definitions {
            assert_eq!(definition.metric.source_id.as_str(), SOURCE);
            assert_eq!(definition.category, MetricCategory::Process);
            assert_eq!(definition.unit, MetricUnit::Count);
            assert_eq!(definition.kind, MetricKind::Gauge);
            assert_eq!(definition.value_type, MetricValueType::Number);
            assert_eq!(definition.provider_id, provider());
            assert!(!definition.description.is_empty());
        }
    }

    #[test]
    fn declares_no_metric_for_an_individual_process() {
        // The property this whole module exists to hold. A per-PID source
        // here would mean a catalog that churns thousands of definitions.
        let definitions = definitions(&provider(), Availability::Available);

        for definition in &definitions {
            assert_eq!(
                definition.metric.source_id.instance(),
                "system",
                "the only registered process source is the machine itself"
            );
        }
    }

    #[test]
    fn the_source_obeys_the_identifier_grammar() {
        let source = system_source();
        assert_eq!(source.kind(), "process");
        assert!(source.has_canonical_kind());
    }

    #[test]
    fn declarations_are_sorted_like_the_catalog() {
        let definitions = definitions(&provider(), Availability::Available);
        let mut sorted = definitions.clone();
        sorted.sort_by(|left, right| left.metric.cmp(&right.metric));
        assert_eq!(definitions, sorted);
    }

    #[test]
    fn a_platform_that_cannot_read_processes_still_declares_them_with_a_reason() {
        let reason = Availability::permission_denied("the process table is not readable");
        let definitions = definitions(&provider(), reason.clone());

        assert_eq!(definitions.len(), 3);
        for definition in &definitions {
            assert_eq!(definition.availability, reason);
        }
    }

    #[test]
    fn both_platforms_declare_the_same_references() {
        let linux = definitions(&provider(), Availability::Available);
        let windows = definitions(
            &ProviderId::new("windows.processes").expect("valid"),
            Availability::Available,
        );

        let references: Vec<_> = linux.iter().map(|d| d.metric.clone()).collect();
        let others: Vec<_> = windows.iter().map(|d| d.metric.clone()).collect();
        assert_eq!(references, others);
    }

    #[test]
    fn the_reference_helpers_agree_with_the_keys() {
        assert_eq!(count_total_ref().key.as_str(), COUNT_TOTAL);
        assert_eq!(count_running_ref().key.as_str(), COUNT_RUNNING);
        assert_eq!(thread_count_total_ref().key.as_str(), THREAD_COUNT_TOTAL);
    }
}
