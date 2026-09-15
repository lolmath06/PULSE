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
//! ## Phase 1 scope
//!
//! The model, the engine and the frontend contract. **No system collectors** —
//! `build_engine` registers nothing, so PULSE reports an empty catalog rather
//! than inventing numbers. Real providers arrive in later phases.

pub mod engine;
pub mod model;
pub mod providers;

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

/// Builds the engine PULSE runs with.
///
/// Phase 1 registers **no providers**. This is intentional: PULSE has no real
/// collectors yet, and a demonstration provider would put fabricated
/// temperatures and CPU loads in front of the user. The engine's behaviour is
/// covered by tests using a `#[cfg(test)]` mock instead, which never ships.
///
/// Later phases register the platform providers here, for example:
///
/// ```ignore
/// #[cfg(target_os = "linux")]
/// engine.register(Arc::new(linux::CpuProvider::new()?))?;
/// #[cfg(target_os = "windows")]
/// engine.register(Arc::new(windows::PdhProvider::new()?))?;
/// ```
///
/// Registration failures are logged and skipped rather than propagated: one
/// unavailable provider must never prevent PULSE from starting.
pub fn build_engine() -> MetricsEngine {
    MetricsEngine::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_engine_registers_no_fabricated_metrics() {
        // Guards the promise that PULSE never displays invented hardware data.
        let engine = build_engine();

        assert_eq!(engine.status().provider_count, 0);
        assert!(engine.catalog().is_empty());
        assert_eq!(engine.status().state, EngineState::Empty);
    }

    #[test]
    fn the_schema_version_is_pinned() {
        // A deliberate change here must also update src/types/metrics.ts and
        // docs/metrics/model.md.
        assert_eq!(METRICS_SCHEMA_VERSION, 1);
    }
}
