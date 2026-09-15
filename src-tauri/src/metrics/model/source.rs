//! [`SourceId`] — the stable identity of *which component* is measured.
//!
//! The same [`MetricKey`] can exist many times on one machine:
//! `gpu.temperature.core` for a discrete NVIDIA GPU and for an Intel iGPU,
//! `storage.temperature` for every NVMe drive installed. The key says what is
//! measured; the source says on what.
//!
//! [`MetricKey`]: super::key::MetricKey

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::ident::{validate_len, validate_segment, IdentError, LeadingDigit};

const KIND: &str = "SourceId";
const MAX_LEN: usize = 128;
const SEPARATOR: char = ':';

/// Canonical source kinds.
///
/// Kept as documented string prefixes rather than a closed enum so that a new
/// hardware class does not require changing the contract. The *shape* is
/// validated; the vocabulary is a convention.
pub const CANONICAL_SOURCE_KINDS: &[&str] = &[
    "system", "cpu", "gpu", "memory", "storage", "network", "battery", "fan", "power",
];

/// A stable `kind:instance` identifier for a measured component.
///
/// Examples: `system:host`, `cpu:0`, `gpu:pci-0000-01-00-0`, `storage:nvme0n1`,
/// `network:enp5s0`.
///
/// **The instance part must be derived from something stable across reboots**
/// — a PCI address, a device serial, a kernel device name — and never from the
/// human-readable product name. A user who renames a device, or who has two
/// identical drives, must not lose their dashboard configuration. See
/// `docs/metrics/identifiers.md`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(String);

impl SourceId {
    /// Validates and creates a source identifier from a full `kind:instance`
    /// string.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentError> {
        let value = value.into();
        validate_len(KIND, &value, MAX_LEN)?;

        let Some((kind, instance)) = value.split_once(SEPARATOR) else {
            return Err(IdentError::MissingSeparator {
                kind: KIND,
                separator: SEPARATOR,
            });
        };

        // The kind is a plain lowercase word. The instance may carry the
        // punctuation found in PCI addresses and kernel device names, and may
        // be purely numeric — `cpu:0` is a perfectly ordinary source.
        validate_segment(KIND, kind, &[], LeadingDigit::Forbidden)?;
        validate_segment(KIND, instance, &['-', '_', '.'], LeadingDigit::Allowed)?;

        Ok(Self(value))
    }

    /// Builds a source identifier from its two parts.
    pub fn from_parts(kind: &str, instance: &str) -> Result<Self, IdentError> {
        Self::new(format!("{kind}{SEPARATOR}{instance}"))
    }

    /// The component class, e.g. `gpu` in `gpu:pci-0000-01-00-0`.
    pub fn kind(&self) -> &str {
        self.0.split_once(SEPARATOR).map(|(k, _)| k).unwrap_or("")
    }

    /// The stable instance identifier, e.g. `pci-0000-01-00-0`.
    pub fn instance(&self) -> &str {
        self.0.split_once(SEPARATOR).map(|(_, i)| i).unwrap_or("")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the kind is one of the documented canonical prefixes.
    ///
    /// Advisory only — an unknown kind is valid, not an error. Used by tests
    /// and by future tooling to spot accidental divergence.
    pub fn has_canonical_kind(&self) -> bool {
        CANONICAL_SOURCE_KINDS.contains(&self.kind())
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for SourceId {
    type Err = IdentError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for SourceId {
    type Error = IdentError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl Serialize for SourceId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SourceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        SourceId::new(raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_realistic_hardware_identifiers() {
        for id in [
            "system:host",
            "cpu:0",
            "cpu:package0",
            "gpu:pci-0000-01-00-0",
            "storage:nvme0n1",
            "storage:ata-samsung_ssd_870",
            "network:enp5s0",
            "network:wlp3s0",
            "battery:bat0",
            "fan:hwmon2-fan1",
            // Purely numeric instances are common: CPU package index, disk
            // number, monitor index.
            "cpu:0",
            "storage:2",
        ] {
            assert!(SourceId::new(id).is_ok(), "expected '{id}' to be valid");
        }
    }

    #[test]
    fn rejects_malformed_identifiers() {
        // No separator at all.
        assert!(matches!(
            SourceId::new("gpu0"),
            Err(IdentError::MissingSeparator { .. })
        ));
        // Empty kind or instance.
        assert!(SourceId::new(":nvme0").is_err());
        assert!(SourceId::new("gpu:").is_err());
        // Uppercase and whitespace, which would make identity casing-dependent.
        assert!(SourceId::new("GPU:nvme0").is_err());
        assert!(SourceId::new("gpu:NVME0").is_err());
        assert!(SourceId::new("gpu:my card").is_err());
        // A digit-led kind (the instance, by contrast, may be numeric).
        assert!(SourceId::new("0gpu:x").is_err());
        // A symbol-led instance.
        assert!(SourceId::new("gpu:-card0").is_err());
    }

    #[test]
    fn splits_into_kind_and_instance() {
        let id = SourceId::new("gpu:pci-0000-01-00-0").expect("valid");
        assert_eq!(id.kind(), "gpu");
        assert_eq!(id.instance(), "pci-0000-01-00-0");
        assert!(id.has_canonical_kind());
    }

    #[test]
    fn from_parts_matches_the_parsed_form() {
        let built = SourceId::from_parts("storage", "nvme0n1").expect("valid");
        assert_eq!(built, SourceId::new("storage:nvme0n1").expect("valid"));
    }

    #[test]
    fn an_unknown_kind_is_allowed_but_flagged_as_non_canonical() {
        let id = SourceId::new("thermalzone:0").expect("valid shape");
        assert!(!id.has_canonical_kind());
    }

    #[test]
    fn round_trips_through_json_as_a_plain_string() {
        let id = SourceId::new("storage:nvme0n1").expect("valid");
        let json = serde_json::to_string(&id).expect("serialise");
        assert_eq!(json, "\"storage:nvme0n1\"");
        assert_eq!(
            serde_json::from_str::<SourceId>(&json).expect("deserialise"),
            id
        );
    }

    #[test]
    fn deserialisation_rejects_a_malformed_identifier() {
        assert!(serde_json::from_str::<SourceId>("\"not-a-source\"").is_err());
    }
}
