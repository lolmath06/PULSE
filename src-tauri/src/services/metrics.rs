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
/// `windows.cpu` and `windows.memory`. Both publish the same five metric
/// references.
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
    use crate::metrics::wellknown::{cpu, memory};

    /// Every metric reference PULSE ships in Phase 2, in catalog order.
    fn expected_references() -> Vec<MetricRef> {
        vec![
            cpu::usage_total_ref(),
            memory::available_ref(),
            memory::total_ref(),
            memory::usage_percent_ref(),
            memory::used_ref(),
        ]
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

        assert_eq!(status.provider_count, 2, "one CPU and one memory provider");
        assert_eq!(status.metric_count, 5);
        assert_eq!(status.state, crate::metrics::EngineState::Ready);
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

        assert_eq!(references, expected_references());
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
            platform::PlatformKind::Linux => ["linux.cpu", "linux.memory"],
            platform::PlatformKind::Windows => ["windows.cpu", "windows.memory"],
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
