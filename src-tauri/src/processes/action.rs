//! [`ProcessActionResult`] — the one answer every inspector and control
//! request returns.
//!
//! # Outcomes are distinct because the user's next step is
//!
//! | Status | Meaning | What the user can do |
//! |---|---|---|
//! | `success` | done | nothing |
//! | `permissionDenied` | the OS refused | run as that user; PULSE never elevates |
//! | `staleProcess` | the PID now belongs to a **different** process | nothing — PULSE refused to act on it |
//! | `processGone` | the process has exited | nothing — the goal is usually already met |
//! | `unsupported` | this platform cannot do this | nothing |
//! | `partialFailure` | some threads / children were refused | read the counts |
//! | `invalidRequest` | the request itself was wrong | fix the request |
//! | `platformError` | the OS failed for another reason | read the reason |
//!
//! Collapsing any two of these would lie to the user. A refused permission is
//! not "unsupported", and a process that already exited is not a failure of
//! *End process* — it is the outcome the user asked for.

use serde::{Deserialize, Serialize};

/// What happened to one requested action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessActionStatus {
    Success,
    PermissionDenied,
    StaleProcess,
    ProcessGone,
    Unsupported,
    PartialFailure,
    InvalidRequest,
    PlatformError,
}

impl ProcessActionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            ProcessActionStatus::Success => "success",
            ProcessActionStatus::PermissionDenied => "permissionDenied",
            ProcessActionStatus::StaleProcess => "staleProcess",
            ProcessActionStatus::ProcessGone => "processGone",
            ProcessActionStatus::Unsupported => "unsupported",
            ProcessActionStatus::PartialFailure => "partialFailure",
            ProcessActionStatus::InvalidRequest => "invalidRequest",
            ProcessActionStatus::PlatformError => "platformError",
        }
    }
}

/// The per-target tally of an *End process tree*.
///
/// Every process PULSE planned to end lands in exactly one bucket, so the
/// buckets always sum to `requested`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeTerminationSummary {
    /// The root plus every descendant found in the snapshot taken at the
    /// moment of the action.
    pub requested: u32,
    pub terminated: u32,
    /// Exited on its own before PULSE reached it.
    pub already_gone: u32,
    pub permission_denied: u32,
    /// The PID was alive but belonged to a different process than the one
    /// planned: **never signalled**.
    pub stale_skipped: u32,
    /// PULSE itself was a descendant and was left alone.
    pub skipped_self: u32,
    pub failed: u32,
}

impl TreeTerminationSummary {
    /// Whether every bucket adds up to what was planned.
    pub fn is_consistent(&self) -> bool {
        self.terminated
            + self.already_gone
            + self.permission_denied
            + self.stale_skipped
            + self.skipped_self
            + self.failed
            == self.requested
    }
}

/// The structured answer to one action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessActionResult {
    pub status: ProcessActionStatus,
    /// A sentence the interface can show as-is.
    pub reason: String,
    /// How many things the action reached — threads for Windows suspend,
    /// threads for a Linux priority change, processes for a tree.
    pub affected_count: Option<u32>,
    /// How many of those the operating system refused.
    pub failed_count: Option<u32>,
    /// Present for *End process tree* only.
    pub tree: Option<TreeTerminationSummary>,
}

impl ProcessActionResult {
    pub fn new(status: ProcessActionStatus, reason: impl Into<String>) -> Self {
        Self {
            status,
            reason: reason.into(),
            affected_count: None,
            failed_count: None,
            tree: None,
        }
    }

    pub fn success(reason: impl Into<String>) -> Self {
        Self::new(ProcessActionStatus::Success, reason)
    }

    pub fn stale(pid: u32) -> Self {
        Self::new(
            ProcessActionStatus::StaleProcess,
            format!(
                "PID {pid} now belongs to a different process than the one selected. \
                 PULSE did not act on it."
            ),
        )
    }

    pub fn gone(pid: u32) -> Self {
        Self::new(
            ProcessActionStatus::ProcessGone,
            format!("Process {pid} has already exited."),
        )
    }

    pub fn invalid(reason: impl Into<String>) -> Self {
        Self::new(ProcessActionStatus::InvalidRequest, reason)
    }

    pub fn unsupported(reason: impl Into<String>) -> Self {
        Self::new(ProcessActionStatus::Unsupported, reason)
    }

    pub fn with_counts(mut self, affected: u32, failed: u32) -> Self {
        self.affected_count = Some(affected);
        self.failed_count = Some(failed);
        self
    }

    pub fn is_success(&self) -> bool {
        self.status == ProcessActionStatus::Success
    }
}

/// A read-only query's answer: a value when the identity held, a reason when
/// it did not.
///
/// The inspector's calls use this rather than `Result` so that a stale or
/// vanished process reaches the interface as an ordinary, explained outcome
/// instead of a rejected promise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessQuery<T> {
    pub outcome: ProcessActionResult,
    pub value: Option<T>,
}

impl<T> ProcessQuery<T> {
    pub fn ok(value: T) -> Self {
        Self {
            outcome: ProcessActionResult::success("ok"),
            value: Some(value),
        }
    }

    pub fn failed(outcome: ProcessActionResult) -> Self {
        debug_assert!(!outcome.is_success());
        Self {
            outcome,
            value: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_serialise_as_their_documented_discriminants() {
        for status in [
            ProcessActionStatus::Success,
            ProcessActionStatus::PermissionDenied,
            ProcessActionStatus::StaleProcess,
            ProcessActionStatus::ProcessGone,
            ProcessActionStatus::Unsupported,
            ProcessActionStatus::PartialFailure,
            ProcessActionStatus::InvalidRequest,
            ProcessActionStatus::PlatformError,
        ] {
            let json = serde_json::to_string(&status).expect("serialise");
            assert_eq!(json, format!("\"{}\"", status.as_str()));
        }
    }

    #[test]
    fn a_result_serialises_in_camel_case() {
        let result = ProcessActionResult::stale(5000).with_counts(0, 0);
        let json = serde_json::to_value(&result).expect("serialise");

        assert_eq!(json["status"], "staleProcess");
        assert!(json["reason"].as_str().expect("text").contains("5000"));
        assert_eq!(json["affectedCount"], 0);
        assert_eq!(json["failedCount"], 0);
        assert!(json["tree"].is_null());
    }

    #[test]
    fn a_tree_summary_accounts_for_every_target() {
        let summary = TreeTerminationSummary {
            requested: 6,
            terminated: 2,
            already_gone: 1,
            permission_denied: 1,
            stale_skipped: 1,
            skipped_self: 0,
            failed: 1,
        };
        assert!(summary.is_consistent());

        let json = serde_json::to_value(summary).expect("serialise");
        assert_eq!(json["alreadyGone"], 1);
        assert_eq!(json["staleSkipped"], 1);
        assert_eq!(json["skippedSelf"], 0);
    }

    #[test]
    fn a_failed_query_carries_no_value() {
        let query: ProcessQuery<u32> = ProcessQuery::failed(ProcessActionResult::gone(7));
        let json = serde_json::to_value(&query).expect("serialise");

        assert!(json["value"].is_null());
        assert_eq!(json["outcome"]["status"], "processGone");
    }
}
