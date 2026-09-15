//! The engine's self-report, surfaced to the UI.

use serde::{Deserialize, Serialize};

use crate::metrics::model::ProviderId;
use crate::metrics::METRICS_SCHEMA_VERSION;

/// Overall condition of the metrics engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineState {
    /// At least one provider is registered and the catalog is usable.
    Ready,
    /// No providers are registered. Correct and expected while PULSE has no
    /// system collectors — reported honestly rather than dressed up as
    /// "ready".
    Empty,
}

/// What one registered provider contributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    pub id: ProviderId,
    /// Metrics this provider contributed to the catalog.
    pub metric_count: usize,
    /// Of those, how many are currently sampleable.
    pub available_metric_count: usize,
}

/// A snapshot of the engine, cheap enough to poll from the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    /// Version of the metrics contract this backend speaks.
    ///
    /// The frontend compares it against its own constant and can warn instead
    /// of misinterpreting payloads after an incompatible change.
    pub schema_version: u32,
    pub state: EngineState,
    pub provider_count: usize,
    pub metric_count: usize,
    /// Metrics whose availability is `available`.
    pub available_metric_count: usize,
    /// Per-provider breakdown, in registration order.
    pub providers: Vec<ProviderSummary>,
}

impl EngineStatus {
    pub(crate) fn new(providers: Vec<ProviderSummary>) -> Self {
        let metric_count = providers.iter().map(|p| p.metric_count).sum();
        let available_metric_count = providers.iter().map(|p| p.available_metric_count).sum();

        Self {
            schema_version: METRICS_SCHEMA_VERSION,
            state: if providers.is_empty() {
                EngineState::Empty
            } else {
                EngineState::Ready
            },
            provider_count: providers.len(),
            metric_count,
            available_metric_count,
            providers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_engine_with_no_providers_reports_empty_not_ready() {
        let status = EngineStatus::new(Vec::new());

        assert_eq!(status.state, EngineState::Empty);
        assert_eq!(status.provider_count, 0);
        assert_eq!(status.metric_count, 0);
        assert_eq!(status.schema_version, METRICS_SCHEMA_VERSION);
    }

    #[test]
    fn counts_are_summed_across_providers() {
        let status = EngineStatus::new(vec![
            ProviderSummary {
                id: ProviderId::new("linux.cpu").expect("valid"),
                metric_count: 4,
                available_metric_count: 4,
            },
            ProviderSummary {
                id: ProviderId::new("linux.hwmon").expect("valid"),
                metric_count: 3,
                available_metric_count: 1,
            },
        ]);

        assert_eq!(status.state, EngineState::Ready);
        assert_eq!(status.provider_count, 2);
        assert_eq!(status.metric_count, 7);
        assert_eq!(status.available_metric_count, 5);
    }

    #[test]
    fn serialises_in_camel_case() {
        let status = EngineStatus::new(vec![ProviderSummary {
            id: ProviderId::new("mock").expect("valid"),
            metric_count: 2,
            available_metric_count: 1,
        }]);
        let json = serde_json::to_value(&status).expect("serialise");

        assert_eq!(json["schemaVersion"], METRICS_SCHEMA_VERSION);
        assert_eq!(json["state"], "ready");
        assert_eq!(json["providerCount"], 1);
        assert_eq!(json["metricCount"], 2);
        assert_eq!(json["availableMetricCount"], 1);
        assert_eq!(json["providers"][0]["id"], "mock");
        assert_eq!(json["providers"][0]["metricCount"], 2);

        assert!(json.get("schema_version").is_none());
        assert!(json.get("provider_count").is_none());
    }

    #[test]
    fn exposes_exactly_the_expected_field_set() {
        let json = serde_json::to_value(EngineStatus::new(Vec::new())).expect("serialise");
        let mut fields: Vec<&str> = json
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();

        assert_eq!(
            fields,
            [
                "availableMetricCount",
                "metricCount",
                "providerCount",
                "providers",
                "schemaVersion",
                "state",
            ]
        );
    }

    #[test]
    fn round_trips_through_json() {
        let status = EngineStatus::new(vec![ProviderSummary {
            id: ProviderId::new("mock").expect("valid"),
            metric_count: 1,
            available_metric_count: 0,
        }]);
        let json = serde_json::to_string(&status).expect("serialise");

        assert_eq!(
            serde_json::from_str::<EngineStatus>(&json).expect("deserialise"),
            status
        );
    }
}
