//! [`MetricRef`] — the full identity of one concrete measurement.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::ident::IdentError;
use super::key::MetricKey;
use super::source::SourceId;

/// A metric key bound to the component it is measured on.
///
/// This pair — and only this pair — is what a saved dashboard stores to refer
/// to a metric. Both halves are stable identifiers; neither contains anything
/// human-readable or translatable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricRef {
    pub key: MetricKey,
    pub source_id: SourceId,
}

impl MetricRef {
    pub fn new(key: MetricKey, source_id: SourceId) -> Self {
        Self { key, source_id }
    }

    /// Convenience constructor validating both halves from strings.
    pub fn parse(key: &str, source_id: &str) -> Result<Self, IdentError> {
        Ok(Self::new(MetricKey::new(key)?, SourceId::new(source_id)?))
    }
}

impl fmt::Display for MetricRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.key, self.source_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_both_halves_in_camel_case() {
        let reference =
            MetricRef::parse("gpu.temperature.core", "gpu:pci-0000-01-00-0").expect("valid");
        let json = serde_json::to_value(&reference).expect("serialise");

        assert_eq!(json["key"], "gpu.temperature.core");
        assert_eq!(json["sourceId"], "gpu:pci-0000-01-00-0");
        assert!(json.get("source_id").is_none());
    }

    #[test]
    fn round_trips_through_json() {
        let reference = MetricRef::parse("storage.temperature", "storage:nvme0n1").expect("valid");
        let json = serde_json::to_string(&reference).expect("serialise");

        assert_eq!(
            serde_json::from_str::<MetricRef>(&json).expect("deserialise"),
            reference
        );
    }

    #[test]
    fn the_same_key_on_two_sources_are_different_references() {
        let discrete = MetricRef::parse("gpu.temperature.core", "gpu:pci-0000-01-00-0").unwrap();
        let integrated = MetricRef::parse("gpu.temperature.core", "gpu:pci-0000-00-02-0").unwrap();

        assert_ne!(discrete, integrated);
        assert_eq!(discrete.key, integrated.key);
    }

    #[test]
    fn displays_in_a_readable_diagnostic_form() {
        let reference = MetricRef::parse("cpu.usage.total", "cpu:0").expect("valid");
        assert_eq!(reference.to_string(), "cpu.usage.total@cpu:0");
    }
}
