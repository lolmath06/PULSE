//! A configurable in-memory provider used **only by tests**.
//!
//! This module is `#[cfg(test)]`: it is not compiled into the shipped binary,
//! so PULSE can never display a fabricated temperature or CPU load. The engine
//! is still exercised end to end, which is what lets Phase 1 ship a tested
//! engine with no real collectors yet.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::metrics::model::{
    Availability, MetricCategory, MetricDefinition, MetricDefinitionBuilder, MetricError,
    MetricKind, MetricRef, MetricSample, MetricUnit, MetricValue, ProviderId,
};
use crate::metrics::providers::MetricProvider;

/// What the mock should do when asked for a particular metric.
#[derive(Debug, Clone)]
pub enum MockBehaviour {
    /// Return this numeric value.
    Value(f64),
    /// Return a sample with this non-available status.
    Unavailable(Availability),
    /// Declare the metric but return no sample for it, exercising the engine's
    /// missing-sample handling.
    Omit,
}

/// A provider whose catalog and behaviour are fully scripted.
pub struct MockProvider {
    id: ProviderId,
    definitions: Vec<MetricDefinition>,
    behaviours: HashMap<MetricRef, MockBehaviour>,
    /// `Some` makes `describe()` fail.
    describe_error: Option<MetricError>,
    /// `Some` makes `sample()` fail wholesale.
    sample_error: Option<MetricError>,
    /// How many times `sample()` has been called, to prove the engine batches.
    sample_calls: AtomicUsize,
}

impl MockProvider {
    pub fn new(id: &str) -> Self {
        Self {
            id: ProviderId::new(id).expect("test provider id must be valid"),
            definitions: Vec::new(),
            behaviours: HashMap::new(),
            describe_error: None,
            sample_error: None,
            sample_calls: AtomicUsize::new(0),
        }
    }

    /// Declares a metric that returns `value` when sampled.
    pub fn with_metric(self, key: &str, source: &str, value: f64) -> Self {
        self.with_behaviour(key, source, MockBehaviour::Value(value))
    }

    /// Declares a metric with scripted behaviour.
    pub fn with_behaviour(mut self, key: &str, source: &str, behaviour: MockBehaviour) -> Self {
        let metric = MetricRef::parse(key, source).expect("test metric reference must be valid");

        let availability = match &behaviour {
            MockBehaviour::Unavailable(availability) => availability.clone(),
            _ => Availability::Available,
        };

        self.definitions.push(
            MetricDefinitionBuilder::new(
                metric.clone(),
                self.id.clone(),
                MetricCategory::Other,
                MetricUnit::Count,
                MetricKind::Gauge,
            )
            .display_name(format!("Mock {key}"))
            .source_label(format!("Mock source {source}"))
            .description("Test-only metric.")
            .availability(availability)
            .build(),
        );

        self.behaviours.insert(metric, behaviour);
        self
    }

    /// Declares the same metric reference twice, to exercise duplicate
    /// detection within a single provider.
    pub fn with_duplicate_metric(mut self, key: &str, source: &str) -> Self {
        self = self.with_metric(key, source, 1.0);
        self.with_metric(key, source, 2.0)
    }

    /// Makes `describe()` fail.
    pub fn failing_describe(mut self, error: MetricError) -> Self {
        self.describe_error = Some(error);
        self
    }

    /// Makes `sample()` fail wholesale.
    pub fn failing_sample(mut self, error: MetricError) -> Self {
        self.sample_error = Some(error);
        self
    }

    /// Number of times the engine has called `sample()` on this provider.
    pub fn sample_call_count(&self) -> usize {
        self.sample_calls.load(Ordering::SeqCst)
    }
}

impl MetricProvider for MockProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError> {
        match &self.describe_error {
            Some(error) => Err(error.clone()),
            None => Ok(self.definitions.clone()),
        }
    }

    fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError> {
        self.sample_calls.fetch_add(1, Ordering::SeqCst);

        if let Some(error) = &self.sample_error {
            return Err(error.clone());
        }

        let samples = requested
            .iter()
            .filter_map(|metric| match self.behaviours.get(metric) {
                Some(MockBehaviour::Value(value)) => Some(MetricSample::available(
                    metric.clone(),
                    MetricValue::number(*value).expect("test values must be finite"),
                )),
                Some(MockBehaviour::Unavailable(availability)) => Some(MetricSample::unavailable(
                    metric.clone(),
                    availability.clone(),
                )),
                Some(MockBehaviour::Omit) | None => None,
            })
            .collect();

        Ok(samples)
    }
}
