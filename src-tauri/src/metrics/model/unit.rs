//! Canonical units.
//!
//! **The backend always reports values in the canonical unit listed here.**
//! Hertz, not gigahertz. Bytes, not gibibytes. Celsius, not Fahrenheit.
//!
//! Presentation units are a frontend concern: the UI may render `3.8 GHz`,
//! `15.4 GB` or `°F`, but the number crossing the IPC boundary is always
//! canonical. This keeps stored history, alert thresholds and dashboard
//! configuration comparable across machines and across PULSE versions.

use serde::{Deserialize, Serialize};

/// The canonical unit of a metric's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetricUnit {
    /// 0–100. Distinct from [`MetricUnit::Ratio`] on purpose: mixing the two
    /// silently is a factor-of-100 bug waiting to happen.
    Percent,
    /// 0–1.
    Ratio,
    /// Degrees Celsius.
    Celsius,
    /// Hertz (not kHz, MHz or GHz).
    Hertz,
    /// Bytes (not KB, MB or GB).
    Bytes,
    /// Bytes per second.
    BytesPerSecond,
    Watts,
    Volts,
    /// Revolutions per minute.
    Rpm,
    Milliseconds,
    Seconds,
    /// A dimensionless count of things.
    Count,
    /// No unit — for boolean and text metrics.
    None,
}

impl MetricUnit {
    /// The symbol conventionally appended to a value of this unit.
    ///
    /// Empty for units that are rendered without a suffix. The frontend is
    /// free to use its own localised formatting instead; this exists so the
    /// backend can produce readable diagnostics.
    pub const fn symbol(self) -> &'static str {
        match self {
            MetricUnit::Percent => "%",
            MetricUnit::Ratio => "",
            MetricUnit::Celsius => "°C",
            MetricUnit::Hertz => "Hz",
            MetricUnit::Bytes => "B",
            MetricUnit::BytesPerSecond => "B/s",
            MetricUnit::Watts => "W",
            MetricUnit::Volts => "V",
            MetricUnit::Rpm => "RPM",
            MetricUnit::Milliseconds => "ms",
            MetricUnit::Seconds => "s",
            MetricUnit::Count => "",
            MetricUnit::None => "",
        }
    }

    /// Whether values of this unit are naturally bounded to 0–100 (`Percent`)
    /// or 0–1 (`Ratio`). Used by future gauge widgets to pick a scale.
    pub const fn is_bounded_fraction(self) -> bool {
        matches!(self, MetricUnit::Percent | MetricUnit::Ratio)
    }

    pub const ALL: &'static [MetricUnit] = &[
        MetricUnit::Percent,
        MetricUnit::Ratio,
        MetricUnit::Celsius,
        MetricUnit::Hertz,
        MetricUnit::Bytes,
        MetricUnit::BytesPerSecond,
        MetricUnit::Watts,
        MetricUnit::Volts,
        MetricUnit::Rpm,
        MetricUnit::Milliseconds,
        MetricUnit::Seconds,
        MetricUnit::Count,
        MetricUnit::None,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_in_camel_case_matching_the_typescript_union() {
        let expected = [
            (MetricUnit::Percent, "\"percent\""),
            (MetricUnit::Ratio, "\"ratio\""),
            (MetricUnit::Celsius, "\"celsius\""),
            (MetricUnit::Hertz, "\"hertz\""),
            (MetricUnit::Bytes, "\"bytes\""),
            (MetricUnit::BytesPerSecond, "\"bytesPerSecond\""),
            (MetricUnit::Watts, "\"watts\""),
            (MetricUnit::Volts, "\"volts\""),
            (MetricUnit::Rpm, "\"rpm\""),
            (MetricUnit::Milliseconds, "\"milliseconds\""),
            (MetricUnit::Seconds, "\"seconds\""),
            (MetricUnit::Count, "\"count\""),
            (MetricUnit::None, "\"none\""),
        ];

        for (unit, json) in expected {
            assert_eq!(serde_json::to_string(&unit).expect("serialise"), json);
        }
    }

    #[test]
    fn every_variant_is_covered_by_all_and_round_trips() {
        assert_eq!(MetricUnit::ALL.len(), 13);

        for unit in MetricUnit::ALL {
            let json = serde_json::to_string(unit).expect("serialise");
            let back: MetricUnit = serde_json::from_str(&json).expect("deserialise");
            assert_eq!(&back, unit);
        }
    }

    #[test]
    fn percent_and_ratio_are_distinct_units() {
        // Conflating them is a factor-of-100 bug; the contract keeps them apart.
        assert_ne!(MetricUnit::Percent, MetricUnit::Ratio);
        assert!(MetricUnit::Percent.is_bounded_fraction());
        assert!(MetricUnit::Ratio.is_bounded_fraction());
        assert!(!MetricUnit::Bytes.is_bounded_fraction());
    }

    #[test]
    fn symbols_match_the_canonical_units() {
        assert_eq!(MetricUnit::Celsius.symbol(), "°C");
        assert_eq!(MetricUnit::Hertz.symbol(), "Hz");
        assert_eq!(MetricUnit::BytesPerSecond.symbol(), "B/s");
        assert_eq!(MetricUnit::None.symbol(), "");
    }
}
