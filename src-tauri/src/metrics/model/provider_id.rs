//! [`ProviderId`] — the stable identity of a metric provider.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::ident::{validate_len, validate_segment, IdentError, LeadingDigit};

const KIND: &str = "ProviderId";
const MAX_LEN: usize = 64;
const SEPARATOR: char = '.';

/// Identifies which provider owns a metric, e.g. `linux.cpu`, `windows.pdh`,
/// `nvidia.nvml`.
///
/// Surfaced in the engine status and in error payloads so a failure can be
/// attributed to a specific provider rather than to "the metrics system".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentError> {
        let value = value.into();
        validate_len(KIND, &value, MAX_LEN)?;

        for segment in value.split(SEPARATOR) {
            validate_segment(KIND, segment, &['_', '-'], LeadingDigit::Forbidden)?;
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ProviderId {
    type Err = IdentError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl Serialize for ProviderId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProviderId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        ProviderId::new(raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plausible_provider_names() {
        for id in [
            "mock",
            "linux.cpu",
            "windows.pdh",
            "nvidia.nvml",
            "linux.hwmon",
        ] {
            assert!(ProviderId::new(id).is_ok(), "expected '{id}' to be valid");
        }
    }

    #[test]
    fn rejects_malformed_names() {
        assert!(ProviderId::new("").is_err());
        assert!(ProviderId::new("Linux.CPU").is_err());
        assert!(ProviderId::new("linux..cpu").is_err());
        assert!(ProviderId::new("linux cpu").is_err());
        assert!(ProviderId::new("1linux").is_err());
    }

    #[test]
    fn serialises_as_a_plain_string() {
        let id = ProviderId::new("linux.cpu").expect("valid");
        assert_eq!(
            serde_json::to_string(&id).expect("serialise"),
            "\"linux.cpu\""
        );
    }
}
