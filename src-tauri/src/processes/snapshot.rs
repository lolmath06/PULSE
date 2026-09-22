//! The payload `get_process_snapshot` returns.
//!
//! Mirrored in `src/types/processes.ts`; the serialisation tests at the bottom
//! of this file pin the wire format so the two cannot drift apart silently.
//!
//! # Why this is not the metric catalog
//!
//! A machine runs several hundred processes, most of them for less than a
//! second. Publishing `process.cpu@process:1234-9001` and five siblings for
//! each of them would add roughly two thousand [`MetricDefinition`]s, replace
//! most of them every refresh, and make the catalog — which exists so a saved
//! dashboard can resolve a reference months later — a churn of identifiers
//! that will never resolve again.
//!
//! So PULSE publishes exactly three low-cardinality process metrics through
//! the engine (see [`wellknown::process`]) and everything else through this
//! snapshot, which is requested, rendered and discarded. The reasoning is
//! written out in `docs/metrics/processes.md`.
//!
//! [`MetricDefinition`]: crate::metrics::model::MetricDefinition
//! [`wellknown::process`]: crate::metrics::wellknown::process

use serde::{Deserialize, Serialize};

use crate::metrics::model::TimestampMs;

use super::application::ApplicationEntry;
use super::field::Field;
use super::identity::ProcessInstanceId;
use super::state::{ProcessClass, ProcessState};

/// The machine-wide counts, identical to what the process provider publishes.
///
/// Carried here too so the interface's header needs one call rather than two,
/// and so the header can never disagree with the table below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessCounts {
    pub total: u32,
    pub running: u32,
    /// The sum of every visible process's thread count. Processes whose thread
    /// count could not be read contribute nothing rather than a guess.
    pub threads: u32,
}

/// One process, as the interface renders it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessEntry {
    /// `process:<pid>-<start token>`. Stable for this incarnation, and
    /// different for the next process to receive the same PID.
    pub instance_id: String,
    pub pid: u32,
    pub parent_pid: Option<u32>,
    /// The short name the operating system knows this process by.
    ///
    /// **Never the command line.** Arguments routinely carry file paths,
    /// URLs, API tokens and passwords; PULSE has no use for them and does not
    /// collect them. See the privacy section of `docs/metrics/processes.md`.
    pub name: String,
    /// The executable's path, for disambiguating same-named programs.
    ///
    /// Surfaced only as a tooltip, never as a column.
    pub executable_path: Field<String>,
    pub state: ProcessState,
    /// Why the state is unknown, where the platform reports none.
    pub state_availability: crate::metrics::model::Availability,
    pub classification: ProcessClass,
    /// Share of the **whole machine's** CPU capacity, 0–100.
    /// See [`rates`](super::rates) for the convention.
    pub cpu_percent: Field<f64>,
    /// Physical memory currently resident: Linux RSS, Windows working set.
    pub resident_memory_bytes: Field<f64>,
    /// `resident_memory_bytes` as a share of installed physical memory.
    pub memory_percent: Field<f64>,
    pub thread_count: Field<f64>,
    pub read_bytes_per_second: Field<f64>,
    pub write_bytes_per_second: Field<f64>,
    /// Which [`ApplicationEntry`] this process was grouped into.
    pub application_key: String,
}

impl ProcessEntry {
    /// The identity this entry was built from.
    pub fn instance(&self) -> ProcessInstanceId {
        ProcessInstanceId::new(self.pid, self.start_token())
    }

    fn start_token(&self) -> u64 {
        self.instance_id
            .rsplit('-')
            .next()
            .and_then(|token| token.parse().ok())
            .unwrap_or(0)
    }
}

/// One pass over the process table, complete.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSnapshot {
    /// Unix epoch milliseconds — directly usable as `new Date(takenAt)`.
    pub taken_at: TimestampMs,
    /// How long the collection itself took, in milliseconds.
    ///
    /// Reported rather than hidden: a process list is the one part of PULSE
    /// whose cost scales with the machine, and the number is what a later
    /// phase would optimise against.
    pub duration_ms: u64,
    pub counts: ProcessCounts,
    /// Every visible process, ordered by CPU descending then name then PID.
    pub processes: Vec<ProcessEntry>,
    /// The same processes grouped into applications, same ordering rule.
    pub applications: Vec<ApplicationEntry>,
    /// Set when the platform has no process collector at all.
    pub unsupported_reason: Option<String>,
}

impl ProcessSnapshot {
    /// The answer for a platform PULSE has no process collector for.
    ///
    /// An empty list plus a reason, never an error: the rest of the interface
    /// must keep working.
    pub fn unsupported(reason: impl Into<String>) -> Self {
        Self {
            taken_at: crate::metrics::model::now_ms(),
            duration_ms: 0,
            counts: ProcessCounts::default(),
            processes: Vec::new(),
            applications: Vec::new(),
            unsupported_reason: Some(reason.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::model::Availability;

    fn entry() -> ProcessEntry {
        ProcessEntry {
            instance_id: "process:1234-9001".to_string(),
            pid: 1234,
            parent_pid: Some(1),
            name: "firefox".to_string(),
            executable_path: Field::available("/usr/lib64/firefox/firefox".to_string()),
            state: ProcessState::SleepingOrWaiting,
            state_availability: Availability::Available,
            classification: ProcessClass::UserApplication,
            cpu_percent: Field::available(3.125),
            resident_memory_bytes: Field::available(512.0 * 1024.0 * 1024.0),
            memory_percent: Field::available(1.6),
            thread_count: Field::available(112.0),
            read_bytes_per_second: Field::available(0.0),
            write_bytes_per_second: Field::waiting_for_another_sample(),
            application_key: "exe:/usr/lib64/firefox/firefox".to_string(),
        }
    }

    #[test]
    fn a_process_entry_serialises_in_camel_case_for_the_frontend() {
        let json = serde_json::to_value(entry()).expect("serialise");

        assert_eq!(json["instanceId"], "process:1234-9001");
        assert_eq!(json["pid"], 1234);
        assert_eq!(json["parentPid"], 1);
        assert_eq!(json["state"], "sleepingOrWaiting");
        assert_eq!(json["classification"], "userApplication");
        assert_eq!(json["cpuPercent"]["value"], 3.125);
        assert_eq!(json["readBytesPerSecond"]["value"], 0.0);
        assert!(json["writeBytesPerSecond"]["value"].is_null());
        assert_eq!(
            json["writeBytesPerSecond"]["availability"]["status"],
            "temporarilyUnavailable"
        );
        assert!(json.get("resident_memory_bytes").is_none());
        assert_eq!(json["applicationKey"], "exe:/usr/lib64/firefox/firefox");
    }

    #[test]
    fn a_process_entry_never_carries_a_command_line() {
        let json = serde_json::to_value(entry()).expect("serialise");
        let fields: Vec<&String> = json.as_object().expect("object").keys().collect();

        for forbidden in ["commandLine", "arguments", "environment", "cmdline", "argv"] {
            assert!(
                !fields.iter().any(|field| field.as_str() == forbidden),
                "the wire format must never carry '{forbidden}'"
            );
        }
    }

    #[test]
    fn an_entry_round_trips_through_its_instance_id() {
        assert_eq!(entry().instance(), ProcessInstanceId::new(1234, 9001));
    }

    #[test]
    fn a_snapshot_round_trips() {
        let snapshot = ProcessSnapshot {
            taken_at: 1_700_000_000_000,
            duration_ms: 11,
            counts: ProcessCounts {
                total: 342,
                running: 2,
                threads: 1_204,
            },
            processes: vec![entry()],
            applications: Vec::new(),
            unsupported_reason: None,
        };

        let json = serde_json::to_string(&snapshot).expect("serialise");
        let parsed: ProcessSnapshot = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(parsed, snapshot);

        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(value["takenAt"], 1_700_000_000_000_u64);
        assert_eq!(value["durationMs"], 11);
        assert_eq!(value["counts"]["threads"], 1_204);
        assert!(value["unsupportedReason"].is_null());
    }

    #[test]
    fn an_unsupported_platform_returns_an_explained_empty_snapshot_not_an_error() {
        let snapshot = ProcessSnapshot::unsupported("PULSE has no process collector here");

        assert!(snapshot.processes.is_empty());
        assert!(snapshot.applications.is_empty());
        assert_eq!(snapshot.counts, ProcessCounts::default());
        assert!(snapshot.unsupported_reason.is_some());
    }
}
