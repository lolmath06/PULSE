//! Metric values and their types.

use serde::{Deserialize, Serialize};

/// The type a metric's values will always have.
///
/// Declared once in the [`MetricDefinition`], so a widget can decide how to
/// render a metric before any sample arrives.
///
/// [`MetricDefinition`]: super::definition::MetricDefinition
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetricValueType {
    Number,
    Boolean,
    Text,
}

/// A single measured value.
///
/// Serialised as a discriminated union — `{"type":"number","value":42.5}` —
/// which maps directly onto a TypeScript tagged union and needs no runtime
/// type sniffing on the frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum MetricValue {
    Number(f64),
    Boolean(bool),
    Text(String),
}

impl MetricValue {
    /// Creates a numeric value, rejecting non-finite floats.
    ///
    /// This matters more than it looks: `serde_json` serialises `NaN` and
    /// infinity as JSON `null`, which would arrive on the frontend as a
    /// `number` value that is not a number and break the contract silently. A
    /// non-finite reading is a failed reading, so callers get `None` and are
    /// pushed towards reporting it as unavailable instead.
    pub fn number(value: f64) -> Option<Self> {
        value.is_finite().then_some(MetricValue::Number(value))
    }

    pub fn boolean(value: bool) -> Self {
        MetricValue::Boolean(value)
    }

    pub fn text(value: impl Into<String>) -> Self {
        MetricValue::Text(value.into())
    }

    /// The declared type this value corresponds to.
    pub const fn value_type(&self) -> MetricValueType {
        match self {
            MetricValue::Number(_) => MetricValueType::Number,
            MetricValue::Boolean(_) => MetricValueType::Boolean,
            MetricValue::Text(_) => MetricValueType::Text,
        }
    }

    /// The numeric payload, if this is a number.
    pub const fn as_number(&self) -> Option<f64> {
        match self {
            MetricValue::Number(value) => Some(*value),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_as_a_tagged_union() {
        let json = serde_json::to_value(MetricValue::Number(42.5)).expect("serialise");
        assert_eq!(json["type"], "number");
        assert_eq!(json["value"], 42.5);

        let json = serde_json::to_value(MetricValue::Boolean(true)).expect("serialise");
        assert_eq!(json["type"], "boolean");
        assert_eq!(json["value"], true);

        let json = serde_json::to_value(MetricValue::text("connected")).expect("serialise");
        assert_eq!(json["type"], "text");
        assert_eq!(json["value"], "connected");
    }

    #[test]
    fn round_trips_through_json() {
        for value in [
            MetricValue::Number(-17.25),
            MetricValue::Boolean(false),
            MetricValue::text("Fedora"),
        ] {
            let json = serde_json::to_string(&value).expect("serialise");
            let back: MetricValue = serde_json::from_str(&json).expect("deserialise");
            assert_eq!(back, value);
        }
    }

    #[test]
    fn rejects_non_finite_numbers() {
        // serde_json would turn these into JSON null and quietly violate the
        // contract, so they are refused at construction.
        assert!(MetricValue::number(f64::NAN).is_none());
        assert!(MetricValue::number(f64::INFINITY).is_none());
        assert!(MetricValue::number(f64::NEG_INFINITY).is_none());

        assert_eq!(
            MetricValue::number(0.0),
            Some(MetricValue::Number(0.0)),
            "zero is a perfectly good reading"
        );
        assert!(MetricValue::number(-40.0).is_some());
    }

    #[test]
    fn reports_its_own_type() {
        assert_eq!(
            MetricValue::Number(1.0).value_type(),
            MetricValueType::Number
        );
        assert_eq!(
            MetricValue::Boolean(true).value_type(),
            MetricValueType::Boolean
        );
        assert_eq!(MetricValue::text("x").value_type(), MetricValueType::Text);
    }

    #[test]
    fn exposes_the_numeric_payload_only_for_numbers() {
        assert_eq!(MetricValue::Number(3.5).as_number(), Some(3.5));
        assert_eq!(MetricValue::Boolean(true).as_number(), None);
    }

    #[test]
    fn value_types_serialise_in_camel_case() {
        assert_eq!(
            serde_json::to_string(&MetricValueType::Number).expect("serialise"),
            "\"number\""
        );
        assert_eq!(
            serde_json::to_string(&MetricValueType::Boolean).expect("serialise"),
            "\"boolean\""
        );
    }
}
