//! Application-level access to the metrics engine.
//!
//! Thin by design: the engine already owns the logic, so this layer exists to
//! keep commands free of any direct engine knowledge and to give later phases
//! (scheduling, history, subscriptions) an obvious place to sit.

use crate::metrics::{
    self, EngineStatus, MetricDefinition, MetricRef, MetricSample, MetricsEngine,
};
use crate::platform;

/// Builds the metrics engine PULSE runs with.
///
/// This is the composition point where the two independent layers meet:
/// the platform layer decides *which* providers exist on this host, and the
/// metrics layer knows *how* to host them. Neither depends on the other —
/// `services` sits above both — which is what keeps `cfg(target_os)` out of
/// the engine entirely.
///
/// On Fedora this yields `linux.cpu` and `linux.memory`; on Windows,
/// `windows.cpu` and `windows.memory`. Both publish the same metric
/// references for a given machine.
///
/// **The catalog size is a property of the host, not a constant.** The four
/// memory metrics and the four machine-wide CPU metrics are always there; the
/// rest is three metrics per logical processor, so the same code yields 20
/// metrics on a four-thread virtual machine and 104 on a 32-thread laptop.
pub fn build_engine() -> MetricsEngine {
    metrics::build_engine(platform::host().metric_providers())
}

/// Reports the engine's condition.
pub fn status(engine: &MetricsEngine) -> EngineStatus {
    engine.status()
}

/// Returns the full catalog, in deterministic order.
pub fn catalog(engine: &MetricsEngine) -> Vec<MetricDefinition> {
    engine.catalog().to_vec()
}

/// Samples the requested metrics.
///
/// Unknown references and provider failures come back as unavailable samples,
/// never as an error for the whole call — a single stale dashboard entry must
/// not blank out every other widget.
pub fn sample(engine: &MetricsEngine, requested: &[MetricRef]) -> Vec<MetricSample> {
    engine.sample(requested)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::wellknown::{cpu, gpu, memory};

    /// The metric references PULSE ships on **every** machine, whatever its
    /// CPU. The per-processor references are added to these at runtime.
    fn machine_independent_references() -> Vec<MetricRef> {
        vec![
            cpu::count_logical_ref(),
            cpu::count_package_ref(),
            cpu::count_physical_ref(),
            cpu::usage_total_ref(),
            gpu::count_ref(),
            memory::available_ref(),
            memory::total_ref(),
            memory::usage_percent_ref(),
            memory::used_ref(),
        ]
    }

    /// How many logical processors this host reports, via the catalog itself.
    fn logical_processor_count(engine: &MetricsEngine) -> usize {
        catalog(engine)
            .iter()
            .filter(|definition| definition.metric.key.as_str() == cpu::USAGE_LOGICAL)
            .count()
    }

    /// How many GPUs this host reports, via the catalog itself.
    ///
    /// Zero on a headless machine, which is a perfectly valid answer.
    fn gpu_count(engine: &MetricsEngine) -> usize {
        catalog(engine)
            .iter()
            .filter(|definition| definition.metric.key.as_str() == gpu::USAGE_CORE)
            .count()
    }

    /// How many CPU packages this host can be measured per-socket on.
    ///
    /// Zero where no package-level sensor was found, which is the normal
    /// Windows answer and a possible Fedora one.
    fn package_count(engine: &MetricsEngine) -> usize {
        catalog(engine)
            .iter()
            .filter(|definition| definition.metric.key.as_str() == cpu::TEMPERATURE_PACKAGE)
            .count()
    }

    #[test]
    fn the_shipped_engine_registers_the_platform_providers() {
        let engine = build_engine();
        let status = status(&engine);

        if !platform::PlatformKind::current().is_supported() {
            // macOS/BSD: no integration, and that is reported honestly.
            assert_eq!(status.provider_count, 0);
            return;
        }

        // Three providers, whatever the machine: each owns a whole metric
        // family rather than there being one per processor or one per GPU.
        assert_eq!(
            status.provider_count, 3,
            "one CPU, one memory and one GPU provider"
        );
        assert_eq!(status.state, crate::metrics::EngineState::Ready);

        // 9 fixed metrics, plus three per logical processor, one per
        // addressable CPU package and eleven per GPU — all discovered from the
        // catalog, never hardcoded.
        let logical = logical_processor_count(&engine);
        let gpus = gpu_count(&engine);
        let packages = package_count(&engine);
        assert!(logical > 0, "a running machine has logical processors");
        assert_eq!(
            status.metric_count,
            9 + 3 * logical + packages + crate::metrics::wellknown::gpu::PER_GPU_KEYS.len() * gpus
        );
        assert!(status.available_metric_count <= status.metric_count);
    }

    #[test]
    fn the_catalog_holds_exactly_the_expected_references_in_order() {
        if !platform::PlatformKind::current().is_supported() {
            return;
        }

        let engine = build_engine();
        let references: Vec<MetricRef> = catalog(&engine)
            .into_iter()
            .map(|definition| definition.metric)
            .collect();

        // Every machine-independent reference is present exactly once.
        for expected in machine_independent_references() {
            assert_eq!(
                references.iter().filter(|r| **r == expected).count(),
                1,
                "{expected} must appear exactly once"
            );
        }

        // And every logical processor contributes its three.
        let logical = logical_processor_count(&engine);
        for key in [
            cpu::USAGE_LOGICAL,
            cpu::FREQUENCY_CURRENT,
            cpu::FREQUENCY_MAX,
        ] {
            assert_eq!(
                references
                    .iter()
                    .filter(|reference| reference.key.as_str() == key)
                    .count(),
                logical,
                "{key} must be declared once per logical processor"
            );
        }

        // The order is deterministic, which is what a saved dashboard and the
        // UI's discovery both rely on.
        let mut sorted = references.clone();
        sorted.sort();
        assert_eq!(references, sorted);
    }

    #[test]
    fn provider_ids_name_the_host_platform() {
        if !platform::PlatformKind::current().is_supported() {
            return;
        }

        let engine = build_engine();
        let ids: Vec<String> = status(&engine)
            .providers
            .iter()
            .map(|summary| summary.id.as_str().to_string())
            .collect();

        let expected = match platform::PlatformKind::current() {
            platform::PlatformKind::Linux => ["linux.cpu", "linux.memory", "linux.gpu"],
            platform::PlatformKind::Windows => ["windows.cpu", "windows.memory", "windows.gpu"],
            platform::PlatformKind::Unsupported => unreachable!("guarded above"),
        };

        assert_eq!(ids, expected);
    }

    #[test]
    fn sampling_an_unknown_metric_returns_a_sample_rather_than_failing() {
        let engine = build_engine();
        let reference = MetricRef::parse("gpu.temperature.core", "gpu:card0").expect("valid");

        let samples = sample(&engine, std::slice::from_ref(&reference));

        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].metric, reference);
        assert!(!samples[0].availability.is_available());
    }

    #[test]
    fn an_empty_request_is_answered_with_nothing() {
        assert!(sample(&build_engine(), &[]).is_empty());
    }
}
