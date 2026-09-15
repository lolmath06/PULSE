//! [`MetricSample`] — one measurement at one instant.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::availability::Availability;
use super::error::{MetricError, MetricErrorCode};
use super::reference::MetricRef;
use super::value::MetricValue;

/// Unix epoch milliseconds.
///
/// Chosen because it is exactly what JavaScript's `Date` and every charting
/// library expect, so no conversion is needed on the frontend. `u64` overflows
/// in the year 584 million; `i64` would be needed only for pre-1970 timestamps,
/// which cannot occur here.
pub type TimestampMs = u64;

/// Returns the current time as Unix epoch milliseconds.
///
/// A system clock set before 1970 yields `0` rather than a panic — PULSE must
/// never die because of a misconfigured clock.
pub fn now_ms() -> TimestampMs {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// A single measurement.
///
/// `value` is `None` whenever `availability` is not [`Availability::Available`].
/// The two are kept as separate fields rather than folded into one enum so the
/// frontend can render "last known state plus why it is missing" without
/// destructuring twice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricSample {
    pub metric: MetricRef,
    /// Unix epoch milliseconds — directly usable as `new Date(timestamp)`.
    pub timestamp: TimestampMs,
    pub value: Option<MetricValue>,
    pub availability: Availability,
}

impl MetricSample {
    /// A successful reading, stamped now.
    pub fn available(metric: MetricRef, value: MetricValue) -> Self {
        Self {
            metric,
            timestamp: now_ms(),
            value: Some(value),
            availability: Availability::Available,
        }
    }

    /// A numeric reading, stamped now.
    ///
    /// A non-finite float is not a reading: it is reported as a provider error
    /// rather than silently serialised as JSON `null`.
    pub fn number(metric: MetricRef, value: f64) -> Self {
        match MetricValue::number(value) {
            Some(value) => Self::available(metric, value),
            None => Self::unavailable(
                metric,
                Availability::provider_error(
                    MetricError::new(
                        MetricErrorCode::Parse,
                        "provider produced a non-finite number",
                    )
                    .with_recoverable(true),
                ),
            ),
        }
    }

    /// A reading that could not be taken, stamped now.
    ///
    /// Debug-asserts that the caller did not pass [`Availability::Available`],
    /// which would produce a sample claiming success with no value.
    pub fn unavailable(metric: MetricRef, availability: Availability) -> Self {
        debug_assert!(
            !availability.is_available(),
            "unavailable() requires a non-available status"
        );

        Self {
            metric,
            timestamp: now_ms(),
            value: None,
            availability,
        }
    }

    /// Overrides the timestamp. Used by tests and by future replay tooling.
    pub fn at(mut self, timestamp: TimestampMs) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Whether this sample carries a usable value.
    pub const fn has_value(&self) -> bool {
        self.value.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference() -> MetricRef {
        MetricRef::parse("cpu.usage.total", "cpu:0").expect("valid")
    }

    #[test]
    fn an_available_sample_serialises_with_its_value() {
        let sample = MetricSample::number(reference(), 37.5).at(1_700_000_000_000);
        let json = serde_json::to_value(&sample).expect("serialise");

        assert_eq!(json["metric"]["key"], "cpu.usage.total");
        assert_eq!(json["metric"]["sourceId"], "cpu:0");
        assert_eq!(json["timestamp"], 1_700_000_000_000_u64);
        assert_eq!(json["value"]["type"], "number");
        assert_eq!(json["value"]["value"], 37.5);
        assert_eq!(json["availability"]["status"], "available");
    }

    #[test]
    fn an_unavailable_sample_serialises_a_null_value_and_a_reason() {
        let sample = MetricSample::unavailable(
            reference(),
            Availability::permission_denied("needs administrator"),
        );
        let json = serde_json::to_value(&sample).expect("serialise");

        assert!(json["value"].is_null());
        assert_eq!(json["availability"]["status"], "permissionDenied");
        assert_eq!(json["availability"]["reason"], "needs administrator");
    }

    #[test]
    fn exposes_exactly_the_expected_field_set() {
        let json = serde_json::to_value(MetricSample::number(reference(), 1.0)).expect("serialise");
        let mut fields: Vec<&str> = json
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();

        assert_eq!(fields, ["availability", "metric", "timestamp", "value"]);
    }

    #[test]
    fn a_non_finite_reading_becomes_a_provider_error_not_a_null_number() {
        // Without this guard serde_json emits `"value": null` while still
        // claiming the sample is available.
        let sample = MetricSample::number(reference(), f64::NAN);

        assert!(!sample.has_value());
        assert!(!sample.availability.is_available());
        assert!(matches!(
            sample.availability,
            Availability::ProviderError { .. }
        ));
        assert!(sample.availability.is_transient());
    }

    #[test]
    fn timestamps_are_epoch_milliseconds() {
        let sample = MetricSample::number(reference(), 1.0);

        // Sanity bracket: after 2020-01-01 and before 2100-01-01, in ms.
        assert!(sample.timestamp > 1_577_836_800_000);
        assert!(sample.timestamp < 4_102_444_800_000);
    }

    #[test]
    fn round_trips_through_json() {
        for sample in [
            MetricSample::number(reference(), -12.5).at(42),
            MetricSample::available(reference(), MetricValue::boolean(true)).at(43),
            MetricSample::unavailable(reference(), Availability::not_detected("absent")).at(44),
        ] {
            let json = serde_json::to_string(&sample).expect("serialise");
            assert_eq!(
                serde_json::from_str::<MetricSample>(&json).expect("deserialise"),
                sample
            );
        }
    }

    #[test]
    fn now_ms_is_monotonic_enough_to_order_samples() {
        let first = now_ms();
        let second = now_ms();
        assert!(second >= first);
    }
}
