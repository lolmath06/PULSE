//! The Tauri command exposing the process snapshot service.
//!
//! Deliberately **its own command** rather than a detour through
//! `sample_metrics`. That call is shaped for tens of references a dashboard
//! has saved; a process snapshot is several hundred rows that exist only until
//! the next refresh. Routing one through the other would make every metric
//! request carry the cost of the process table, and would put ephemeral
//! per-PID references into a call whose contract is that references are
//! stable. See `docs/metrics/processes.md`.

use tauri::State;

use crate::processes::ProcessSnapshot;
use crate::services::processes as service;
use crate::state::AppState;

/// Walks the process table once and returns everything derived from it.
///
/// One call per *Refresh*, whatever the machine runs — never one call per
/// process. Rates come from the interval between this snapshot and the
/// previous one; the first snapshot reports them as waiting rather than as
/// zero.
#[tauri::command]
pub fn get_process_snapshot(state: State<'_, AppState>) -> ProcessSnapshot {
    service::snapshot(state.processes())
}
