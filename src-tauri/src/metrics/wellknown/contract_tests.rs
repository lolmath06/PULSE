//! Cross-platform contract tests.
//!
//! These guard the single promise that makes a PULSE dashboard portable:
//!
//! > A widget configured against `memory.used@memory:system` on Fedora keeps
//! > working, unchanged, on Windows.
//!
//! They compile and run on **every** platform, because both platform modules
//! are built everywhere. A Fedora CI run therefore catches a Windows
//! declaration drifting, and vice versa.

use crate::metrics::model::{MetricDefinition, ProviderId};
use crate::metrics::wellknown::{cpu, memory};

/// Every metric reference PULSE ships, in catalog order (`key`, then `sourceId`).
fn expected_references() -> Vec<String> {
    vec![
        "cpu.usage.total@cpu:system".to_string(),
        "memory.available@memory:system".to_string(),
        "memory.total@memory:system".to_string(),
        "memory.usage.percent@memory:system".to_string(),
        "memory.used@memory:system".to_string(),
    ]
}

/// The declarations a platform's two providers produce, sorted like the catalog.
fn declarations(cpu_provider: &str, memory_provider: &str) -> Vec<MetricDefinition> {
    let cpu_id = ProviderId::new(cpu_provider).expect("valid provider id");
    let memory_id = ProviderId::new(memory_provider).expect("valid provider id");

    let mut definitions = cpu::definitions(&cpu_id);
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
fn both_platforms_declare_exactly_five_metrics() {
    assert_eq!(linux_declarations().len(), 5);
    assert_eq!(windows_declarations().len(), 5);
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
        assert_eq!(
            linux.availability, windows.availability,
            "availability for {reference}"
        );

        // The one permitted difference.
        assert_ne!(
            linux.provider_id, windows.provider_id,
            "provider id should identify the platform for {reference}"
        );
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
fn user_facing_text_never_names_an_operating_system() {
    // A widget's label must read the same whichever machine it runs on.
    for definition in linux_declarations()
        .iter()
        .chain(windows_declarations().iter())
    {
        for text in [&definition.display_name, &definition.source_label] {
            let lowered = text.to_lowercase();
            for forbidden in ["linux", "windows", "fedora", "/proc", "win32"] {
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
        assert_eq!(source.instance(), "system");
    }
}
