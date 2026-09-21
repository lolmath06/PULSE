//! Application state shared across Tauri commands.
//!
//! Held by Tauri as managed state and handed to commands as
//! `State<'_, AppState>`. Everything in here must be `Send + Sync`: commands
//! run on Tauri's thread pool, and later phases will read the same state from
//! background samplers and from the Mini overlay window.

use std::sync::Arc;

use crate::metrics::MetricsEngine;
use crate::services::metrics::build_engine;

/// Root state object managed by Tauri.
#[derive(Debug)]
pub struct AppState {
    /// The metrics engine, built once at startup.
    ///
    /// `Arc` rather than a bare value so that future background tasks and
    /// additional windows can hold their own handle without rebuilding the
    /// catalog. Sampling takes `&self`, so no lock is needed on the read path.
    metrics: Arc<MetricsEngine>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(build_engine()),
        }
    }

    /// The shared metrics engine.
    pub fn metrics(&self) -> &MetricsEngine {
        &self.metrics
    }

    /// A cloneable handle to the engine, for background work and other windows.
    pub fn metrics_handle(&self) -> Arc<MetricsEngine> {
        Arc::clone(&self.metrics)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_is_built_once_and_shared() {
        let state = AppState::new();

        let first = state.metrics_handle();
        let second = state.metrics_handle();

        assert!(
            Arc::ptr_eq(&first, &second),
            "handles must point at the same engine, not rebuild it"
        );
        assert_eq!(Arc::strong_count(&first), 3, "state plus two handles");
    }

    #[test]
    fn state_is_shareable_across_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AppState>();
    }

    #[test]
    fn the_engine_is_populated_from_the_host_platform() {
        let state = AppState::new();
        let status = state.metrics().status();

        if crate::platform::PlatformKind::current().is_supported() {
            assert_eq!(status.provider_count, 5);
            // The catalog is sized by the machine: 13 fixed metrics, plus
            // three per logical processor, eleven per GPU, thirteen per
            // storage device, four per volume, eleven per network interface
            // and four per Wi-Fi interface. Nothing here may assume a number.
            let count_of = |key: &str| {
                state
                    .metrics()
                    .catalog()
                    .iter()
                    .filter(|definition| definition.metric.key.as_str() == key)
                    .count()
            };
            let logical = count_of(crate::metrics::wellknown::cpu::USAGE_LOGICAL);
            let gpus = count_of(crate::metrics::wellknown::gpu::USAGE_CORE);
            let packages = count_of(crate::metrics::wellknown::cpu::TEMPERATURE_PACKAGE);
            let devices = count_of(crate::metrics::wellknown::storage::CAPACITY_TOTAL);
            let volumes = count_of(crate::metrics::wellknown::storage::VOLUME_CAPACITY_TOTAL);
            let interfaces = count_of(crate::metrics::wellknown::network::MTU);
            let wireless = count_of(crate::metrics::wellknown::network::WIFI_SIGNAL_RSSI);
            assert!(logical > 0);
            assert_eq!(
                status.metric_count,
                13 + 3 * logical
                    + packages
                    + crate::metrics::wellknown::gpu::PER_GPU_KEYS.len() * gpus
                    + crate::metrics::wellknown::storage::PER_DEVICE_KEYS.len() * devices
                    + crate::metrics::wellknown::storage::PER_VOLUME_KEYS.len() * volumes
                    + crate::metrics::wellknown::network::PER_INTERFACE_KEYS.len() * interfaces
                    + crate::metrics::wellknown::network::PER_WIFI_KEYS.len() * wireless
            );
        } else {
            assert_eq!(status.provider_count, 0);
        }
    }
}
