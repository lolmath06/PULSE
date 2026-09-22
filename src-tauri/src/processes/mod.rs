//! Process and application monitoring.
//!
//! PULSE answers two different questions about processes, and they have
//! different shapes:
//!
//! | Question | Answered by | Cardinality |
//! |---|---|---|
//! | How many processes and threads exist? | the metrics engine | 3 metrics, forever |
//! | Which programs are using this machine? | [`ProcessSnapshotService`] | hundreds of rows, gone in seconds |
//!
//! Only the first belongs in the metric catalog. The reasoning is in
//! [`service`] and, for the reader who wants the whole argument,
//! `docs/metrics/processes.md`.
//!
//! # Observation and control are separate services
//!
//! | Service | Does | Triggered by |
//! |---|---|---|
//! | [`ProcessSnapshotService`] | walks the whole table, read-only | *Refresh* |
//! | [`ProcessInspectorService`] | reads one process in depth, read-only | selecting a row |
//! | [`ProcessControlService`] | terminates, suspends, reprioritises one process | an explicit click, confirmed where destructive |
//!
//! The snapshot and inspector paths contain no call that changes a process.
//! Control lives in [`control`] alone, is never reachable from the metrics
//! engine or a provider, and acts only on a [`ProcessInstanceId`] it has
//! re-validated immediately before acting — never on a bare PID, never on a
//! name. See `docs/processes/controls.md`.

pub mod action;
pub mod application;
pub mod collector;
pub mod control;
pub mod field;
pub mod hash;
pub mod identity;
pub mod inspector;
pub mod rates;
pub mod raw;
pub mod search;
pub mod service;
pub mod snapshot;
pub mod state;

pub use action::{ProcessActionResult, ProcessActionStatus, ProcessQuery, TreeTerminationSummary};
pub use control::{
    ProcessAffinity, ProcessControlBackend, ProcessControlService, ProcessPriority, TerminateMode,
};
pub use hash::FileHash;
pub use inspector::{ProcessDetails, ProcessInspectorBackend, ProcessInspectorService, Provenance};

pub use application::{
    aggregate, application_key, display_name_for, ApplicationEntry, ApplicationIdentity,
};
pub use collector::ProcessCollector;
pub use field::Field;
pub use identity::ProcessInstanceId;
pub use rates::{
    cpu_percent, per_second, CounterSnapshot, ProcessCounters, ProcessRateTracker, ProcessRates,
    Rate, Rates,
};
pub use raw::{RawProcess, RawProcessIo, RawProcessScan};
pub use service::{counts, sort_processes, ProcessSnapshotService};
pub use snapshot::{ProcessCounts, ProcessEntry, ProcessSnapshot};
pub use state::{ProcessClass, ProcessState};
