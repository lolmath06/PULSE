//! [`MetricDefinition`] — everything PULSE knows about a metric before any
//! value is read.

use serde::{Deserialize, Serialize};

use super::availability::Availability;
use super::category::MetricCategory;
use super::kind::MetricKind;
use super::provider_id::ProviderId;
use super::reference::MetricRef;
use super::unit::MetricUnit;
use super::value::MetricValueType;

/// The metadata describing one concrete metric on this machine.
///
/// Identity (`metric`) is separated from presentation (`source_label`,
/// `display_name`, `description`) on purpose: the former is stable and stored
/// in dashboards, the latter is free to change with hardware detection
/// improvements or translation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricDefinition {
    /// Stable identity: metric key plus source. The only part a saved
    /// dashboard may store.
    pub metric: MetricRef,

    /// Human-readable name of the measured component, e.g.
    /// `NVIDIA GeForce RTX 4070 Laptop GPU`. **Presentation only** — never an
    /// identifier.
    pub source_label: String,

    /// Human-readable name of the measurement, e.g. `Core temperature`.
    pub display_name: String,

    /// Longer explanation shown in tooltips and the widget picker.
    pub description: String,

    pub category: MetricCategory,

    /// Canonical unit. See [`MetricUnit`] — the backend never converts.
    pub unit: MetricUnit,

    /// The type every value of this metric will have.
    pub value_type: MetricValueType,

    /// How values behave over time (gauge / counter / state).
    pub kind: MetricKind,

    /// Whether this metric can currently be sampled, and if not, why.
    ///
    /// Determined at catalog build time. A metric that is `NotDetected` still
    /// appears in the catalog so the UI can explain its absence rather than
    /// silently omitting it.
    pub availability: Availability,

    /// Which provider owns this metric.
    pub provider_id: ProviderId,
}

/// Builder for [`MetricDefinition`].
///
/// Providers declare many metrics; a builder keeps those declarations readable
/// and makes the required fields explicit without a ten-argument constructor.
#[derive(Debug, Clone)]
pub struct MetricDefinitionBuilder {
    definition: MetricDefinition,
}

impl MetricDefinitionBuilder {
    /// Starts a definition with the mandatory identity and classification.
    pub fn new(
        metric: MetricRef,
        provider_id: ProviderId,
        category: MetricCategory,
        unit: MetricUnit,
        kind: MetricKind,
    ) -> Self {
        let display_name = metric.key.as_str().to_string();
        let source_label = metric.source_id.as_str().to_string();

        Self {
            definition: MetricDefinition {
                metric,
                source_label,
                display_name,
                description: String::new(),
                category,
                unit,
                value_type: MetricValueType::Number,
                kind,
                availability: Availability::Available,
                provider_id,
            },
        }
    }

    pub fn source_label(mut self, label: impl Into<String>) -> Self {
        self.definition.source_label = label.into();
        self
    }

    pub fn display_name(mut self, name: impl Into<String>) -> Self {
        self.definition.display_name = name.into();
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.definition.description = description.into();
        self
    }

    pub fn value_type(mut self, value_type: MetricValueType) -> Self {
        self.definition.value_type = value_type;
        self
    }

    pub fn availability(mut self, availability: Availability) -> Self {
        self.definition.availability = availability;
        self
    }

    pub fn build(self) -> MetricDefinition {
        self.definition
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_definition() -> MetricDefinition {
        MetricDefinitionBuilder::new(
            MetricRef::parse("gpu.temperature.core", "gpu:pci-0000-01-00-0").expect("valid"),
            ProviderId::new("nvidia.nvml").expect("valid"),
            MetricCategory::Gpu,
            MetricUnit::Celsius,
            MetricKind::Gauge,
        )
        .source_label("NVIDIA GeForce RTX 4070 Laptop GPU")
        .display_name("Core temperature")
        .description("Temperature of the GPU core die.")
        .build()
    }

    #[test]
    fn serialises_every_field_in_camel_case() {
        let json = serde_json::to_value(sample_definition()).expect("serialise");

        assert_eq!(json["metric"]["key"], "gpu.temperature.core");
        assert_eq!(json["metric"]["sourceId"], "gpu:pci-0000-01-00-0");
        assert_eq!(json["sourceLabel"], "NVIDIA GeForce RTX 4070 Laptop GPU");
        assert_eq!(json["displayName"], "Core temperature");
        assert_eq!(json["description"], "Temperature of the GPU core die.");
        assert_eq!(json["category"], "gpu");
        assert_eq!(json["unit"], "celsius");
        assert_eq!(json["valueType"], "number");
        assert_eq!(json["kind"], "gauge");
        assert_eq!(json["availability"]["status"], "available");
        assert_eq!(json["providerId"], "nvidia.nvml");

        // Snake_case must never appear on the wire.
        for snake in [
            "source_label",
            "display_name",
            "value_type",
            "provider_id",
            "source_id",
        ] {
            assert!(
                json.get(snake).is_none(),
                "unexpected snake_case field '{snake}'"
            );
        }
    }

    #[test]
    fn exposes_exactly_the_expected_field_set() {
        // Guards against a field being added to Rust and forgotten in
        // src/types/metrics.ts.
        let json = serde_json::to_value(sample_definition()).expect("serialise");
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
                "availability",
                "category",
                "description",
                "displayName",
                "kind",
                "metric",
                "providerId",
                "sourceLabel",
                "unit",
                "valueType",
            ]
        );
    }

    #[test]
    fn round_trips_through_json() {
        let definition = sample_definition();
        let json = serde_json::to_string(&definition).expect("serialise");

        assert_eq!(
            serde_json::from_str::<MetricDefinition>(&json).expect("deserialise"),
            definition
        );
    }

    #[test]
    fn the_builder_defaults_to_an_available_numeric_gauge() {
        let definition = MetricDefinitionBuilder::new(
            MetricRef::parse("cpu.usage.total", "cpu:0").expect("valid"),
            ProviderId::new("linux.cpu").expect("valid"),
            MetricCategory::Cpu,
            MetricUnit::Percent,
            MetricKind::Gauge,
        )
        .build();

        assert_eq!(definition.value_type, MetricValueType::Number);
        assert_eq!(definition.availability, Availability::Available);
        // Labels fall back to the identifiers rather than being empty, so a
        // half-declared metric is still displayable.
        assert_eq!(definition.display_name, "cpu.usage.total");
        assert_eq!(definition.source_label, "cpu:0");
    }

    #[test]
    fn an_unavailable_metric_still_carries_full_metadata() {
        // Absent sensors stay in the catalog so the UI can explain them.
        let definition = MetricDefinitionBuilder::new(
            MetricRef::parse("cpu.temperature.package", "cpu:0").expect("valid"),
            ProviderId::new("windows.wmi").expect("valid"),
            MetricCategory::Sensors,
            MetricUnit::Celsius,
            MetricKind::Gauge,
        )
        .availability(Availability::unsupported(
            "Windows exposes no unprivileged CPU temperature API",
        ))
        .build();

        assert!(!definition.availability.is_available());
        assert_eq!(definition.unit, MetricUnit::Celsius);
        assert_eq!(definition.category, MetricCategory::Sensors);
    }
}
