//! The PULSE metrics engine.
//!
//! ```text
//! React UI
//!    │  src/services/metrics.ts
//! Tauri commands            commands/metrics.rs
//!    │
//! MetricsEngine             metrics/engine/
//!    │  MetricProvider       metrics/providers/
//!    ├── linux.cpu ──────┐
//!    ├── windows.pdh ────┼──► platform/ ──► /proc, /sys, PDH, WMI, NVML…
//!    └── nvidia.nvml ────┘
//! ```
//!
//! The engine itself contains **no platform code at all**. It knows about
//! providers, a catalog and references; where a number comes from is entirely
//! the provider's business. That is the property which makes this true:
//!
//! > All future PULSE metrics can be added behind a single contract without the
//! > interface needing to know whether they come from Fedora, Windows, NVIDIA,
//! > `/proc`, WMI or SMART.
//!
//! ## Scope
//!
//! Phase 1 built the model, the engine and the frontend contract. Phase 2 added
//! the first real collectors: CPU usage and physical memory, natively on both
//! Fedora and Windows. The engine itself did not change to accommodate them —
//! which was the point. Real providers arrive in later phases.

pub mod engine;
pub mod model;
pub mod providers;
pub mod wellknown;

use std::sync::Arc;

use crate::metrics::providers::MetricProvider;

pub use engine::{EngineState, EngineStatus, MetricsEngine, ProviderSummary, RegistrationError};
pub use model::*;

/// Version of the metrics contract shared with the frontend.
///
/// Bump this when a change to the payloads is **incompatible** — a removed or
/// renamed field, a changed unit convention, a repurposed enum variant. Adding
/// an optional field does not require a bump.
///
/// The frontend compares this against its own constant and warns rather than
/// misinterpreting data. There is deliberately no migration machinery yet:
/// detecting the mismatch is what matters at this stage.
pub const METRICS_SCHEMA_VERSION: u32 = 1;

/// Assembles an engine from a list of providers.
///
/// **Deliberately platform-free.** It takes providers as an argument rather
/// than discovering them, so the metrics layer never learns that Linux or
/// Windows exist. Choosing which providers to build is the platform layer's
/// job (`HostPlatform::metric_providers`), and wiring the two together is the
/// service layer's (`services::metrics::build_engine`).
///
/// A provider that fails to register is **logged and skipped**, never
/// propagated: one unavailable data source must not prevent PULSE from
/// starting. The returned engine reports what actually registered, so the UI
/// can show a smaller provider count instead of a blank window.
pub fn build_engine(providers: Vec<Arc<dyn MetricProvider>>) -> MetricsEngine {
    let mut engine = MetricsEngine::new();

    for provider in providers {
        let id = provider.id().clone();
        if let Err(error) = engine.register(provider) {
            eprintln!("PULSE: skipping metric provider '{id}': {error}");
        }
    }

    engine
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_engine_built_from_no_providers_is_empty_not_broken() {
        let engine = build_engine(Vec::new());

        assert_eq!(engine.status().provider_count, 0);
        assert!(engine.catalog().is_empty());
        assert_eq!(engine.status().state, EngineState::Empty);
    }

    #[test]
    fn a_provider_that_fails_to_register_is_skipped_not_fatal() {
        use crate::metrics::model::{MetricError, MetricErrorCode};
        use crate::metrics::providers::mock::MockProvider;

        let engine = build_engine(vec![
            Arc::new(MockProvider::new("mock.ok").with_metric(
                "cpu.usage.total",
                "cpu:system",
                1.0,
            )),
            Arc::new(
                MockProvider::new("mock.broken").failing_describe(MetricError::new(
                    MetricErrorCode::ProviderUnavailable,
                    "driver missing",
                )),
            ),
            // Collides with mock.ok: also rejected, also non-fatal.
            Arc::new(MockProvider::new("mock.clash").with_metric(
                "cpu.usage.total",
                "cpu:system",
                2.0,
            )),
        ]);

        // The healthy provider survived; PULSE still starts.
        assert_eq!(engine.status().provider_count, 1);
        assert_eq!(engine.status().metric_count, 1);
        assert_eq!(engine.status().providers[0].id.as_str(), "mock.ok");
    }

    #[test]
    fn the_schema_version_is_pinned() {
        // A deliberate change here must also update src/types/metrics.ts and
        // docs/metrics/model.md.
        assert_eq!(METRICS_SCHEMA_VERSION, 1);
    }
}
