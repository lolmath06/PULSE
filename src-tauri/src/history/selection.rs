//! Which metrics are historized.
//!
//! An explicit allow-list of **keys**, applied to whatever sources the catalog
//! holds on this machine. Explicit rather than "every number" because the
//! catalog also contains values that never move (core counts, total memory,
//! link speed, MTU), counters that are only meaningful as a single reading
//! (power-on hours), and health readings that are costly to take — none of
//! which earns a row every five seconds.
//!
//! # Never per process
//!
//! Only the three machine-wide process metrics on `process:system` are
//! historized. A per-process series is refused **whatever its key**: a PID is
//! recycled, a process lives for seconds, and a history keyed by one would be
//! both unbounded and a record of what the user ran. The process snapshot
//! service stays strictly live.
//!
//! # Deliberately excluded
//!
//! - `storage.health.*`, including the drive temperature: on NVMe every read is
//!   an admin command, and one every five seconds keeps the controller out of
//!   its low-power states. Health stays a Refresh-time reading.
//! - `cpu.frequency.*` per logical processor: it would double the per-processor
//!   row count for a value the CPU changes hundreds of times per second, so a
//!   five-second sample of it says very little.
//! - Constants and capacities (`*.count`, `*.total`, link speeds, MTU).

use crate::metrics::model::{MetricDefinition, MetricRef, MetricValueType};
use crate::metrics::wellknown::{cpu, gpu, memory, network, process, storage};

/// Every key PULSE records history for, grouped by family.
pub const HISTORIZED_KEYS: &[&str] = &[
    // CPU
    cpu::USAGE_TOTAL,
    cpu::USAGE_LOGICAL,
    cpu::TEMPERATURE_PACKAGE,
    // Memory
    memory::USED,
    memory::AVAILABLE,
    memory::USAGE_PERCENT,
    // GPU
    gpu::USAGE_CORE,
    gpu::MEMORY_USED,
    gpu::MEMORY_USAGE_PERCENT,
    gpu::TEMPERATURE_CORE,
    gpu::TEMPERATURE_HOTSPOT,
    gpu::TEMPERATURE_MEMORY,
    gpu::FAN_SPEED,
    // Storage
    storage::IO_READ_BYTES,
    storage::IO_WRITE_BYTES,
    storage::IO_READ_IOPS,
    storage::IO_WRITE_IOPS,
    storage::VOLUME_USAGE_PERCENT,
    // Network
    network::RECEIVE_BYTES,
    network::TRANSMIT_BYTES,
    network::WIFI_SIGNAL_RSSI,
    network::WIFI_SIGNAL_QUALITY,
    // Machine-wide process counts
    process::COUNT_TOTAL,
    process::COUNT_RUNNING,
    process::THREAD_COUNT_TOTAL,
];

/// The only `process:` source history accepts.
const MACHINE_PROCESS_SOURCE: &str = process::SOURCE;

/// Whether one reference may ever be written to history.
///
/// Also used by the store as a last line of defence, so a caller that builds a
/// batch by hand still cannot record a per-process series.
pub fn is_historizable(metric: &MetricRef) -> bool {
    let source = metric.source_id.as_str();
    if source.starts_with("process:") && source != MACHINE_PROCESS_SOURCE {
        return false;
    }

    HISTORIZED_KEYS.contains(&metric.key.as_str())
}

/// Whether one catalog entry is historized.
pub fn is_historized(definition: &MetricDefinition) -> bool {
    definition.value_type == MetricValueType::Number && is_historizable(&definition.metric)
}

/// The references to record, from a catalog, in catalog order.
pub fn historized_refs(catalog: &[MetricDefinition]) -> Vec<MetricRef> {
    catalog
        .iter()
        .filter(|definition| is_historized(definition))
        .map(|definition| definition.metric.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::model::{
        MetricCategory, MetricDefinitionBuilder, MetricKind, MetricUnit, ProviderId,
    };

    fn definition(key: &str, source: &str) -> MetricDefinition {
        MetricDefinitionBuilder::new(
            MetricRef::parse(key, source).expect("valid reference"),
            ProviderId::new("mock").expect("valid"),
            MetricCategory::Cpu,
            MetricUnit::Percent,
            MetricKind::Gauge,
        )
        .build()
    }

    #[test]
    fn cpu_total_is_historized_on_its_canonical_source() {
        assert!(is_historized(&definition(cpu::USAGE_TOTAL, cpu::SOURCE)));
    }

    #[test]
    fn the_three_machine_process_counts_are_historized() {
        for key in [
            process::COUNT_TOTAL,
            process::COUNT_RUNNING,
            process::THREAD_COUNT_TOTAL,
        ] {
            assert!(is_historized(&definition(key, "process:system")), "{key}");
        }
    }

    #[test]
    fn no_per_process_series_is_ever_historized() {
        // A process identity obeys the source grammar — and must still be
        // refused, whatever key it is paired with.
        for key in [
            process::COUNT_TOTAL,
            cpu::USAGE_TOTAL,
            "process.cpu.usage",
            "process.memory.resident",
        ] {
            let per_pid = definition(key, "process:12345-9001");
            assert!(!is_historized(&per_pid), "{key}@process:12345-9001");
        }

        let catalog = vec![
            definition(process::COUNT_TOTAL, "process:system"),
            definition(process::COUNT_TOTAL, "process:4242-1"),
            definition("process.cpu.usage", "process:4242-1"),
        ];
        let selected = historized_refs(&catalog);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].source_id.as_str(), "process:system");
    }

    #[test]
    fn constants_and_costly_health_readings_are_not_historized() {
        for (key, source) in [
            (cpu::COUNT_LOGICAL, cpu::SOURCE),
            (memory::TOTAL, memory::SOURCE),
            (cpu::FREQUENCY_CURRENT, "cpu:logical-0"),
            (storage::HEALTH_TEMPERATURE, "storage:nvme0n1"),
            (storage::HEALTH_POWER_ON_HOURS, "storage:nvme0n1"),
            (network::MTU, "network:mac-001122334455"),
        ] {
            assert!(!is_historized(&definition(key, source)), "{key}");
        }
    }

    #[test]
    fn non_numeric_metrics_are_not_historized_even_when_listed() {
        let mut text = definition(cpu::USAGE_TOTAL, cpu::SOURCE);
        text.value_type = MetricValueType::Text;
        assert!(!is_historized(&text));
    }

    #[test]
    fn the_allow_list_has_no_duplicates() {
        let mut keys = HISTORIZED_KEYS.to_vec();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), HISTORIZED_KEYS.len());
    }

    #[test]
    fn selection_keeps_catalog_order() {
        let catalog = vec![
            definition(cpu::USAGE_LOGICAL, "cpu:logical-0"),
            definition(cpu::COUNT_LOGICAL, cpu::SOURCE),
            definition(cpu::USAGE_TOTAL, cpu::SOURCE),
        ];
        let selected: Vec<String> = historized_refs(&catalog)
            .iter()
            .map(|r| r.to_string())
            .collect();
        assert_eq!(selected.len(), 2);
        assert!(selected[0].starts_with(cpu::USAGE_LOGICAL));
        assert!(selected[1].starts_with(cpu::USAGE_TOTAL));
    }
}
