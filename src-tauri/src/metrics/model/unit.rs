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
    ///
    /// A *rate*, never a quantity. Publishing a throughput as
    /// [`MetricUnit::Bytes`] would make a 125 MB/s disk read indistinguishable
    /// from a 125 MB file, and would let history average the two together.
    BytesPerSecond,
    /// Completed I/O operations per second — IOPS.
    ///
    /// Distinct from [`MetricUnit::Count`] for the same reason
    /// `BytesPerSecond` is distinct from `Bytes`: a count of operations and a
    /// rate of operations are not the same measurement, and only one of them
    /// depends on the interval it was measured over.
    OperationsPerSecond,
    /// Network packets per second.
    ///
    /// Deliberately **not** [`MetricUnit::OperationsPerSecond`]. A disk
    /// operation and a network packet are different things measured by
    /// different subsystems, and a dashboard that let them share an axis would
    /// invite comparing 900 IOPS against 900 packets/s as if the numbers meant
    /// the same. They also differ in scale by orders of magnitude on the same
    /// machine.
    PacketsPerSecond,
    /// Bits per second.
    ///
    /// The unit of **link capacity**, never of observed traffic. A 1 Gbit/s
    /// Ethernet link carrying 12 MiB/s is one number in bits and one in bytes,
    /// and conflating them is a factor-of-eight error that looks plausible.
    /// See `docs/metrics/network.md`.
    BitsPerSecond,
    Watts,
    Volts,
    /// Revolutions per minute.
    Rpm,
    Milliseconds,
    Seconds,
    /// Decibel-milliwatts — the unit radio signal strength is reported in.
    ///
    /// A **logarithmic** scale, and always negative in practice for a received
    /// Wi-Fi signal: −40 dBm is strong, −90 dBm is barely usable. It has its
    /// own unit rather than [`MetricUnit::None`] because it must never be
    /// averaged arithmetically with another dBm figure, and because a widget
    /// needs to know not to render it on a 0-based scale.
    DecibelMilliwatts,
    /// Hours.
    ///
    /// Used for durations a device reports in whole hours — an NVMe
    /// controller's power-on time is specified that way, and converting it to
    /// seconds would invent five orders of magnitude of precision the counter
    /// does not have.
    Hours,
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
            MetricUnit::OperationsPerSecond => "IOPS",
            MetricUnit::PacketsPerSecond => "pkt/s",
            MetricUnit::BitsPerSecond => "bit/s",
            MetricUnit::Watts => "W",
            MetricUnit::Volts => "V",
            MetricUnit::Rpm => "RPM",
            MetricUnit::Milliseconds => "ms",
            MetricUnit::Seconds => "s",
            MetricUnit::DecibelMilliwatts => "dBm",
            MetricUnit::Hours => "h",
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
        MetricUnit::OperationsPerSecond,
        MetricUnit::PacketsPerSecond,
        MetricUnit::BitsPerSecond,
        MetricUnit::Watts,
        MetricUnit::Volts,
        MetricUnit::Rpm,
        MetricUnit::Milliseconds,
        MetricUnit::Seconds,
        MetricUnit::DecibelMilliwatts,
        MetricUnit::Hours,
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
            (MetricUnit::OperationsPerSecond, "\"operationsPerSecond\""),
            (MetricUnit::PacketsPerSecond, "\"packetsPerSecond\""),
            (MetricUnit::BitsPerSecond, "\"bitsPerSecond\""),
            (MetricUnit::Watts, "\"watts\""),
            (MetricUnit::Volts, "\"volts\""),
            (MetricUnit::Rpm, "\"rpm\""),
            (MetricUnit::Milliseconds, "\"milliseconds\""),
            (MetricUnit::Seconds, "\"seconds\""),
            (MetricUnit::DecibelMilliwatts, "\"decibelMilliwatts\""),
            (MetricUnit::Hours, "\"hours\""),
            (MetricUnit::Count, "\"count\""),
            (MetricUnit::None, "\"none\""),
        ];

        for (unit, json) in expected {
            assert_eq!(serde_json::to_string(&unit).expect("serialise"), json);
        }
    }

    #[test]
    fn every_variant_is_covered_by_all_and_round_trips() {
        assert_eq!(MetricUnit::ALL.len(), 18);

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
    fn a_rate_is_never_the_same_unit_as_the_quantity_it_counts() {
        // Storage throughput and IOPS only mean anything relative to the
        // interval they were measured over. Reusing `Bytes` or `Count` for
        // them would let a chart average a rate with a total.
        assert_ne!(MetricUnit::BytesPerSecond, MetricUnit::Bytes);
        assert_ne!(MetricUnit::OperationsPerSecond, MetricUnit::Count);
        assert_ne!(MetricUnit::Hours, MetricUnit::Seconds);
        assert_ne!(MetricUnit::Milliseconds, MetricUnit::Seconds);
        assert_ne!(MetricUnit::PacketsPerSecond, MetricUnit::Count);
    }

    #[test]
    fn a_disk_operation_and_a_network_packet_are_not_one_unit() {
        // Both are "things per second", and that is where the resemblance
        // ends: they come from different subsystems, differ by orders of
        // magnitude on the same machine, and must never share an axis.
        assert_ne!(
            MetricUnit::PacketsPerSecond,
            MetricUnit::OperationsPerSecond
        );
    }

    #[test]
    fn link_capacity_and_observed_traffic_are_not_one_unit() {
        // A 1 Gbit/s link carrying 12 MiB/s is one number in bits and one in
        // bytes. Conflating them is a factor-of-eight error that looks
        // entirely plausible on screen.
        assert_ne!(MetricUnit::BitsPerSecond, MetricUnit::BytesPerSecond);
    }

    #[test]
    fn a_signal_strength_is_not_a_unitless_number() {
        // dBm is logarithmic and negative. Publishing it as `None` would let a
        // widget average two readings arithmetically, or scale it from zero.
        assert_ne!(MetricUnit::DecibelMilliwatts, MetricUnit::None);
        assert!(!MetricUnit::DecibelMilliwatts.is_bounded_fraction());
    }

    #[test]
    fn symbols_match_the_canonical_units() {
        assert_eq!(MetricUnit::Celsius.symbol(), "°C");
        assert_eq!(MetricUnit::Hertz.symbol(), "Hz");
        assert_eq!(MetricUnit::BytesPerSecond.symbol(), "B/s");
        assert_eq!(MetricUnit::OperationsPerSecond.symbol(), "IOPS");
        assert_eq!(MetricUnit::PacketsPerSecond.symbol(), "pkt/s");
        assert_eq!(MetricUnit::BitsPerSecond.symbol(), "bit/s");
        assert_eq!(MetricUnit::DecibelMilliwatts.symbol(), "dBm");
        assert_eq!(MetricUnit::Hours.symbol(), "h");
        assert_eq!(MetricUnit::None.symbol(), "");
    }
}
