//! The `cpu.*` metrics: declarations shared by every platform.
//!
//! This module owns every CPU metric key, source, unit, kind and user-facing
//! string PULSE ships. Platform providers supply **only the raw numbers** and
//! the topology they discovered; they never choose a key or a unit, which is
//! what keeps `cpu.usage.logical@cpu:logical-3` meaning exactly the same thing
//! on Fedora and on Windows.
//!
//! # The catalog is sized by the machine
//!
//! Unlike memory, the CPU metric set is **not a fixed list**. For a machine
//! with `N` logical processors, [`definitions`] produces:
//!
//! ```text
//!  1  cpu.usage.total@cpu:system
//!  3  cpu.count.{logical,physical,package}@cpu:system
//!  N  cpu.usage.logical@cpu:logical-i
//!  N  cpu.frequency.current@cpu:logical-i
//!  N  cpu.frequency.max@cpu:logical-i
//!  P  cpu.temperature.package@cpu:package-i
//! ─────
//!  4 + 3N + P
//! ```
//!
//! `P` is the number of packages PULSE can address as a measurement source,
//! which is **not** the same as `cpu.count.package`: a machine can know it has
//! two sockets while exposing a thermal sensor for neither.
//!
//! Nothing anywhere in PULSE hardcodes `N`. The runtime discovers it, and a
//! four-processor virtual machine and a 128-thread workstation differ only in
//! how many rows the catalog has.
//!
//! # Submodules
//!
//! - [`topology`] — logical processors, physical cores and packages, kept
//!   rigorously distinct.
//! - [`usage`] — counters, delta arithmetic and the multi-processor baseline.
//! - [`frequency`] — the hertz contract and what "current" and "max" mean.

pub mod frequency;
pub mod topology;
pub mod usage;

use crate::metrics::model::{
    Availability, MetricCategory, MetricDefinition, MetricDefinitionBuilder, MetricKey, MetricKind,
    MetricRef, MetricUnit, ProviderId, SourceId,
};

pub use frequency::{kilohertz_to_hertz, megahertz_to_hertz, MaxFrequencySource};
pub use topology::{CpuPackage, CpuTopology, LogicalId, LogicalProcessor, PackageId};
pub use usage::{
    usage_percent, CpuCounters, CpuSnapshot, CpuUsage, CpuUsageReport, CpuUsageTracker,
    NeedsAnotherSample,
};

// --- metric keys ----------------------------------------------------------

/// `cpu.usage.total` — aggregate CPU utilisation across the whole machine.
pub const USAGE_TOTAL: &str = "cpu.usage.total";

/// `cpu.usage.logical` — utilisation of one logical processor.
///
/// Named `logical`, not `per_core`, because that is what it measures: with SMT
/// enabled, two of these share one physical core. See [`topology`].
pub const USAGE_LOGICAL: &str = "cpu.usage.logical";

/// `cpu.frequency.current` — the clock the OS currently reports for one
/// logical processor, in hertz.
pub const FREQUENCY_CURRENT: &str = "cpu.frequency.current";

/// `cpu.frequency.max` — the maximum clock the platform reports for one
/// logical processor, in hertz.
pub const FREQUENCY_MAX: &str = "cpu.frequency.max";

/// `cpu.count.logical` — how many logical processors (hardware threads) exist.
pub const COUNT_LOGICAL: &str = "cpu.count.logical";

/// `cpu.count.physical` — how many physical execution cores exist.
pub const COUNT_PHYSICAL: &str = "cpu.count.physical";

/// `cpu.count.package` — how many processor packages (sockets) exist.
pub const COUNT_PACKAGE: &str = "cpu.count.package";

/// `cpu.temperature.package` — the temperature of one processor package, in
/// degrees Celsius.
///
/// **The package, not a core, and never an average of cores.** A package sensor
/// is a real measurement the hardware reports; a mean of per-core readings is a
/// number PULSE would have invented, and one that reads lower than the truth
/// exactly when it matters — a single core boosting hard is what throttles a
/// machine, and averaging it away hides that.
///
/// Nor is it a *limit*: `Tjmax`, `Tcontrol` and `Tthrottle` are thresholds the
/// silicon is designed around, and publishing one as the current temperature
/// would tell a user their idle laptop is running at 100 °C. See
/// `docs/metrics/thermals.md`.
pub const TEMPERATURE_PACKAGE: &str = "cpu.temperature.package";

// --- sources --------------------------------------------------------------

/// The machine-wide CPU source.
///
/// `cpu:system` is a *logical* identifier for "this machine's CPU as a whole",
/// not a device path. It is therefore stable by construction and identical on
/// every platform.
pub const SOURCE: &str = "cpu:system";

/// The user-facing label for `cpu:system`.
const SYSTEM_LABEL: &str = "System CPU";

// --- references -----------------------------------------------------------

fn system_source() -> SourceId {
    SourceId::new(SOURCE).expect("well-known CPU source must be valid")
}

fn key(name: &str) -> MetricKey {
    MetricKey::new(name).expect("well-known CPU key must be valid")
}

/// Builds a reference against `cpu:system`.
fn system_ref(name: &str) -> MetricRef {
    MetricRef::new(key(name), system_source())
}

/// Builds the `cpu.usage.total` reference.
pub fn usage_total_ref() -> MetricRef {
    system_ref(USAGE_TOTAL)
}

/// Builds the `cpu.count.logical` reference.
pub fn count_logical_ref() -> MetricRef {
    system_ref(COUNT_LOGICAL)
}

/// Builds the `cpu.count.physical` reference.
pub fn count_physical_ref() -> MetricRef {
    system_ref(COUNT_PHYSICAL)
}

/// Builds the `cpu.count.package` reference.
pub fn count_package_ref() -> MetricRef {
    system_ref(COUNT_PACKAGE)
}

/// Builds the `cpu.usage.logical` reference for one logical processor.
pub fn usage_logical_ref(id: LogicalId) -> MetricRef {
    MetricRef::new(key(USAGE_LOGICAL), id.source_id())
}

/// Builds the `cpu.frequency.current` reference for one logical processor.
pub fn frequency_current_ref(id: LogicalId) -> MetricRef {
    MetricRef::new(key(FREQUENCY_CURRENT), id.source_id())
}

/// Builds the `cpu.frequency.max` reference for one logical processor.
pub fn frequency_max_ref(id: LogicalId) -> MetricRef {
    MetricRef::new(key(FREQUENCY_MAX), id.source_id())
}

/// Builds the `cpu.temperature.package` reference for one package.
pub fn temperature_package_ref(id: PackageId) -> MetricRef {
    MetricRef::new(key(TEMPERATURE_PACKAGE), id.source_id())
}

// --- declarations ---------------------------------------------------------

/// Declares every CPU metric PULSE ships on a machine with this topology.
///
/// Both the Linux and the Windows CPU provider call this, so their
/// declarations are identical apart from `providerId` and the availabilities
/// each platform genuinely discovered. A contract test asserts exactly that.
///
/// The result is sorted by metric reference, matching the order the engine's
/// catalog will hold it in, so the output is deterministic for a given
/// topology regardless of the order the platform enumerated processors.
pub fn definitions(provider: &ProviderId, topology: &CpuTopology) -> Vec<MetricDefinition> {
    let mut definitions =
        Vec::with_capacity(4 + 3 * topology.logical().len() + topology.packages().len());

    definitions.push(
        gauge(
            usage_total_ref(),
            provider,
            MetricUnit::Percent,
            SYSTEM_LABEL,
            "CPU usage",
            "Share of CPU time spent doing work across the whole machine, \
             measured since the previous sample.",
        )
        .build(),
    );

    definitions.extend(count_definitions(provider, topology));

    for processor in topology.logical() {
        let label = processor.id.label();

        definitions.push(
            gauge(
                usage_logical_ref(processor.id),
                provider,
                MetricUnit::Percent,
                &label,
                "CPU usage",
                "Share of CPU time this logical processor spent doing work, \
                 measured since the previous sample. With simultaneous \
                 multithreading, two logical processors share one physical core.",
            )
            .availability(processor.usage.clone())
            .build(),
        );

        definitions.push(
            gauge(
                frequency_current_ref(processor.id),
                provider,
                MetricUnit::Hertz,
                &label,
                "Current frequency",
                "Clock frequency the operating system currently reports for this \
                 logical processor. Scaling, boost and power policies move it \
                 continuously, so it is the platform's latest figure rather than \
                 an instantaneous measurement of the silicon.",
            )
            .availability(processor.frequency_current.clone())
            .build(),
        );

        definitions.push(
            gauge(
                frequency_max_ref(processor.id),
                provider,
                MetricUnit::Hertz,
                &label,
                "Maximum frequency",
                "Maximum clock frequency the platform reports for this logical \
                 processor. On a hybrid CPU this differs between performance and \
                 efficiency cores. A governor or power-policy ceiling is never \
                 published here in its place.",
            )
            .availability(processor.frequency_max.clone())
            .build(),
        );
    }

    for package in topology.packages() {
        definitions.push(
            gauge(
                temperature_package_ref(package.id),
                provider,
                MetricUnit::Celsius,
                &package.id.label(),
                "Package temperature",
                "Temperature the processor package reports for itself. It is the \
                 package's own sensor, never an average of the individual cores, \
                 and never one of the thermal limits the silicon is designed \
                 around.",
            )
            .availability(package.temperature.clone())
            .build(),
        );
    }

    definitions.sort_by(|left, right| left.metric.cmp(&right.metric));
    definitions
}

/// The three topology counts.
///
/// # Why `state` and not `gauge`
///
/// A core count does not rise and fall, and averaging one over time is
/// meaningless: "this machine had 23.6 physical cores last hour" is not a
/// sentence PULSE should ever be able to produce. [`MetricKind::State`] is
/// documented as "a discrete condition", it is what the model already uses for
/// facts rather than readings, and crucially `is_directly_averageable()` is
/// `false` for it — so the aggregation rules that arrive with history in a
/// later phase will refuse to average these by construction, rather than
/// relying on a widget author to know better.
///
/// The unit is [`MetricUnit::Count`] and the value type is a number, so a
/// widget can still render `24` without special-casing.
fn count_definitions(provider: &ProviderId, topology: &CpuTopology) -> Vec<MetricDefinition> {
    let count = |reference: MetricRef,
                 display: &str,
                 description: &str,
                 value: Option<u32>,
                 missing: &str| {
        MetricDefinitionBuilder::new(
            reference,
            provider.clone(),
            MetricCategory::Cpu,
            MetricUnit::Count,
            MetricKind::State,
        )
        .source_label(SYSTEM_LABEL)
        .display_name(display)
        .description(description)
        .availability(match value {
            Some(_) => Availability::Available,
            // Not a guess, and not a zero: the platform genuinely could not
            // tell PULSE, and the UI says so.
            None => Availability::not_detected(missing.to_string()),
        })
        .build()
    };

    vec![
        count(
            count_logical_ref(),
            "Logical processors",
            "Number of logical processors (hardware threads) the operating system \
             schedules work on. With simultaneous multithreading this exceeds the \
             number of physical cores.",
            Some(topology.logical_count()),
            "no logical processor could be enumerated",
        ),
        count(
            count_physical_ref(),
            "Physical cores",
            "Number of physical execution cores, counted as distinct \
             (package, core) pairs so that identically numbered cores in two \
             different packages are never merged.",
            topology.physical_core_count,
            "this platform exposes no CPU core topology",
        ),
        count(
            count_package_ref(),
            "Processor packages",
            "Number of physical processor packages (sockets) installed.",
            topology.package_count,
            "this platform exposes no CPU package topology",
        ),
    ]
}

/// Shared builder for the CPU gauges.
fn gauge(
    reference: MetricRef,
    provider: &ProviderId,
    unit: MetricUnit,
    source_label: &str,
    display_name: &str,
    description: &str,
) -> MetricDefinitionBuilder {
    MetricDefinitionBuilder::new(
        reference,
        provider.clone(),
        MetricCategory::Cpu,
        unit,
        MetricKind::Gauge,
    )
    .source_label(source_label)
    .display_name(display_name)
    .description(description)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::model::MetricValueType;

    fn provider() -> ProviderId {
        ProviderId::new("linux.cpu").expect("valid")
    }

    fn topology(count: u32) -> CpuTopology {
        CpuTopology::new(
            (0..count)
                .map(|ordinal| LogicalProcessor::available(LogicalId::new(ordinal)))
                .collect(),
            Some(count / 2),
            Some(1),
        )
    }

    #[test]
    fn the_system_references_are_valid_and_stable() {
        assert_eq!(usage_total_ref().to_string(), "cpu.usage.total@cpu:system");
        assert_eq!(
            count_logical_ref().to_string(),
            "cpu.count.logical@cpu:system"
        );
        assert_eq!(
            count_physical_ref().to_string(),
            "cpu.count.physical@cpu:system"
        );
        assert_eq!(
            count_package_ref().to_string(),
            "cpu.count.package@cpu:system"
        );
    }

    #[test]
    fn the_per_processor_references_carry_the_canonical_source() {
        let id = LogicalId::new(3);

        assert_eq!(
            usage_logical_ref(id).to_string(),
            "cpu.usage.logical@cpu:logical-3"
        );
        assert_eq!(
            frequency_current_ref(id).to_string(),
            "cpu.frequency.current@cpu:logical-3"
        );
        assert_eq!(
            frequency_max_ref(id).to_string(),
            "cpu.frequency.max@cpu:logical-3"
        );
    }

    #[test]
    fn the_catalog_grows_with_the_machine() {
        // 4 + 3N, discovered at runtime and never hardcoded.
        for logical_count in [1_u32, 2, 4, 8, 16, 32, 64, 128] {
            let definitions = definitions(&provider(), &topology(logical_count));
            assert_eq!(
                definitions.len(),
                (4 + 3 * logical_count) as usize,
                "wrong catalog size for {logical_count} logical processors"
            );
        }
    }

    #[test]
    fn a_machine_with_no_discoverable_processors_still_declares_the_system_metrics() {
        let definitions = definitions(&provider(), &CpuTopology::default());

        assert_eq!(definitions.len(), 4);
        // And the logical count is honestly reported as zero-but-known, while
        // the counts it could not determine are not.
        let logical = definitions
            .iter()
            .find(|definition| definition.metric == count_logical_ref())
            .expect("declared");
        assert!(logical.availability.is_available());
    }

    #[test]
    fn the_declaration_order_is_deterministic_and_numeric() {
        let ordered = definitions(&provider(), &topology(12));
        let references: Vec<String> = ordered
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect();

        let mut sorted = references.clone();
        sorted.sort();
        assert_eq!(references, sorted, "catalog order must be deterministic");

        // Enumerating in a different order must produce the same catalog.
        let shuffled = CpuTopology::new(
            (0..12)
                .rev()
                .map(|ordinal| LogicalProcessor::available(LogicalId::new(ordinal)))
                .collect(),
            Some(6),
            Some(1),
        );
        let from_shuffled: Vec<String> = definitions(&provider(), &shuffled)
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect();
        assert_eq!(references, from_shuffled);
    }

    #[test]
    fn usage_metrics_are_percent_gauges() {
        let definitions = definitions(&provider(), &topology(2));

        for reference in [
            usage_total_ref(),
            usage_logical_ref(LogicalId::new(0)),
            usage_logical_ref(LogicalId::new(1)),
        ] {
            let definition = definitions
                .iter()
                .find(|definition| definition.metric == reference)
                .unwrap_or_else(|| panic!("{reference} must be declared"));

            assert_eq!(definition.unit, MetricUnit::Percent);
            assert_eq!(definition.kind, MetricKind::Gauge);
            assert_eq!(definition.value_type, MetricValueType::Number);
            assert_eq!(definition.category, MetricCategory::Cpu);
        }
    }

    #[test]
    fn frequency_metrics_are_hertz_gauges() {
        let definitions = definitions(&provider(), &topology(2));

        for reference in [
            frequency_current_ref(LogicalId::new(0)),
            frequency_max_ref(LogicalId::new(0)),
            frequency_current_ref(LogicalId::new(1)),
            frequency_max_ref(LogicalId::new(1)),
        ] {
            let definition = definitions
                .iter()
                .find(|definition| definition.metric == reference)
                .unwrap_or_else(|| panic!("{reference} must be declared"));

            // Hertz, never kHz, MHz or GHz. The frontend converts for display.
            assert_eq!(definition.unit, MetricUnit::Hertz);
            assert_eq!(definition.kind, MetricKind::Gauge);
            assert_eq!(definition.value_type, MetricValueType::Number);
        }
    }

    #[test]
    fn counts_are_states_so_they_can_never_be_averaged() {
        let definitions = definitions(&provider(), &topology(4));

        for reference in [
            count_logical_ref(),
            count_physical_ref(),
            count_package_ref(),
        ] {
            let definition = definitions
                .iter()
                .find(|definition| definition.metric == reference)
                .unwrap_or_else(|| panic!("{reference} must be declared"));

            assert_eq!(definition.unit, MetricUnit::Count);
            assert_eq!(definition.value_type, MetricValueType::Number);
            assert_eq!(
                definition.kind,
                MetricKind::State,
                "{reference} must not be averageable"
            );
            assert!(!definition.kind.is_directly_averageable());
        }
    }

    #[test]
    fn an_undetectable_count_is_declared_unavailable_rather_than_guessed() {
        // A hypervisor hiding topology: PULSE says so instead of dividing the
        // logical count by two and calling it physical cores.
        let hidden = CpuTopology::new(
            (0..4)
                .map(|ordinal| LogicalProcessor::available(LogicalId::new(ordinal)))
                .collect(),
            None,
            None,
        );
        let definitions = definitions(&provider(), &hidden);

        let availability = |reference: MetricRef| {
            definitions
                .iter()
                .find(|definition| definition.metric == reference)
                .expect("declared")
                .availability
                .clone()
        };

        assert!(availability(count_logical_ref()).is_available());
        assert_eq!(
            availability(count_physical_ref()).status_str(),
            "notDetected"
        );
        assert_eq!(
            availability(count_package_ref()).status_str(),
            "notDetected"
        );
    }

    #[test]
    fn a_processor_without_frequency_support_still_declares_its_usage() {
        // The cpu17-has-no-cpufreq case: one missing file must not cost the
        // user thirty-one working usage metrics.
        let mixed = CpuTopology::new(
            vec![
                LogicalProcessor::available(LogicalId::new(0)),
                LogicalProcessor::available(LogicalId::new(1))
                    .with_frequencies(Availability::unsupported("no cpufreq directory")),
            ],
            Some(1),
            Some(1),
        );
        let definitions = definitions(&provider(), &mixed);

        let find = |reference: MetricRef| {
            definitions
                .iter()
                .find(|definition| definition.metric == reference)
                .expect("declared")
                .clone()
        };

        assert!(find(usage_logical_ref(LogicalId::new(1)))
            .availability
            .is_available());
        assert_eq!(
            find(frequency_current_ref(LogicalId::new(1)))
                .availability
                .status_str(),
            "unsupported"
        );
        // The other processor is untouched.
        assert!(find(frequency_current_ref(LogicalId::new(0)))
            .availability
            .is_available());
    }

    #[test]
    fn hybrid_processors_may_carry_different_maximum_frequencies() {
        // P-cores and E-cores are separate metrics on separate sources, so
        // nothing in the contract forces them to agree.
        let hybrid = topology(4);
        let definitions = definitions(&provider(), &hybrid);

        let maxima: Vec<&MetricDefinition> = definitions
            .iter()
            .filter(|definition| definition.metric.key.as_str() == FREQUENCY_MAX)
            .collect();

        assert_eq!(maxima.len(), 4);
        let sources: Vec<&str> = maxima
            .iter()
            .map(|definition| definition.metric.source_id.as_str())
            .collect();
        assert_eq!(
            sources,
            [
                "cpu:logical-0",
                "cpu:logical-1",
                "cpu:logical-2",
                "cpu:logical-3"
            ]
        );
    }

    #[test]
    fn every_declared_reference_is_unique() {
        // A collision would make the engine reject the whole provider.
        let definitions = definitions(&provider(), &topology(32));
        let mut references: Vec<String> = definitions
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect();

        let total = references.len();
        references.sort();
        references.dedup();

        assert_eq!(references.len(), total);
    }

    #[test]
    fn user_facing_labels_name_processors_the_way_other_tools_do() {
        let definitions = definitions(&provider(), &topology(2));

        let labels: Vec<&str> = definitions
            .iter()
            .filter(|definition| definition.metric.key.as_str() == USAGE_LOGICAL)
            .map(|definition| definition.source_label.as_str())
            .collect();

        assert_eq!(labels, ["CPU 0", "CPU 1"]);
    }

    #[test]
    fn no_declaration_leaks_the_operating_system_into_user_facing_text() {
        for definition in definitions(&provider(), &topology(4)) {
            for text in [&definition.display_name, &definition.source_label] {
                let lowered = text.to_lowercase();
                for forbidden in ["linux", "windows", "fedora", "/proc", "/sys", "win32"] {
                    assert!(!lowered.contains(forbidden), "'{text}' leaks '{forbidden}'");
                }
            }
        }
    }

    #[test]
    fn descriptions_keep_logical_and_physical_distinct() {
        let definitions = definitions(&provider(), &topology(2));

        let describe = |reference: MetricRef| {
            definitions
                .iter()
                .find(|definition| definition.metric == reference)
                .expect("declared")
                .description
                .to_lowercase()
        };

        // The whole point of the naming: a user must be able to learn the
        // difference from PULSE itself.
        assert!(describe(count_logical_ref()).contains("hardware thread"));
        assert!(describe(count_physical_ref()).contains("physical execution core"));
        assert!(describe(usage_logical_ref(LogicalId::new(0))).contains("logical processor"));
    }
}
