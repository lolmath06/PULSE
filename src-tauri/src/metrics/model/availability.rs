//! Metric availability.
//!
//! This is one of the most important types in PULSE. Not every machine exposes
//! every sensor, and the difference between the reasons matters enormously to
//! the user:
//!
//! - *"Your laptop has no discrete GPU"* — nothing to fix.
//! - *"Fedora will not let PULSE read this without elevation"* — fixable, and
//!   the user should be told how.
//! - *"The driver hiccupped this second"* — ignore it, it will come back.
//! - *"PULSE does not implement this on Windows yet"* — our problem, not theirs.
//!
//! Collapsing all four into a generic error, or into a silent `0`, is the
//! single most common failure of system monitors. The contract refuses to.

use serde::{Deserialize, Serialize};

use super::error::MetricError;

/// Why a metric can or cannot currently produce a value.
///
/// Serialised as a discriminated union on `status`, so the frontend can switch
/// exhaustively:
/// `{"status":"available"}`, `{"status":"notDetected","reason":"…"}`,
/// `{"status":"providerError","error":{…}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum Availability {
    /// The metric can be sampled.
    Available,

    /// This platform or PULSE build has no implementation for the metric.
    /// Example: motherboard fan RPM on Windows without a kernel helper.
    Unsupported { reason: String },

    /// The capability exists, but this machine has no such component.
    /// Example: `gpu.temperature.core` on a system with no discrete GPU.
    NotDetected { reason: String },

    /// The OS refuses access at the current privilege level.
    /// Example: SMART attributes without root or administrator.
    PermissionDenied { reason: String },

    /// Normally available, but not right now. Expected to recover on its own.
    TemporarilyUnavailable { reason: String },

    /// The provider failed while producing this metric.
    ProviderError { error: MetricError },

    /// The reference is not in this engine's catalog.
    ///
    /// Distinct from the states above because it describes the *request*, not
    /// the hardware: it means a dashboard referenced a metric this build or
    /// this machine does not know, which is a configuration problem rather
    /// than a missing sensor.
    NotRegistered { reason: String },
}

impl Availability {
    /// Whether a value is expected to accompany this state.
    pub const fn is_available(&self) -> bool {
        matches!(self, Availability::Available)
    }

    /// Whether the situation may resolve itself without user action.
    ///
    /// Drives whether the UI shows a transient hint or a persistent
    /// explanation.
    pub fn is_transient(&self) -> bool {
        match self {
            Availability::TemporarilyUnavailable { .. } => true,
            Availability::ProviderError { error } => error.recoverable,
            _ => false,
        }
    }

    /// The short machine-readable discriminant, e.g. `notDetected`.
    ///
    /// Derived from the serialised form so it can never drift from the wire
    /// contract.
    pub fn status_str(&self) -> &'static str {
        match self {
            Availability::Available => "available",
            Availability::Unsupported { .. } => "unsupported",
            Availability::NotDetected { .. } => "notDetected",
            Availability::PermissionDenied { .. } => "permissionDenied",
            Availability::TemporarilyUnavailable { .. } => "temporarilyUnavailable",
            Availability::ProviderError { .. } => "providerError",
            Availability::NotRegistered { .. } => "notRegistered",
        }
    }

    pub fn unsupported(reason: impl Into<String>) -> Self {
        Availability::Unsupported {
            reason: reason.into(),
        }
    }

    pub fn not_detected(reason: impl Into<String>) -> Self {
        Availability::NotDetected {
            reason: reason.into(),
        }
    }

    pub fn permission_denied(reason: impl Into<String>) -> Self {
        Availability::PermissionDenied {
            reason: reason.into(),
        }
    }

    pub fn temporarily_unavailable(reason: impl Into<String>) -> Self {
        Availability::TemporarilyUnavailable {
            reason: reason.into(),
        }
    }

    pub fn provider_error(error: MetricError) -> Self {
        Availability::ProviderError { error }
    }

    pub fn not_registered(reason: impl Into<String>) -> Self {
        Availability::NotRegistered {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::model::error::MetricErrorCode;

    #[test]
    fn available_serialises_without_extra_fields() {
        let json = serde_json::to_value(Availability::Available).expect("serialise");
        assert_eq!(json["status"], "available");
        assert_eq!(json.as_object().expect("object").len(), 1);
    }

    #[test]
    fn each_unavailable_state_carries_its_reason() {
        let json =
            serde_json::to_value(Availability::not_detected("no discrete GPU")).expect("serialise");
        assert_eq!(json["status"], "notDetected");
        assert_eq!(json["reason"], "no discrete GPU");

        let json =
            serde_json::to_value(Availability::permission_denied("needs root")).expect("serialise");
        assert_eq!(json["status"], "permissionDenied");
        assert_eq!(json["reason"], "needs root");
    }

    #[test]
    fn provider_error_nests_the_structured_error() {
        let availability = Availability::provider_error(MetricError::new(
            MetricErrorCode::Io,
            "hwmon read failed",
        ));
        let json = serde_json::to_value(&availability).expect("serialise");

        assert_eq!(json["status"], "providerError");
        assert_eq!(json["error"]["code"], "io");
        assert_eq!(json["error"]["recoverable"], true);
    }

    #[test]
    fn the_four_reasons_for_absence_stay_distinguishable() {
        // The whole point of the enum: these must never collapse into one
        // another.
        let states = [
            Availability::unsupported("not implemented on Windows"),
            Availability::not_detected("no sensor on this board"),
            Availability::permission_denied("needs administrator"),
            Availability::temporarily_unavailable("driver restarting"),
        ];

        let mut statuses: Vec<&str> = states.iter().map(Availability::status_str).collect();
        statuses.sort_unstable();
        statuses.dedup();

        assert_eq!(statuses.len(), 4);
        assert!(states.iter().all(|state| !state.is_available()));
    }

    #[test]
    fn transience_distinguishes_wait_from_act() {
        assert!(Availability::temporarily_unavailable("retry").is_transient());
        assert!(
            Availability::provider_error(MetricError::new(MetricErrorCode::Timeout, "slow"))
                .is_transient()
        );

        assert!(!Availability::permission_denied("needs root").is_transient());
        assert!(!Availability::not_detected("absent").is_transient());
        assert!(!Availability::provider_error(MetricError::new(
            MetricErrorCode::Unsupported,
            "never"
        ))
        .is_transient());
        assert!(!Availability::Available.is_transient());
    }

    #[test]
    fn status_str_matches_the_serialised_discriminant() {
        let states = [
            Availability::Available,
            Availability::unsupported("r"),
            Availability::not_detected("r"),
            Availability::permission_denied("r"),
            Availability::temporarily_unavailable("r"),
            Availability::provider_error(MetricError::internal("r")),
            Availability::not_registered("r"),
        ];

        for state in states {
            let json = serde_json::to_value(&state).expect("serialise");
            assert_eq!(json["status"], state.status_str());
        }
    }

    #[test]
    fn round_trips_through_json() {
        let states = [
            Availability::Available,
            Availability::not_detected("absent"),
            Availability::provider_error(MetricError::new(MetricErrorCode::Parse, "bad")),
            Availability::not_registered("unknown reference"),
        ];

        for state in states {
            let json = serde_json::to_string(&state).expect("serialise");
            assert_eq!(
                serde_json::from_str::<Availability>(&json).expect("deserialise"),
                state
            );
        }
    }
}
