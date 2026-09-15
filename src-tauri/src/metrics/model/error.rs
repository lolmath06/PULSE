//! Structured metric errors.
//!
//! The frontend must be able to react to *what kind* of thing went wrong
//! without parsing an English sentence. The message is for humans; the code is
//! for code.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Machine-readable error classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetricErrorCode {
    /// The metric reference is not present in this engine's catalog.
    UnknownMetric,
    /// Two providers claimed the same metric reference.
    DuplicateMetric,
    /// The platform or hardware has no such capability at all.
    Unsupported,
    /// The capability exists but the sensor or device was not found.
    NotDetected,
    /// Blocked by OS permissions — needs root, administrator, or a capability.
    PermissionDenied,
    /// Reading a file, device or API failed.
    Io,
    /// The data source was read but could not be interpreted.
    Parse,
    /// The source did not answer in time.
    Timeout,
    /// The provider itself is not usable right now (driver gone, service down).
    ProviderUnavailable,
    /// The provider returned no sample for a metric it declared.
    MissingSample,
    /// A bug in PULSE rather than in the environment.
    Internal,
}

impl MetricErrorCode {
    /// Whether an error of this class is worth retrying on the next sample.
    ///
    /// The default used by [`MetricError::new`]; callers may override it when
    /// they know better.
    pub const fn is_recoverable_by_default(self) -> bool {
        matches!(
            self,
            MetricErrorCode::Io
                | MetricErrorCode::Timeout
                | MetricErrorCode::ProviderUnavailable
                | MetricErrorCode::MissingSample
                | MetricErrorCode::NotDetected
        )
    }
}

/// An error attached to a metric or to a provider operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricError {
    pub code: MetricErrorCode,
    /// Human-readable detail. Never parsed by the frontend.
    pub message: String,
    /// Whether retrying later might succeed.
    pub recoverable: bool,
}

impl MetricError {
    /// Creates an error, taking the default recoverability of the code.
    pub fn new(code: MetricErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            recoverable: code.is_recoverable_by_default(),
        }
    }

    /// Overrides the default recoverability.
    pub fn with_recoverable(mut self, recoverable: bool) -> Self {
        self.recoverable = recoverable;
        self
    }

    pub fn unknown_metric(message: impl Into<String>) -> Self {
        Self::new(MetricErrorCode::UnknownMetric, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(MetricErrorCode::Internal, message)
    }
}

impl fmt::Display for MetricError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

impl std::error::Error for MetricError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_in_camel_case_with_a_machine_readable_code() {
        let error = MetricError::new(MetricErrorCode::PermissionDenied, "needs root");
        let json = serde_json::to_value(&error).expect("serialise");

        assert_eq!(json["code"], "permissionDenied");
        assert_eq!(json["message"], "needs root");
        assert_eq!(json["recoverable"], false);
    }

    #[test]
    fn transient_classes_default_to_recoverable() {
        assert!(MetricError::new(MetricErrorCode::Io, "read failed").recoverable);
        assert!(MetricError::new(MetricErrorCode::Timeout, "slow").recoverable);
        assert!(MetricError::new(MetricErrorCode::NotDetected, "no sensor").recoverable);
    }

    #[test]
    fn permanent_classes_default_to_unrecoverable() {
        assert!(!MetricError::new(MetricErrorCode::Unsupported, "no such API").recoverable);
        assert!(!MetricError::new(MetricErrorCode::PermissionDenied, "denied").recoverable);
        assert!(!MetricError::unknown_metric("not in catalog").recoverable);
        assert!(!MetricError::internal("bug").recoverable);
    }

    #[test]
    fn recoverability_can_be_overridden_by_the_caller() {
        let error =
            MetricError::new(MetricErrorCode::Io, "device vanished").with_recoverable(false);
        assert!(!error.recoverable);
    }

    #[test]
    fn round_trips_through_json() {
        let error = MetricError::new(MetricErrorCode::Parse, "unexpected field");
        let json = serde_json::to_string(&error).expect("serialise");
        assert_eq!(
            serde_json::from_str::<MetricError>(&json).expect("deserialise"),
            error
        );
    }

    #[test]
    fn every_code_serialises_distinctly() {
        let codes = [
            MetricErrorCode::UnknownMetric,
            MetricErrorCode::DuplicateMetric,
            MetricErrorCode::Unsupported,
            MetricErrorCode::NotDetected,
            MetricErrorCode::PermissionDenied,
            MetricErrorCode::Io,
            MetricErrorCode::Parse,
            MetricErrorCode::Timeout,
            MetricErrorCode::ProviderUnavailable,
            MetricErrorCode::MissingSample,
            MetricErrorCode::Internal,
        ];

        let mut encoded: Vec<String> = codes
            .iter()
            .map(|code| serde_json::to_string(code).expect("serialise"))
            .collect();
        encoded.sort();
        encoded.dedup();

        assert_eq!(encoded.len(), codes.len());
    }
}
