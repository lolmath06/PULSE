//! Tauri commands exposing metric history.
//!
//! Read-only: the UI can ask what history holds and how the recorder is doing.
//! It cannot write a sample — only the scheduler in `HistoryService` does — so
//! no button, chart or widget can ever add rows to the database.
//!
//! Queries run on a blocking worker: a week-long answer reads tens of
//! thousands of rows, which must never happen on the thread serving the UI.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::history::{HistoryRange, HistoryResponse, HistoryService, HistoryStatus};
use crate::metrics::MetricRef;

/// The outcome of a history request. Every expected failure — history
/// unavailable, a bad request — is a value, never a rejected promise.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum HistoryQueryResult {
    Ok(HistoryResponse),
    Unavailable { reason: String },
}

/// Async commands that borrow Tauri state must return a `Result`; every
/// outcome travels inside `Ok`.
type NeverError = String;

/// Reports whether history is recording, where, and how fast.
///
/// `includeDatabase` adds row counts and file sizes; it scans the tables, so
/// the UI asks for it only on demand.
#[tauri::command]
pub async fn get_history_status(
    history: State<'_, Arc<HistoryService>>,
    include_database: Option<bool>,
) -> Result<HistoryStatus, NeverError> {
    let include = include_database.unwrap_or(false);
    if !include {
        return Ok(history.status(false));
    }
    let service = Arc::clone(history.inner());
    Ok(
        tauri::async_runtime::spawn_blocking(move || service.status(true))
            .await
            .unwrap_or_else(|_| history.status(false)),
    )
}

/// Returns the history of `metrics` over `range`, at a resolution the backend
/// chooses so that no series exceeds roughly 720 points.
#[tauri::command]
pub async fn get_metric_history(
    history: State<'_, Arc<HistoryService>>,
    metrics: Vec<MetricRef>,
    range: HistoryRange,
) -> Result<HistoryQueryResult, NeverError> {
    let service = Arc::clone(history.inner());
    let answer = tauri::async_runtime::spawn_blocking(move || service.query(&metrics, range))
        .await
        .map_err(|_| "the history worker failed".to_string());

    Ok(match answer {
        Ok(Ok(response)) => HistoryQueryResult::Ok(response),
        Ok(Err(error)) => HistoryQueryResult::Unavailable {
            reason: error.to_string(),
        },
        Err(reason) => HistoryQueryResult::Unavailable { reason },
    })
}
