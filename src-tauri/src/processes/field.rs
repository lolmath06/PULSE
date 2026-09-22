//! [`Field`] — one per-process datum with its own availability.
//!
//! Availability in PULSE is never a property of a whole row. A process can be
//! perfectly visible — named, counted, with a PID and a thread count — while
//! its I/O counters are refused and its executable path is unreadable:
//!
//! ```text
//! process        visible
//!   cpu          available
//!   memory       available
//!   read/write   permissionDenied
//!   exe path     permissionDenied
//! ```
//!
//! Dropping the row because two of its six fields are missing would hide a
//! process that is genuinely running. Reporting `0 B/s` for the refused
//! counters would be a lie. So each datum carries its own
//! [`Availability`], and the row survives as long as its inventory does.

use serde::{Deserialize, Serialize};

use crate::metrics::model::Availability;

/// A value that may or may not exist, with the reason attached.
///
/// Mirrors [`MetricSample`]'s value/availability split so the frontend can
/// reuse the same rendering logic for process rows and for catalog metrics.
///
/// [`MetricSample`]: crate::metrics::model::MetricSample
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field<T> {
    /// `None` whenever `availability` is not [`Availability::Available`].
    pub value: Option<T>,
    pub availability: Availability,
}

impl<T> Field<T> {
    /// A datum that was read.
    pub fn available(value: T) -> Self {
        Self {
            value: Some(value),
            availability: Availability::Available,
        }
    }

    /// A datum that was not read, and why.
    ///
    /// Debug-asserts the caller did not pass [`Availability::Available`],
    /// which would claim success with nothing to show.
    pub fn missing(availability: Availability) -> Self {
        debug_assert!(
            !availability.is_available(),
            "missing() requires a non-available status"
        );

        Self {
            value: None,
            availability,
        }
    }

    /// The reading a rate needs a second sample to produce.
    ///
    /// **Not zero.** Before a baseline exists there is no interval to divide
    /// by, so there is no measurement — and `0 B/s` would read as "measured,
    /// and idle". After a baseline, a genuine zero *is* reported as zero.
    pub fn waiting_for_another_sample() -> Self {
        Self::missing(Availability::temporarily_unavailable(
            "waiting for another sample",
        ))
    }

    pub fn is_available(&self) -> bool {
        self.value.is_some()
    }
}

impl<T> Default for Field<T> {
    fn default() -> Self {
        Self::missing(Availability::not_detected("not measured"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_available_field_carries_its_value() {
        let field = Field::available(42.0_f64);

        assert_eq!(field.value, Some(42.0));
        assert!(field.availability.is_available());
        assert!(field.is_available());
    }

    #[test]
    fn a_refused_field_keeps_the_reason_and_no_value() {
        let field: Field<f64> = Field::missing(Availability::permission_denied(
            "/proc/7/io is not readable",
        ));

        assert_eq!(field.value, None);
        assert_eq!(field.availability.status_str(), "permissionDenied");
        assert!(!field.is_available());
    }

    #[test]
    fn waiting_is_transient_and_never_zero() {
        let field: Field<f64> = Field::waiting_for_another_sample();

        assert_eq!(field.value, None, "a missing rate must never become 0");
        assert!(field.availability.is_transient());
        assert_eq!(field.availability.status_str(), "temporarilyUnavailable");
    }

    #[test]
    fn a_true_zero_stays_a_value() {
        let field = Field::available(0.0_f64);

        assert_eq!(field.value, Some(0.0));
        assert!(field.is_available());
    }

    #[test]
    fn serialises_for_the_frontend_as_value_plus_availability() {
        let json = serde_json::to_value(Field::available(1.5_f64)).expect("serialise");
        assert_eq!(json["value"], 1.5);
        assert_eq!(json["availability"]["status"], "available");

        let json =
            serde_json::to_value(Field::<f64>::waiting_for_another_sample()).expect("serialise");
        assert!(json["value"].is_null());
        assert_eq!(json["availability"]["status"], "temporarilyUnavailable");
    }
}
