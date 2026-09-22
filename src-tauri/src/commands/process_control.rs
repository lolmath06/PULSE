//! Tauri commands for the Process Inspector and process controls.
//!
//! Small and targeted: one command per question or action, each taking the
//! `process:<pid>-<token>` identity the frontend received in a snapshot. Each
//! command:
//!
//! 1. parses that identity strictly — a malformed one is `invalidRequest`
//!    before anything touches the system;
//! 2. hands it to a service that re-validates PID **and** start token
//!    immediately before reading or acting;
//! 3. returns a structured outcome, never a panic and never a bare error
//!    string for an expected situation.
//!
//! Everything runs on a blocking worker, not the UI thread: hashing a large
//! executable, asking `rpm` or `WinVerifyTrust`, or ending a process tree can
//! take a noticeable moment.
//!
//! # The only outbound doors
//!
//! [`open_web_search`] and [`open_hash_lookup`] hand a URL to the default
//! browser, and [`open_process_location`] asks the file manager to reveal a
//! file. PULSE itself makes no network request anywhere in this file.

use tauri::State;

use crate::processes::control::{ProcessAffinity, ProcessPriority, TerminateMode};
use crate::processes::search::{hash_search_url, virustotal_url, web_search_url, PrivateContext};
use crate::processes::{
    FileHash, ProcessActionResult, ProcessDetails, ProcessInstanceId, ProcessQuery, Provenance,
};
use crate::state::AppState;

/// Async commands that borrow Tauri state must return a `Result`. Every
/// expected outcome is carried *inside* the `Ok` value, so the error side is
/// never produced; it exists only to satisfy that signature.
type NeverError = String;

fn parse(instance_id: &str) -> Result<ProcessInstanceId, ProcessActionResult> {
    ProcessInstanceId::parse(instance_id).map_err(ProcessActionResult::invalid)
}

/// Runs `work` off the UI thread. A worker that panicked is reported as a
/// platform error rather than crashing the command.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    failed: impl FnOnce() -> T,
) -> T {
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(value) => value,
        Err(_) => failed(),
    }
}

fn worker_failed() -> ProcessActionResult {
    ProcessActionResult::new(
        crate::processes::ProcessActionStatus::PlatformError,
        "The request could not be completed.",
    )
}

/// Reads the inspector's cheap facts about exactly this instance.
#[tauri::command]
pub async fn get_process_details(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<ProcessQuery<ProcessDetails>, NeverError> {
    let id = match parse(&instance_id) {
        Ok(id) => id,
        Err(result) => return Ok(ProcessQuery::failed(result)),
    };
    let inspector = state.inspector_handle();
    Ok(blocking(
        move || inspector.details(id),
        || ProcessQuery::failed(worker_failed()),
    )
    .await)
}

/// Reads the package (Fedora) or signature (Windows) behind the executable.
#[tauri::command]
pub async fn get_process_provenance(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<ProcessQuery<Provenance>, NeverError> {
    let id = match parse(&instance_id) {
        Ok(id) => id,
        Err(result) => return Ok(ProcessQuery::failed(result)),
    };
    let inspector = state.inspector_handle();
    Ok(blocking(
        move || inspector.provenance(id),
        || ProcessQuery::failed(worker_failed()),
    )
    .await)
}

/// Hashes the executable — only ever on an explicit click.
#[tauri::command]
pub async fn compute_process_sha256(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<ProcessQuery<FileHash>, NeverError> {
    let id = match parse(&instance_id) {
        Ok(id) => id,
        Err(result) => return Ok(ProcessQuery::failed(result)),
    };
    let inspector = state.inspector_handle();
    Ok(blocking(
        move || inspector.sha256(id),
        || ProcessQuery::failed(worker_failed()),
    )
    .await)
}

/// Re-reads the priority, e.g. after a change.
#[tauri::command]
pub async fn get_process_priority(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<ProcessQuery<ProcessPriority>, NeverError> {
    let id = match parse(&instance_id) {
        Ok(id) => id,
        Err(result) => return Ok(ProcessQuery::failed(result)),
    };
    let control = state.control_handle();
    Ok(blocking(
        move || match control.priority(id) {
            Ok(priority) => ProcessQuery::ok(priority),
            Err(result) => ProcessQuery::failed(result),
        },
        || ProcessQuery::failed(worker_failed()),
    )
    .await)
}

/// Re-reads the CPU affinity, e.g. after a change.
#[tauri::command]
pub async fn get_process_affinity(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<ProcessQuery<ProcessAffinity>, NeverError> {
    let id = match parse(&instance_id) {
        Ok(id) => id,
        Err(result) => return Ok(ProcessQuery::failed(result)),
    };
    let control = state.control_handle();
    Ok(blocking(
        move || match control.affinity(id) {
            Ok(affinity) => ProcessQuery::ok(affinity),
            Err(result) => ProcessQuery::failed(result),
        },
        || ProcessQuery::failed(worker_failed()),
    )
    .await)
}

/// *Suspend*.
#[tauri::command]
pub async fn suspend_process(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<ProcessActionResult, NeverError> {
    let id = match parse(&instance_id) {
        Ok(id) => id,
        Err(result) => return Ok(result),
    };
    let control = state.control_handle();
    Ok(blocking(move || control.suspend(id), worker_failed).await)
}

/// *Resume* — only what PULSE suspended.
#[tauri::command]
pub async fn resume_process(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<ProcessActionResult, NeverError> {
    let id = match parse(&instance_id) {
        Ok(id) => id,
        Err(result) => return Ok(result),
    };
    let control = state.control_handle();
    Ok(blocking(move || control.resume(id), worker_failed).await)
}

/// *End process*, or with `force` the Linux-only *Force kill*.
#[tauri::command]
pub async fn terminate_process(
    state: State<'_, AppState>,
    instance_id: String,
    force: bool,
) -> Result<ProcessActionResult, NeverError> {
    let id = match parse(&instance_id) {
        Ok(id) => id,
        Err(result) => return Ok(result),
    };
    let mode = if force {
        TerminateMode::Force
    } else {
        TerminateMode::Graceful
    };
    let control = state.control_handle();
    Ok(blocking(move || control.terminate(id, mode), worker_failed).await)
}

/// *End process tree*.
#[tauri::command]
pub async fn terminate_process_tree(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<ProcessActionResult, NeverError> {
    let id = match parse(&instance_id) {
        Ok(id) => id,
        Err(result) => return Ok(result),
    };
    let control = state.control_handle();
    Ok(blocking(move || control.terminate_tree(id), worker_failed).await)
}

/// *Set priority*. `confirm_realtime` must be true for Windows *Realtime*.
#[tauri::command]
pub async fn set_process_priority(
    state: State<'_, AppState>,
    instance_id: String,
    priority: ProcessPriority,
    confirm_realtime: bool,
) -> Result<ProcessActionResult, NeverError> {
    let id = match parse(&instance_id) {
        Ok(id) => id,
        Err(result) => return Ok(result),
    };
    let control = state.control_handle();
    Ok(blocking(
        move || control.set_priority(id, priority, confirm_realtime),
        worker_failed,
    )
    .await)
}

/// *Set affinity*.
#[tauri::command]
pub async fn set_process_affinity(
    state: State<'_, AppState>,
    instance_id: String,
    cpus: Vec<u32>,
) -> Result<ProcessActionResult, NeverError> {
    let id = match parse(&instance_id) {
        Ok(id) => id,
        Err(result) => return Ok(result),
    };
    let control = state.control_handle();
    Ok(blocking(move || control.set_affinity(id, &cpus), worker_failed).await)
}

/// *Open file location*: re-resolves the executable of exactly this instance
/// and asks the file manager to reveal it. No shell, no interpolated command.
#[tauri::command]
pub async fn open_process_location(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<ProcessActionResult, NeverError> {
    let id = match parse(&instance_id) {
        Ok(id) => id,
        Err(result) => return Ok(result),
    };
    let inspector = state.inspector_handle();
    Ok(blocking(
        move || {
            let query = inspector.executable_path(id);
            let Some(path) = query.value else {
                return query.outcome;
            };
            match tauri_plugin_opener::reveal_item_in_dir(&path) {
                Ok(()) => ProcessActionResult::success("Opened the executable's location."),
                Err(error) => ProcessActionResult::new(
                    crate::processes::ProcessActionStatus::PlatformError,
                    format!("The file manager could not be opened: {error}"),
                ),
            }
        },
        worker_failed,
    )
    .await)
}

/// *Search online*: opens the default browser on a search for program names.
///
/// The terms are checked again here — no path, no home directory, no user
/// name — and percent-encoded into a fixed search provider's URL.
#[tauri::command]
pub fn open_web_search(terms: Vec<String>) -> ProcessActionResult {
    match web_search_url(&terms, &PrivateContext::current()) {
        Ok(url) => open_url(&url),
        Err(reason) => ProcessActionResult::invalid(reason),
    }
}

/// *Search hash online* (`target = "web"`) or *Check hash on VirusTotal*
/// (`target = "virusTotal"`): opens the page for a digest. Only the digest
/// is ever sent, never the file.
#[tauri::command]
pub fn open_hash_lookup(sha256: String, target: String) -> ProcessActionResult {
    let url = match target.as_str() {
        "web" => hash_search_url(&sha256),
        "virusTotal" => virustotal_url(&sha256),
        _ => Err(format!("'{target}' is not a hash lookup PULSE offers.")),
    };
    match url {
        Ok(url) => open_url(&url),
        Err(reason) => ProcessActionResult::invalid(reason),
    }
}

fn open_url(url: &str) -> ProcessActionResult {
    match tauri_plugin_opener::open_url(url, None::<&str>) {
        Ok(()) => ProcessActionResult::success("Opened in your browser."),
        Err(error) => ProcessActionResult::new(
            crate::processes::ProcessActionStatus::PlatformError,
            format!("The browser could not be opened: {error}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_malformed_identity_is_an_invalid_request() {
        for bad in ["", "1234", "process:0-1", "process:1-x", "../etc"] {
            assert_eq!(
                parse(bad).unwrap_err().status,
                crate::processes::ProcessActionStatus::InvalidRequest
            );
        }
        assert!(parse("process:1234-5678").is_ok());
    }

    #[test]
    fn a_hash_lookup_refuses_unknown_targets_and_bad_digests_before_opening_anything() {
        let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(
            open_hash_lookup(hash.into(), "upload".into()).status,
            crate::processes::ProcessActionStatus::InvalidRequest
        );
        assert_eq!(
            open_hash_lookup("not-a-hash".into(), "virusTotal".into()).status,
            crate::processes::ProcessActionStatus::InvalidRequest
        );
    }

    #[test]
    fn a_web_search_refuses_a_path_before_opening_anything() {
        assert_eq!(
            open_web_search(vec!["/home/alice/private/project/token-app".into()]).status,
            crate::processes::ProcessActionStatus::InvalidRequest
        );
    }
}
