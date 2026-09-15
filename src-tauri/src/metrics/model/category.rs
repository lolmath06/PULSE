//! Metric categories — how the catalog is grouped for the user.

use serde::{Deserialize, Serialize};

/// Broad grouping of a metric, used for navigation and widget pickers.
///
/// Kept deliberately small. A category is a *user-facing grouping*, not a
/// taxonomy of hardware: resist adding one per sensor chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetricCategory {
    System,
    Cpu,
    Gpu,
    Memory,
    Storage,
    Network,
    /// Temperatures and other environmental sensors.
    Sensors,
    Fan,
    Power,
    Battery,
    Process,
    Other,
}

impl MetricCategory {
    /// All categories, in the order PULSE presents them.
    pub const ALL: &'static [MetricCategory] = &[
        MetricCategory::System,
        MetricCategory::Cpu,
        MetricCategory::Gpu,
        MetricCategory::Memory,
        MetricCategory::Storage,
        MetricCategory::Network,
        MetricCategory::Sensors,
        MetricCategory::Fan,
        MetricCategory::Power,
        MetricCategory::Battery,
        MetricCategory::Process,
        MetricCategory::Other,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_in_camel_case() {
        assert_eq!(
            serde_json::to_string(&MetricCategory::Gpu).expect("serialise"),
            "\"gpu\""
        );
        assert_eq!(
            serde_json::to_string(&MetricCategory::Sensors).expect("serialise"),
            "\"sensors\""
        );
    }

    #[test]
    fn every_variant_is_listed_in_all() {
        // Guards against adding a variant and forgetting the presentation
        // order, which the frontend mirrors.
        assert_eq!(MetricCategory::ALL.len(), 12);

        let mut sorted = MetricCategory::ALL.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), MetricCategory::ALL.len());
    }

    #[test]
    fn round_trips_through_json() {
        for category in MetricCategory::ALL {
            let json = serde_json::to_string(category).expect("serialise");
            let back: MetricCategory = serde_json::from_str(&json).expect("deserialise");
            assert_eq!(&back, category);
        }
    }
}
