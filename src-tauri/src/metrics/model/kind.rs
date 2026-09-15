//! How a metric's values behave over time.

use serde::{Deserialize, Serialize};

/// The temporal nature of a metric.
///
/// This tells later phases how a value may legitimately be aggregated: a gauge
/// can be averaged, a counter must be differenced, a state can only be counted.
/// Getting this wrong produces charts that look plausible and are wrong, which
/// is why it is declared up front rather than inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetricKind {
    /// An instantaneous reading that rises and falls: CPU usage, temperature,
    /// fan speed, free memory.
    Gauge,
    /// A monotonically increasing total: bytes received since boot, total
    /// energy consumed. Rates are derived from differences between samples.
    Counter,
    /// A discrete condition: link up/down, battery charging, throttling active.
    State,
}

impl MetricKind {
    /// Whether values may be averaged directly.
    ///
    /// False for counters (which must be differenced first) and for states.
    pub const fn is_directly_averageable(self) -> bool {
        matches!(self, MetricKind::Gauge)
    }

    pub const ALL: &'static [MetricKind] =
        &[MetricKind::Gauge, MetricKind::Counter, MetricKind::State];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_in_camel_case() {
        assert_eq!(
            serde_json::to_string(&MetricKind::Gauge).expect("serialise"),
            "\"gauge\""
        );
        assert_eq!(
            serde_json::to_string(&MetricKind::Counter).expect("serialise"),
            "\"counter\""
        );
        assert_eq!(
            serde_json::to_string(&MetricKind::State).expect("serialise"),
            "\"state\""
        );
    }

    #[test]
    fn only_gauges_may_be_averaged_directly() {
        assert!(MetricKind::Gauge.is_directly_averageable());
        assert!(!MetricKind::Counter.is_directly_averageable());
        assert!(!MetricKind::State.is_directly_averageable());
    }

    #[test]
    fn round_trips_through_json() {
        for kind in MetricKind::ALL {
            let json = serde_json::to_string(kind).expect("serialise");
            assert_eq!(
                &serde_json::from_str::<MetricKind>(&json).expect("deserialise"),
                kind
            );
        }
    }
}
