//! Tauri commands exposing the metrics engine to the frontend.
//!
//! Thin wrappers: they read the shared engine out of Tauri state and delegate.
//! No platform branching, no logic.

use tauri::State;

use crate::metrics::{EngineStatus, MetricDefinition, MetricRef, MetricSample};
use crate::services::metrics as service;
use crate::state::AppState;

/// Reports schema version, provider count, metric counts and overall state.
#[tauri::command]
pub fn get_metrics_engine_status(state: State<'_, AppState>) -> EngineStatus {
    service::status(state.metrics())
}

/// Returns the metadata of every registered metric, ordered deterministically.
///
/// Empty while PULSE has no system providers — see
/// [`crate::metrics::build_engine`].
#[tauri::command]
pub fn get_metric_catalog(state: State<'_, AppState>) -> Vec<MetricDefinition> {
    service::catalog(state.metrics())
}

/// Samples the requested metrics.
///
/// Returns one sample per requested reference, in the same order. Unknown
/// references and failing providers produce unavailable samples rather than
/// failing the call.
///
/// A malformed reference is rejected by deserialisation before reaching this
/// function, so the frontend receives a clean error instead of a panic.
#[tauri::command]
pub fn sample_metrics(state: State<'_, AppState>, metrics: Vec<MetricRef>) -> Vec<MetricSample> {
    service::sample(state.metrics(), &metrics)
}

/// `sourceId → persistable reference` for every catalog source.
///
/// Saved widget bindings store the persistable form, so no MAC-derived or
/// serial-derived identifier ever reaches the configuration file.
#[tauri::command]
pub fn get_source_refs(state: State<'_, AppState>) -> std::collections::BTreeMap<String, String> {
    service::persistable_source_refs(state.metrics())
}
