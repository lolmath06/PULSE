//! The contract a platform implements to enumerate processes.
//!
//! Deliberately **one call per refresh**, not one call per process. The
//! interface between the process service and the operating system is a single
//! pass that returns everything, because the alternative shapes — a command
//! per process, an IPC round trip per process, a thread per process — are what
//! make process monitors slow and are explicitly refused here.
//!
//! A collector reads counters and never derives anything: no rate, no
//! percentage, no grouping. That arithmetic is shared, so Fedora and Windows
//! cannot drift apart on what "12 % CPU" means.

use super::raw::RawProcessScan;

/// A platform's process table reader.
pub trait ProcessCollector: Send + Sync + std::fmt::Debug {
    /// Walks the whole process table once.
    ///
    /// Must not panic and must not fail as a whole: a process that exits
    /// mid-walk, a handle that is refused, a counter file that cannot be
    /// opened are all *expected*, and each costs that process's affected
    /// fields and nothing else. A collector that can find nothing returns an
    /// empty scan, which is a fact rather than an error.
    fn collect(&self) -> RawProcessScan;
}
