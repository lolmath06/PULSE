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
//! # Read-only, always
//!
//! This module **observes**. It does not terminate, suspend, resume, renice,
//! debug, inject into or read the memory of any process, and there is no code
//! path here that could. Every interface it touches — `/proc`,
//! `CreateToolhelp32Snapshot`, `GetProcessTimes` — is a read.

pub mod application;
pub mod collector;
pub mod field;
pub mod identity;
pub mod rates;
pub mod raw;
pub mod service;
pub mod snapshot;
pub mod state;

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
