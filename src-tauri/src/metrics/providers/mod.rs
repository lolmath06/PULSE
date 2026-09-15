//! Metric providers.
//!
//! A provider is the unit of ownership in the metrics engine: it declares a set
//! of metrics and knows how to read them. Future providers map onto one data
//! source each — `linux.cpu` reading `/proc/stat`, `windows.pdh` reading
//! performance counters, `nvidia.nvml` talking to the NVIDIA driver.
//!
//! Providers are **synchronous by design**. The system interfaces PULSE will
//! read — `/proc`, `/sys`, PDH, WMI, NVML — are blocking calls, and wrapping
//! them in an async runtime would add machinery without removing any blocking.
//! Scheduling and concurrency are the engine's concern in a later phase, not
//! the provider's.
//!
//! Providers must be `Send + Sync`: the engine is shared across Tauri commands
//! and, later, across windows and background tasks.

use crate::metrics::model::{MetricDefinition, MetricError, MetricRef, MetricSample, ProviderId};

#[cfg(test)]
pub mod mock;

/// A source of metrics.
///
/// Implementations must not panic. Every foreseeable failure has a
/// representation: a metric that cannot be read is an unavailable
/// [`MetricSample`], and a provider that cannot work at all returns
/// `Err(MetricError)`.
pub trait MetricProvider: Send + Sync {
    /// Stable identity, e.g. `linux.cpu`. Surfaced in engine status and errors
    /// so failures can be attributed precisely.
    fn id(&self) -> &ProviderId;

    /// Declares every metric this provider owns on this machine.
    ///
    /// Called once at registration. Metrics that exist conceptually but are not
    /// readable here should still be declared, with a non-available
    /// [`Availability`] explaining why — that is what lets the UI say "your
    /// board has no fan sensor" instead of silently omitting it.
    ///
    /// Returning `Err` means the provider could not enumerate anything and
    /// registration fails cleanly.
    ///
    /// [`Availability`]: crate::metrics::model::Availability
    fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError>;

    /// Reads the requested metrics.
    ///
    /// The engine only ever passes references this provider declared, already
    /// deduplicated. Implementations should return one sample per requested
    /// reference; any omission is turned into a `MissingSample` error by the
    /// engine rather than being silently dropped.
    ///
    /// Returning `Err` marks *this provider's* requested metrics as failed and
    /// leaves every other provider's results untouched.
    fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError>;
}
