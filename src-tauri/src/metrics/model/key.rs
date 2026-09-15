//! [`MetricKey`] — the stable, semantic name of *what* is measured.
//!
//! A key describes the nature of a measurement and nothing else. It says
//! "GPU core temperature", never "the RTX 4070's core temperature" — which
//! component is measured is carried separately by [`SourceId`].
//!
//! [`SourceId`]: super::source::SourceId

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::ident::{validate_len, validate_segment, IdentError, LeadingDigit};

const KIND: &str = "MetricKey";
const MAX_LEN: usize = 128;
const MIN_SEGMENTS: usize = 2;
const MAX_SEGMENTS: usize = 6;
const SEPARATOR: char = '.';

/// A dotted, lowercase, semantically stable metric name.
///
/// Convention: `<domain>.<aspect>[.<qualifier>…]`, for example
/// `cpu.usage.total`, `memory.used`, `gpu.temperature.hotspot`,
/// `network.download.rate`.
///
/// Keys are part of PULSE's public contract: saved dashboards reference them,
/// so renaming one is a breaking change. See `docs/metrics/identifiers.md`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MetricKey(String);

impl MetricKey {
    /// Validates and creates a key.
    ///
    /// Rules: 2 to 6 dot-separated segments; each segment starts with a
    /// lowercase letter and continues with lowercase letters, digits or `_`;
    /// at most 128 characters overall.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentError> {
        let value = value.into();
        validate_len(KIND, &value, MAX_LEN)?;

        let segments: Vec<&str> = value.split(SEPARATOR).collect();
        if segments.len() < MIN_SEGMENTS || segments.len() > MAX_SEGMENTS {
            return Err(IdentError::SegmentCount {
                kind: KIND,
                min: MIN_SEGMENTS,
                max: MAX_SEGMENTS,
                actual: segments.len(),
            });
        }

        for segment in &segments {
            validate_segment(KIND, segment, &['_'], LeadingDigit::Forbidden)?;
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first segment, e.g. `cpu` in `cpu.usage.total`.
    ///
    /// Useful for grouping a catalog without parsing the whole key.
    pub fn domain(&self) -> &str {
        self.0.split(SEPARATOR).next().unwrap_or(&self.0)
    }
}

impl fmt::Display for MetricKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for MetricKey {
    type Err = IdentError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for MetricKey {
    type Error = IdentError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl Serialize for MetricKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// Deserialises from a plain JSON string, validating on the way in.
///
/// An invalid key sent by the frontend produces a clean deserialisation error
/// rather than an unresolvable reference deeper in the engine.
impl<'de> Deserialize<'de> for MetricKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        MetricKey::new(raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_documented_key_conventions() {
        for key in [
            "cpu.usage.total",
            "cpu.frequency.current",
            "cpu.temperature.package",
            "memory.used",
            "memory.available",
            "gpu.usage.core",
            "gpu.temperature.hotspot",
            "gpu.memory.used",
            "storage.temperature",
            "storage.read.rate",
            "network.download.rate",
            "network.upload.rate",
            "cpu.usage.per_core",
        ] {
            assert!(MetricKey::new(key).is_ok(), "expected '{key}' to be valid");
        }
    }

    #[test]
    fn rejects_invalid_keys() {
        // Single segment: a key must name a domain and an aspect.
        assert!(MetricKey::new("cpu").is_err());
        // Uppercase, spaces, separators from other conventions.
        assert!(MetricKey::new("CPU.usage").is_err());
        assert!(MetricKey::new("cpu.Usage").is_err());
        assert!(MetricKey::new("cpu usage").is_err());
        assert!(MetricKey::new("cpu-usage").is_err());
        assert!(MetricKey::new("cpu/usage").is_err());
        // Empty segments.
        assert!(MetricKey::new("cpu..usage").is_err());
        assert!(MetricKey::new(".cpu.usage").is_err());
        assert!(MetricKey::new("cpu.usage.").is_err());
        assert!(MetricKey::new("").is_err());
        // Segment starting with a digit.
        assert!(MetricKey::new("cpu.0usage").is_err());
        // Too many segments.
        assert!(MetricKey::new("a.b.c.d.e.f.g").is_err());
    }

    #[test]
    fn rejects_a_key_over_the_length_limit() {
        let long = format!("cpu.{}", "a".repeat(200));
        assert!(matches!(
            MetricKey::new(long),
            Err(IdentError::TooLong { .. })
        ));
    }

    #[test]
    fn exposes_its_domain() {
        let key = MetricKey::new("gpu.temperature.hotspot").expect("valid");
        assert_eq!(key.domain(), "gpu");
    }

    #[test]
    fn serialises_as_a_plain_string() {
        let key = MetricKey::new("cpu.usage.total").expect("valid");
        assert_eq!(
            serde_json::to_string(&key).expect("serialise"),
            "\"cpu.usage.total\""
        );
    }

    #[test]
    fn deserialisation_validates_instead_of_trusting_the_caller() {
        let ok: MetricKey = serde_json::from_str("\"cpu.usage.total\"").expect("valid key");
        assert_eq!(ok.as_str(), "cpu.usage.total");

        // A malformed key from the frontend is a clean error, never a panic.
        let err = serde_json::from_str::<MetricKey>("\"CPU Usage\"");
        assert!(err.is_err());
    }

    #[test]
    fn ordering_is_lexicographic_for_deterministic_catalogues() {
        let mut keys = [
            MetricKey::new("memory.used").expect("valid"),
            MetricKey::new("cpu.usage.total").expect("valid"),
            MetricKey::new("gpu.usage.core").expect("valid"),
        ];
        keys.sort();

        let ordered: Vec<&str> = keys.iter().map(MetricKey::as_str).collect();
        assert_eq!(
            ordered,
            ["cpu.usage.total", "gpu.usage.core", "memory.used"]
        );
    }
}
