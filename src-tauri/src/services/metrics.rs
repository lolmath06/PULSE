//! Application-level access to the metrics engine.
//!
//! Thin by design: the engine already owns the logic, so this layer exists to
//! keep commands free of any direct engine knowledge and to give later phases
//! (scheduling, history, subscriptions) an obvious place to sit.

use crate::metrics::{EngineStatus, MetricDefinition, MetricRef, MetricSample, MetricsEngine};

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
    use crate::metrics::build_engine;

    #[test]
    fn the_shipped_engine_exposes_an_empty_catalog() {
        let engine = build_engine();

        assert!(catalog(&engine).is_empty());
        assert_eq!(status(&engine).metric_count, 0);
        assert!(sample(&engine, &[]).is_empty());
    }

    #[test]
    fn sampling_an_unknown_metric_returns_a_sample_rather_than_failing() {
        let engine = build_engine();
        let reference = MetricRef::parse("cpu.usage.total", "cpu:0").expect("valid");

        let samples = sample(&engine, std::slice::from_ref(&reference));

        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].metric, reference);
        assert!(!samples[0].availability.is_available());
    }
}
