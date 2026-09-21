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
use crate::metrics::wellknown::cpu::{
    self, CpuPackage, CpuTopology, LogicalId, LogicalProcessor, PackageId,
};
use crate::metrics::wellknown::gpu::{
    self, GpuCapabilities, GpuDescriptor, GpuIdentity, GpuVendor, PciAddress,
};
use crate::metrics::wellknown::memory;

/// The NVIDIA card both platforms are asked to describe.
///
/// Identified by its NVML UUID, which is the whole point: the *same* physical
/// card yields the *same* `SourceId` on Fedora and on Windows, so a dashboard
/// widget bound to it survives moving between them.
const SYNTHETIC_UUID: &str = "GPU-11111111-2222-3333-4444-555555555555";

fn synthetic_gpus() -> Vec<GpuDescriptor> {
    vec![GpuDescriptor {
        source_id: gpu::nvml_source_id(SYNTHETIC_UUID).expect("valid"),
        display_name: "NVIDIA GeForce RTX 4070".to_string(),
        vendor: GpuVendor::Nvidia,
        identity: GpuIdentity::NvmlUuid(SYNTHETIC_UUID.to_string()),
        pci: Some(PciAddress::new(0, 1, 0, 0)),
        backend: "nvml",
        capabilities: GpuCapabilities::all_available(),
    }]
}

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
    .with_packages(vec![CpuPackage::available(PackageId::new(0))])
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
    references.push("cpu.temperature.package@cpu:package-0".to_string());

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

/// The GPU declarations a platform's GPU provider produces for one synthetic
/// NVIDIA card, sorted like the catalog.
fn gpu_declarations(gpu_provider: &str) -> Vec<MetricDefinition> {
    let id = ProviderId::new(gpu_provider).expect("valid provider id");
    let mut definitions = gpu::definitions(&id, &synthetic_gpus());
    definitions.sort_by(|left, right| left.metric.cmp(&right.metric));

    definitions
}

fn linux_gpu_declarations() -> Vec<MetricDefinition> {
    gpu_declarations(crate::platform::linux::gpu::PROVIDER_ID)
}

fn windows_gpu_declarations() -> Vec<MetricDefinition> {
    gpu_declarations(crate::platform::windows::gpu::PROVIDER_ID)
}

// --- GPU contract ---------------------------------------------------------

#[test]
fn both_platforms_declare_the_same_gpu_references_for_one_card() {
    let references = |definitions: Vec<MetricDefinition>| -> Vec<String> {
        definitions
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect()
    };

    const CARD: &str = "gpu:nvidia-11111111-2222-3333-4444-555555555555";

    let mut expected = vec!["gpu.count@gpu:system".to_string()];
    expected.extend(
        [
            "gpu.fan.speed",
            "gpu.frequency.core",
            "gpu.frequency.memory",
            "gpu.memory.free",
            "gpu.memory.total",
            "gpu.memory.usage.percent",
            "gpu.memory.used",
            "gpu.temperature.core",
            "gpu.temperature.hotspot",
            "gpu.temperature.memory",
            "gpu.usage.core",
        ]
        .map(|key| format!("{key}@{CARD}")),
    );

    assert_eq!(references(linux_gpu_declarations()), expected);
    assert_eq!(references(windows_gpu_declarations()), expected);
}

#[test]
fn an_nvidia_card_keeps_one_identity_across_operating_systems() {
    // The portability promise for GPUs: a hardware UUID is a hardware UUID,
    // so a widget bound to this card works on either OS unchanged.
    let source = |definitions: Vec<MetricDefinition>| -> String {
        definitions
            .iter()
            .find(|definition| definition.metric.key.as_str() == gpu::USAGE_CORE)
            .expect("declared")
            .metric
            .source_id
            .as_str()
            .to_string()
    };

    assert_eq!(
        source(linux_gpu_declarations()),
        source(windows_gpu_declarations())
    );
}

#[test]
fn only_the_provider_id_differs_between_the_platforms_for_gpus() {
    for (linux, windows) in linux_gpu_declarations()
        .iter()
        .zip(windows_gpu_declarations().iter())
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

        assert_ne!(
            linux.provider_id, windows.provider_id,
            "provider id should identify the platform for {reference}"
        );
    }
}

#[test]
fn the_gpu_catalog_size_follows_the_machine_identically_on_both_platforms() {
    for count in 0..=4_usize {
        let gpus: Vec<GpuDescriptor> = (0..count)
            .map(|index| {
                let uuid = format!("GPU-{index:08x}-0000-0000-0000-000000000000");
                GpuDescriptor {
                    source_id: gpu::nvml_source_id(&uuid).expect("valid"),
                    display_name: "NVIDIA GeForce RTX 4090".to_string(),
                    vendor: GpuVendor::Nvidia,
                    identity: GpuIdentity::NvmlUuid(uuid),
                    pci: None,
                    backend: "nvml",
                    capabilities: GpuCapabilities::all_available(),
                }
            })
            .collect();

        let sizes: Vec<usize> = ["linux.gpu", "windows.gpu"]
            .map(|id| {
                let provider = ProviderId::new(id).expect("valid");
                gpu::definitions(&provider, &gpus).len()
            })
            .to_vec();

        assert_eq!(sizes[0], sizes[1]);
        assert_eq!(sizes[0], 1 + gpu::PER_GPU_KEYS.len() * count);
    }
}

#[test]
fn an_unsupported_gpu_metric_keeps_its_definition_on_both_platforms() {
    // A driver difference between the two operating systems must change the
    // availability and nothing else, so the reference stays resolvable.
    let degraded = vec![GpuDescriptor {
        capabilities: GpuCapabilities::none_available(&Availability::unsupported(
            "no telemetry backend",
        )),
        ..synthetic_gpus().remove(0)
    }];

    for id in ["linux.gpu", "windows.gpu"] {
        let provider = ProviderId::new(id).expect("valid");
        let definitions = gpu::definitions(&provider, &degraded);

        assert_eq!(
            definitions.len(),
            1 + gpu::PER_GPU_KEYS.len(),
            "{id} dropped a metric"
        );
        assert_eq!(
            definitions
                .iter()
                .filter(|definition| !definition.availability.is_available())
                .count(),
            gpu::PER_GPU_KEYS.len()
        );
    }
}

#[test]
fn gpu_provider_ids_follow_the_documented_convention() {
    assert_eq!(crate::platform::linux::gpu::PROVIDER_ID, "linux.gpu");
    assert_eq!(crate::platform::windows::gpu::PROVIDER_ID, "windows.gpu");

    for id in [
        crate::platform::linux::gpu::PROVIDER_ID,
        crate::platform::windows::gpu::PROVIDER_ID,
    ] {
        assert!(ProviderId::new(id).is_ok(), "invalid provider id: {id}");
    }
}

#[test]
fn one_gpu_provider_owns_every_gpu_metric_on_both_platforms() {
    // Not one provider per GPU, and not one per vendor backend — which would
    // make two providers claim the same reference.
    for declarations in [linux_gpu_declarations(), windows_gpu_declarations()] {
        let mut owners: Vec<&str> = declarations
            .iter()
            .map(|definition| definition.provider_id.as_str())
            .collect();
        owners.sort_unstable();
        owners.dedup();

        assert_eq!(owners.len(), 1);
    }
}

#[test]
fn gpu_user_facing_text_never_names_an_operating_system_or_backend() {
    for definition in linux_gpu_declarations()
        .iter()
        .chain(windows_gpu_declarations().iter())
    {
        let lowered = definition.display_name.to_lowercase();
        for forbidden in [
            "linux", "windows", "fedora", "nvml", "dxgi", "amdgpu", "/sys",
        ] {
            assert!(
                !lowered.contains(forbidden),
                "'{}' leaks '{forbidden}'",
                definition.display_name
            );
        }
    }
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
    // 4 memory + 4 CPU system + 3 per logical processor + 1 per package.
    let expected = 4 + 4 + 3 * 4 + 1;

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
        )
        .with_packages(vec![CpuPackage::unavailable(
            PackageId::new(0),
            Availability::unsupported("no package temperature sensor"),
        )]),
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
        unavailable, 9,
        "both frequency metrics of all four processors, plus the package temperature"
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

        // Either the machine-wide instance, a numbered logical processor or a
        // numbered package — never anything derived from a product name or a
        // bus address.
        let instance = source.instance();
        assert!(
            instance == "system"
                || LogicalId::from_source(source).is_some()
                || PackageId::from_source(source).is_some(),
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

// --- thermal contract -----------------------------------------------------

/// The five metrics Phase 5 added, and the shape each one promises.
const THERMAL_CONTRACT: &[(&str, crate::metrics::model::MetricUnit)] = &[
    (
        cpu::TEMPERATURE_PACKAGE,
        crate::metrics::model::MetricUnit::Celsius,
    ),
    (
        gpu::TEMPERATURE_CORE,
        crate::metrics::model::MetricUnit::Celsius,
    ),
    (
        gpu::TEMPERATURE_HOTSPOT,
        crate::metrics::model::MetricUnit::Celsius,
    ),
    (
        gpu::TEMPERATURE_MEMORY,
        crate::metrics::model::MetricUnit::Celsius,
    ),
    (gpu::FAN_SPEED, crate::metrics::model::MetricUnit::Rpm),
];

/// Every declaration a platform produces for the synthetic machine.
fn all_declarations(
    cpu_provider: &str,
    memory_provider: &str,
    gpu_provider: &str,
) -> Vec<MetricDefinition> {
    let mut definitions = declarations(cpu_provider, memory_provider);
    definitions.extend(gpu_declarations(gpu_provider));
    definitions.sort_by(|left, right| left.metric.cmp(&right.metric));
    definitions
}

fn linux_all() -> Vec<MetricDefinition> {
    all_declarations("linux.cpu", "linux.memory", "linux.gpu")
}

fn windows_all() -> Vec<MetricDefinition> {
    all_declarations("windows.cpu", "windows.memory", "windows.gpu")
}

#[test]
fn the_thermal_metrics_carry_the_same_contract_on_both_platforms() {
    // The portability promise for Phase 5: a widget bound to a temperature on
    // Fedora means the same thing on Windows, whatever each platform can
    // actually read.
    for (key, unit) in THERMAL_CONTRACT {
        let of = |definitions: Vec<MetricDefinition>| -> MetricDefinition {
            definitions
                .into_iter()
                .find(|definition| definition.metric.key.as_str() == *key)
                .unwrap_or_else(|| panic!("{key} is not declared"))
        };

        let linux = of(linux_all());
        let windows = of(windows_all());

        assert_eq!(linux.metric, windows.metric, "reference for {key}");
        assert_eq!(linux.unit, *unit, "unit for {key}");
        assert_eq!(windows.unit, *unit, "unit for {key}");
        assert_eq!(
            linux.kind,
            crate::metrics::model::MetricKind::Gauge,
            "kind for {key}"
        );
        assert_eq!(linux.kind, windows.kind, "kind for {key}");
        assert_eq!(
            linux.value_type,
            crate::metrics::model::MetricValueType::Number,
            "value type for {key}"
        );
        assert_eq!(linux.value_type, windows.value_type, "value type for {key}");
        assert_eq!(
            linux.display_name, windows.display_name,
            "display name for {key}"
        );
        assert_eq!(
            linux.source_label, windows.source_label,
            "source label for {key}"
        );
        assert_eq!(
            linux.description, windows.description,
            "description for {key}"
        );
        assert_eq!(linux.category, windows.category, "category for {key}");

        // Only the provider may differ.
        assert_ne!(linux.provider_id, windows.provider_id, "provider for {key}");
    }
}

#[test]
fn a_temperature_is_never_declared_in_anything_but_celsius() {
    // The canonical-unit rule, at the point it is easiest to break: a driver
    // reporting millidegrees, a vendor library reporting whole degrees, and a
    // frontend that must never learn either exists.
    for definition in linux_all().iter().chain(windows_all().iter()) {
        if definition.metric.key.as_str().contains("temperature") {
            assert_eq!(
                definition.unit,
                crate::metrics::model::MetricUnit::Celsius,
                "{} must be declared in Celsius",
                definition.metric
            );
        }
    }
}

#[test]
fn a_fan_speed_is_never_declared_in_anything_but_rpm() {
    // A duty-cycle percentage is not a speed. Declaring the metric in RPM is
    // what makes republishing one as the other a type-level mistake rather
    // than a plausible shortcut.
    for definition in linux_all().iter().chain(windows_all().iter()) {
        if definition.metric.key.as_str() == gpu::FAN_SPEED {
            assert_eq!(definition.unit, crate::metrics::model::MetricUnit::Rpm);
            assert_ne!(definition.unit, crate::metrics::model::MetricUnit::Percent);
        }
    }
}

#[test]
fn the_thermal_descriptions_say_what_the_sensor_is_not() {
    // Documentation the user actually sees, pinned: these three sentences are
    // what stops a hotspot being read as a die temperature, a package
    // temperature as a core average, and a fan percentage as an RPM.
    let description = |key: &str| -> String {
        linux_all()
            .into_iter()
            .find(|definition| definition.metric.key.as_str() == key)
            .expect("declared")
            .description
    };

    assert!(description(cpu::TEMPERATURE_PACKAGE).contains("never an average"));
    assert!(description(gpu::TEMPERATURE_HOTSPOT).contains("separate sensor"));
    assert!(description(gpu::FAN_SPEED).contains("duty cycle"));
}

#[test]
fn every_thermal_source_is_addressable_the_same_way_on_both_platforms() {
    // A package is `cpu:package-N` and a GPU keeps the identity it already had.
    // Neither gains a thermal-only source, which would split one device into
    // two rows in every future dashboard.
    for definitions in [linux_all(), windows_all()] {
        for definition in definitions {
            let key = definition.metric.key.as_str();
            if !THERMAL_CONTRACT.iter().any(|(name, _)| *name == key) {
                continue;
            }

            let source = &definition.metric.source_id;
            if key == cpu::TEMPERATURE_PACKAGE {
                assert!(
                    PackageId::from_source(source).is_some(),
                    "{source} is not a package source"
                );
            } else {
                assert_eq!(source.kind(), "gpu", "{source} is not a GPU source");
                assert_ne!(source.as_str(), gpu::SOURCE, "a device, not the aggregate");
            }
        }
    }
}
