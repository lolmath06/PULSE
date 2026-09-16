//! Well-known metric declarations shared by every platform.
//!
//! This module is the reason a dashboard configured on Fedora keeps working on
//! Windows. It owns, for each metric PULSE ships:
//!
//! - the [`MetricKey`] and [`SourceId`],
//! - the unit, category, kind and value type,
//! - the user-facing names,
//! - and the arithmetic that turns raw OS counters into the published value.
//!
//! Platform providers supply **only the raw numbers**. They never choose a key,
//! a unit, or a formula, which is what keeps `memory.used@memory:system`
//! meaning exactly the same thing on both operating systems.
//!
//! Nothing here is OS-specific, and nothing here may become OS-specific.
//!
//! [`MetricKey`]: crate::metrics::model::MetricKey
//! [`SourceId`]: crate::metrics::model::SourceId

pub mod cpu;
pub mod gpu;
pub mod memory;
pub mod units;

#[cfg(test)]
mod contract_tests;

use crate::metrics::model::{Availability, MetricError, MetricErrorCode};

/// Maps a failed system read to the availability state that describes it
/// honestly.
///
/// Shared by every provider on both platforms, because the distinction matters
/// more than the mechanism: a momentarily unreadable counter is not the same as
/// a missing permission, and neither means "your machine does not have a CPU".
/// Collapsing them into a generic error is what PULSE's availability contract
/// exists to prevent.
pub fn availability_for(error: MetricError) -> Availability {
    match error.code {
        // The user can act on this one.
        MetricErrorCode::PermissionDenied => Availability::permission_denied(error.message),
        // Transient: a busy /proc, a driver reloading. It will come back.
        MetricErrorCode::Io | MetricErrorCode::Timeout => {
            Availability::temporarily_unavailable(error.message)
        }
        // The capability genuinely is not there.
        MetricErrorCode::Unsupported => Availability::unsupported(error.message),
        MetricErrorCode::NotDetected => Availability::not_detected(error.message),
        // Everything else keeps the structured error for the UI to inspect.
        _ => Availability::provider_error(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_failures_keep_their_distinct_meanings() {
        let cases = [
            (MetricErrorCode::PermissionDenied, "permissionDenied"),
            (MetricErrorCode::Io, "temporarilyUnavailable"),
            (MetricErrorCode::Timeout, "temporarilyUnavailable"),
            (MetricErrorCode::Unsupported, "unsupported"),
            (MetricErrorCode::NotDetected, "notDetected"),
            (MetricErrorCode::Parse, "providerError"),
            (MetricErrorCode::Internal, "providerError"),
        ];

        for (code, expected) in cases {
            let availability = availability_for(MetricError::new(code, "detail"));
            assert_eq!(availability.status_str(), expected, "for {code:?}");
            assert!(!availability.is_available());
        }
    }

    #[test]
    fn a_transient_read_failure_is_never_reported_as_unsupported() {
        // Telling a user their hardware lacks a CPU counter because /proc was
        // busy for a moment would be actively misleading.
        let availability = availability_for(MetricError::new(MetricErrorCode::Io, "busy"));
        assert!(availability.is_transient());
    }
}
