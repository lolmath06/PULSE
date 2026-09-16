//! Cross-platform contract tests.
//!
//! These guard the single promise that makes a PULSE dashboard portable:
//!
//! > A widget configured against `memory.used@memory:system` or
//! > `cpu.usage.logical@cpu:logical-3` on Fedora keeps working, unchanged, on
//! > Windows.
//!
//! They compile and run on **every** platform, because both platform modules
//! are built everywhere. A Fedora CI run therefore catches a Windows
//! declaration drifting, and vice versa.
//!
//! The CPU declarations are driven from a **synthetic topology** rather than
//! from the host, so the comparison is between what the two platforms would
//! publish for the *same* machine. Reading the real CPU would compare a
//! 32-processor Fedora laptop against a Windows machine that does not exist,
//! and would prove nothing.

use crate::metrics::model::{Availability, MetricDefinition, ProviderId};
use crate::metrics::wellknown::cpu::{self, CpuTopology, LogicalId, LogicalProcessor};
use crate::metrics::wellknown::memory;

/// The topology both platforms are asked to describe: four logical processors
/// on two physical cores in one package — a plain SMT dual-core.
fn synthetic_topology() -> CpuTopology {
    CpuTopology::new(
        (0..4)
            .map(|ordinal| LogicalProcessor::available(LogicalId::new(ordinal)))
            .collect(),
        Some(2),
        Some(1),
    )
}

/// Every metric reference PULSE ships for [`synthetic_topology`], in catalog
/// order (`key`, then `sourceId`).
fn expected_references() -> Vec<String> {
    let mut references = vec![
        "cpu.count.logical@cpu:system".to_string(),
        "cpu.count.package@cpu:system".to_string(),
        "cpu.count.physical@cpu:system".to_string(),
    ];

    for ordinal in 0..4 {
        references.push(format!("cpu.frequency.current@cpu:logical-{ordinal}"));
    }
    for ordinal in 0..4 {
        references.push(format!("cpu.frequency.max@cpu:logical-{ordinal}"));
    }
    for ordinal in 0..4 {
        references.push(format!("cpu.usage.logical@cpu:logical-{ordinal}"));
    }

    references.push("cpu.usage.total@cpu:system".to_string());
    references.extend(
        [
            "memory.available@memory:system",
            "memory.total@memory:system",
            "memory.usage.percent@memory:system",
            "memory.used@memory:system",
        ]
        .map(String::from),
    );

    references
}

/// The declarations a platform's two providers produce, sorted like the catalog.
fn declarations(cpu_provider: &str, memory_provider: &str) -> Vec<MetricDefinition> {
    let cpu_id = ProviderId::new(cpu_provider).expect("valid provider id");
    let memory_id = ProviderId::new(memory_provider).expect("valid provider id");

    let mut definitions = cpu::definitions(&cpu_id, &synthetic_topology());
    definitions.extend(memory::definitions(&memory_id));
    definitions.sort_by(|left, right| left.metric.cmp(&right.metric));

    definitions
}

fn linux_declarations() -> Vec<MetricDefinition> {
    declarations(
        crate::platform::linux::cpu::PROVIDER_ID,
        crate::platform::linux::memory::PROVIDER_ID,
    )
}

fn windows_declarations() -> Vec<MetricDefinition> {
    declarations(
        crate::platform::windows::cpu::PROVIDER_ID,
        crate::platform::windows::memory::PROVIDER_ID,
    )
}

#[test]
fn both_platforms_declare_the_same_number_of_metrics_for_one_topology() {
    // 4 memory + 4 CPU system + 3 per logical processor.
    let expected = 4 + 4 + 3 * 4;

    assert_eq!(linux_declarations().len(), expected);
    assert_eq!(windows_declarations().len(), expected);
}

#[test]
fn both_platforms_declare_the_same_references() {
    let references = |definitions: Vec<MetricDefinition>| -> Vec<String> {
        definitions
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect()
    };

    assert_eq!(references(linux_declarations()), expected_references());
    assert_eq!(references(windows_declarations()), expected_references());
}

#[test]
fn only_the_provider_id_differs_between_the_platforms() {
    // The heart of cross-platform portability. Key, source, unit, kind, value
    // type and every user-facing string must match; only the attribution may
    // differ.
    for (linux, windows) in linux_declarations()
        .iter()
        .zip(windows_declarations().iter())
    {
        let reference = &linux.metric;

        assert_eq!(linux.metric, windows.metric, "metric ref for {reference}");
        assert_eq!(linux.unit, windows.unit, "unit for {reference}");
        assert_eq!(linux.kind, windows.kind, "kind for {reference}");
        assert_eq!(
            linux.value_type, windows.value_type,
            "value type for {reference}"
        );
        assert_eq!(linux.category, windows.category, "category for {reference}");
        assert_eq!(
            linux.display_name, windows.display_name,
            "display name for {reference}"
        );
        assert_eq!(
            linux.source_label, windows.source_label,
            "source label for {reference}"
        );
        assert_eq!(
            linux.description, windows.description,
            "description for {reference}"
        );

        // The one permitted difference.
        assert_ne!(
            linux.provider_id, windows.provider_id,
            "provider id should identify the platform for {reference}"
        );
    }
}

#[test]
fn availability_is_the_only_field_a_platform_may_decide_for_itself() {
    // The same machine may genuinely expose a frequency on one OS and not the
    // other; that must be expressible without changing the reference, the unit
    // or the label, so a dashboard keeps working either way.
    let cpu_id = ProviderId::new("linux.cpu").expect("valid");

    let permissive = cpu::definitions(&cpu_id, &synthetic_topology());
    let restricted = cpu::definitions(
        &cpu_id,
        &CpuTopology::new(
            (0..4)
                .map(|ordinal| {
                    LogicalProcessor::available(LogicalId::new(ordinal))
                        .with_frequencies(Availability::unsupported("no frequency interface"))
                })
                .collect(),
            Some(2),
            Some(1),
        ),
    );

    assert_eq!(permissive.len(), restricted.len());

    for (open, closed) in permissive.iter().zip(restricted.iter()) {
        assert_eq!(open.metric, closed.metric);
        assert_eq!(open.unit, closed.unit);
        assert_eq!(open.kind, closed.kind);
        assert_eq!(open.display_name, closed.display_name);
        assert_eq!(open.source_label, closed.source_label);
    }

    let unavailable = restricted
        .iter()
        .filter(|definition| !definition.availability.is_available())
        .count();
    assert_eq!(
        unavailable, 8,
        "both frequency metrics of all four processors"
    );
}

#[test]
fn the_declaration_order_is_deterministic_on_both_platforms() {
    for declarations in [linux_declarations(), windows_declarations()] {
        let references: Vec<String> = declarations
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect();

        let mut sorted = references.clone();
        sorted.sort();
        assert_eq!(references, sorted);

        let mut deduplicated = sorted.clone();
        deduplicated.dedup();
        assert_eq!(deduplicated.len(), references.len(), "duplicate reference");
    }
}

#[test]
fn the_catalog_size_follows_the_machine_identically_on_both_platforms() {
    // The dynamic-catalog promise: 8 + 3N, whatever N is, on either OS.
    for logical_count in [1_u32, 2, 8, 32, 64, 128] {
        let topology = CpuTopology::new(
            (0..logical_count)
                .map(|ordinal| LogicalProcessor::available(LogicalId::new(ordinal)))
                .collect(),
            Some(logical_count / 2),
            Some(1),
        );

        let sizes: Vec<usize> = ["linux.cpu", "windows.cpu"]
            .map(|id| {
                let provider = ProviderId::new(id).expect("valid");
                cpu::definitions(&provider, &topology).len() + 4
            })
            .to_vec();

        assert_eq!(sizes[0], sizes[1]);
        assert_eq!(sizes[0], (8 + 3 * logical_count) as usize);
    }
}

#[test]
fn provider_ids_follow_the_documented_convention() {
    assert_eq!(crate::platform::linux::cpu::PROVIDER_ID, "linux.cpu");
    assert_eq!(crate::platform::linux::memory::PROVIDER_ID, "linux.memory");
    assert_eq!(crate::platform::windows::cpu::PROVIDER_ID, "windows.cpu");
    assert_eq!(
        crate::platform::windows::memory::PROVIDER_ID,
        "windows.memory"
    );

    // All four must be valid identifiers, not just plausible strings.
    for id in [
        crate::platform::linux::cpu::PROVIDER_ID,
        crate::platform::linux::memory::PROVIDER_ID,
        crate::platform::windows::cpu::PROVIDER_ID,
        crate::platform::windows::memory::PROVIDER_ID,
    ] {
        assert!(ProviderId::new(id).is_ok(), "invalid provider id: {id}");
    }
}

#[test]
fn one_cpu_provider_owns_every_cpu_metric_on_both_platforms() {
    // Not one provider per processor: a 128-thread machine must still report
    // two providers, with a larger catalog.
    for declarations in [linux_declarations(), windows_declarations()] {
        let mut cpu_providers: Vec<&str> = declarations
            .iter()
            .filter(|definition| definition.metric.key.domain() == "cpu")
            .map(|definition| definition.provider_id.as_str())
            .collect();
        cpu_providers.sort_unstable();
        cpu_providers.dedup();

        assert_eq!(cpu_providers.len(), 1, "CPU metrics must have one owner");
    }
}

#[test]
fn user_facing_text_never_names_an_operating_system() {
    // A widget's label must read the same whichever machine it runs on.
    for definition in linux_declarations()
        .iter()
        .chain(windows_declarations().iter())
    {
        for text in [&definition.display_name, &definition.source_label] {
            let lowered = text.to_lowercase();
            for forbidden in ["linux", "windows", "fedora", "/proc", "/sys", "win32"] {
                assert!(
                    !lowered.contains(forbidden),
                    "user-facing text '{text}' leaks the platform term '{forbidden}'"
                );
            }
        }
    }
}

#[test]
fn the_logical_sources_are_stable_identifiers() {
    // `cpu:system` and `memory:system` are logical, not device-derived, so
    // they cannot shift with detection order the way `nvme0n1` or `card0` can.
    assert_eq!(cpu::SOURCE, "cpu:system");
    assert_eq!(memory::SOURCE, "memory:system");

    for definition in linux_declarations() {
        let source = &definition.metric.source_id;
        assert!(
            source.has_canonical_kind(),
            "{source} should use a canonical source kind"
        );

        // Either the machine-wide instance, or a numbered logical processor —
        // never anything derived from a product name or a bus address.
        let instance = source.instance();
        assert!(
            instance == "system" || LogicalId::from_source(source).is_some(),
            "unexpected source instance '{instance}'"
        );
    }
}

#[test]
fn logical_processor_sources_are_numbered_from_zero_without_gaps() {
    // What lets the UI discover the processor list from the catalog alone.
    let mut ordinals: Vec<u32> = linux_declarations()
        .iter()
        .filter_map(|definition| LogicalId::from_source(&definition.metric.source_id))
        .map(LogicalId::get)
        .collect();
    ordinals.sort_unstable();
    ordinals.dedup();

    assert_eq!(ordinals, [0, 1, 2, 3]);
}

#[test]
fn every_logical_processor_declares_all_three_of_its_metrics() {
    // A processor missing one of them would leave a hole in the CPU table.
    for declarations in [linux_declarations(), windows_declarations()] {
        for ordinal in 0..4 {
            let id = LogicalId::new(ordinal);
            for reference in [
                cpu::usage_logical_ref(id),
                cpu::frequency_current_ref(id),
                cpu::frequency_max_ref(id),
            ] {
                assert!(
                    declarations
                        .iter()
                        .any(|definition| definition.metric == reference),
                    "{reference} is missing"
                );
            }
        }
    }
}
