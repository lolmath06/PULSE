//! Engine behaviour tests, driven entirely by the test-only mock provider.

use std::sync::Arc;

use super::*;
use crate::metrics::model::{MetricErrorCode, MetricValue};
use crate::metrics::providers::mock::{MockBehaviour, MockProvider};
use crate::metrics::METRICS_SCHEMA_VERSION;

fn engine_with(providers: Vec<Arc<dyn MetricProvider>>) -> MetricsEngine {
    let mut engine = MetricsEngine::new();
    for provider in providers {
        engine
            .register(provider)
            .expect("registration must succeed");
    }
    engine
}

fn metric(key: &str, source: &str) -> MetricRef {
    MetricRef::parse(key, source).expect("valid reference")
}

// --- registration --------------------------------------------------------

#[test]
fn a_new_engine_is_empty() {
    let engine = MetricsEngine::new();

    assert!(engine.catalog().is_empty());
    assert_eq!(engine.status().provider_count, 0);
    assert_eq!(engine.status().state, EngineState::Empty);
}

#[test]
fn registers_a_provider_and_adopts_its_metrics() {
    let engine = engine_with(vec![Arc::new(
        MockProvider::new("mock.a")
            .with_metric("cpu.usage.total", "cpu:0", 12.0)
            .with_metric("memory.used", "system:host", 2048.0),
    )]);

    assert_eq!(engine.catalog().len(), 2);
    assert!(engine.knows(&metric("cpu.usage.total", "cpu:0")));

    let status = engine.status();
    assert_eq!(status.state, EngineState::Ready);
    assert_eq!(status.provider_count, 1);
    assert_eq!(status.metric_count, 2);
    assert_eq!(status.providers[0].id.as_str(), "mock.a");
}

#[test]
fn aggregates_the_catalog_across_several_providers() {
    let engine = engine_with(vec![
        Arc::new(MockProvider::new("mock.cpu").with_metric("cpu.usage.total", "cpu:0", 1.0)),
        Arc::new(MockProvider::new("mock.gpu").with_metric("gpu.usage.core", "gpu:card0", 2.0)),
        Arc::new(MockProvider::new("mock.net").with_metric(
            "network.download.rate",
            "network:eth0",
            3.0,
        )),
    ]);

    let status = engine.status();
    assert_eq!(status.provider_count, 3);
    assert_eq!(status.metric_count, 3);
    assert_eq!(engine.catalog().len(), 3);
}

#[test]
fn the_catalog_order_is_deterministic_regardless_of_registration_order() {
    let forwards = engine_with(vec![
        Arc::new(MockProvider::new("mock.a").with_metric("memory.used", "system:host", 1.0)),
        Arc::new(MockProvider::new("mock.b").with_metric("cpu.usage.total", "cpu:0", 2.0)),
        Arc::new(MockProvider::new("mock.c").with_metric("gpu.usage.core", "gpu:card0", 3.0)),
    ]);

    let backwards = engine_with(vec![
        Arc::new(MockProvider::new("mock.c").with_metric("gpu.usage.core", "gpu:card0", 3.0)),
        Arc::new(MockProvider::new("mock.b").with_metric("cpu.usage.total", "cpu:0", 2.0)),
        Arc::new(MockProvider::new("mock.a").with_metric("memory.used", "system:host", 1.0)),
    ]);

    let keys = |engine: &MetricsEngine| -> Vec<String> {
        engine
            .catalog()
            .iter()
            .map(|definition| definition.metric.to_string())
            .collect()
    };

    assert_eq!(
        keys(&forwards),
        [
            "cpu.usage.total@cpu:0",
            "gpu.usage.core@gpu:card0",
            "memory.used@system:host",
        ]
    );
    assert_eq!(keys(&forwards), keys(&backwards));
}

#[test]
fn multiple_sources_of_the_same_key_coexist() {
    // The reason MetricKey and SourceId are separate: two GPUs, one key.
    let engine = engine_with(vec![Arc::new(
        MockProvider::new("mock.gpu")
            .with_metric("gpu.temperature.core", "gpu:pci-0000-01-00-0", 71.0)
            .with_metric("gpu.temperature.core", "gpu:pci-0000-00-02-0", 48.0),
    )]);

    assert_eq!(engine.catalog().len(), 2);

    let samples = engine.sample(&[
        metric("gpu.temperature.core", "gpu:pci-0000-01-00-0"),
        metric("gpu.temperature.core", "gpu:pci-0000-00-02-0"),
    ]);

    assert_eq!(samples[0].value, MetricValue::number(71.0));
    assert_eq!(samples[1].value, MetricValue::number(48.0));
}

#[test]
fn a_collision_between_providers_is_rejected_explicitly() {
    let mut engine = MetricsEngine::new();
    engine
        .register(Arc::new(MockProvider::new("mock.first").with_metric(
            "cpu.usage.total",
            "cpu:0",
            1.0,
        )))
        .expect("first registration");

    let result = engine.register(Arc::new(MockProvider::new("mock.second").with_metric(
        "cpu.usage.total",
        "cpu:0",
        2.0,
    )));

    let Err(RegistrationError::MetricCollision {
        metric: colliding,
        existing_provider,
        new_provider,
    }) = result
    else {
        panic!("expected a collision, got {result:?}");
    };

    assert_eq!(colliding, metric("cpu.usage.total", "cpu:0"));
    assert_eq!(existing_provider.as_str(), "mock.first");
    assert_eq!(new_provider.as_str(), "mock.second");
}

#[test]
fn a_rejected_registration_leaves_the_engine_untouched() {
    // Atomicity: the second provider also brings a brand new metric, which must
    // not be adopted when the registration fails.
    let mut engine = MetricsEngine::new();
    engine
        .register(Arc::new(MockProvider::new("mock.first").with_metric(
            "cpu.usage.total",
            "cpu:0",
            1.0,
        )))
        .expect("first registration");

    let result = engine.register(Arc::new(
        MockProvider::new("mock.second")
            .with_metric("memory.used", "system:host", 5.0)
            .with_metric("cpu.usage.total", "cpu:0", 2.0),
    ));

    assert!(result.is_err());
    assert_eq!(engine.catalog().len(), 1);
    assert_eq!(engine.status().provider_count, 1);
    assert!(!engine.knows(&metric("memory.used", "system:host")));
}

#[test]
fn a_provider_declaring_the_same_metric_twice_is_rejected() {
    let mut engine = MetricsEngine::new();

    let result = engine.register(Arc::new(
        MockProvider::new("mock.sloppy").with_duplicate_metric("cpu.usage.total", "cpu:0"),
    ));

    assert!(matches!(
        result,
        Err(RegistrationError::DuplicateWithinProvider { .. })
    ));
    assert!(engine.catalog().is_empty());
}

#[test]
fn two_providers_cannot_share_an_identifier() {
    let mut engine = MetricsEngine::new();
    engine
        .register(Arc::new(MockProvider::new("mock.dup")))
        .expect("first registration");

    let result = engine.register(Arc::new(MockProvider::new("mock.dup")));

    assert!(matches!(
        result,
        Err(RegistrationError::DuplicateProvider { .. })
    ));
}

#[test]
fn a_provider_that_cannot_describe_itself_fails_registration_cleanly() {
    let mut engine = MetricsEngine::new();

    let result = engine.register(Arc::new(MockProvider::new("mock.broken").failing_describe(
        MetricError::new(MetricErrorCode::ProviderUnavailable, "driver missing"),
    )));

    assert!(matches!(result, Err(RegistrationError::Describe { .. })));
    assert_eq!(engine.status().provider_count, 0);

    // The failure converts into a frontend-shaped error rather than a string.
    let error: MetricError = result.unwrap_err().into();
    assert_eq!(error.code, MetricErrorCode::ProviderUnavailable);
}

// --- lookup --------------------------------------------------------------

#[test]
fn resolves_a_known_metric_to_its_definition() {
    let engine = engine_with(vec![Arc::new(MockProvider::new("mock.a").with_metric(
        "cpu.usage.total",
        "cpu:0",
        1.0,
    ))]);

    let definition = engine
        .definition(&metric("cpu.usage.total", "cpu:0"))
        .expect("metric is registered");

    assert_eq!(definition.provider_id.as_str(), "mock.a");
    assert_eq!(definition.display_name, "Mock cpu.usage.total");
}

#[test]
fn an_unknown_metric_resolves_to_nothing_without_panicking() {
    let engine = engine_with(vec![Arc::new(MockProvider::new("mock.a").with_metric(
        "cpu.usage.total",
        "cpu:0",
        1.0,
    ))]);

    assert!(engine
        .definition(&metric("gpu.usage.core", "gpu:card0"))
        .is_none());
    assert!(!engine.knows(&metric("gpu.usage.core", "gpu:card0")));
}

// --- sampling ------------------------------------------------------------

#[test]
fn samples_a_single_known_metric() {
    let engine = engine_with(vec![Arc::new(MockProvider::new("mock.a").with_metric(
        "cpu.usage.total",
        "cpu:0",
        42.5,
    ))]);

    let samples = engine.sample(&[metric("cpu.usage.total", "cpu:0")]);

    assert_eq!(samples.len(), 1);
    assert!(samples[0].availability.is_available());
    assert_eq!(samples[0].value, MetricValue::number(42.5));
}

#[test]
fn samples_several_metrics_in_request_order() {
    let engine = engine_with(vec![
        Arc::new(MockProvider::new("mock.a").with_metric("cpu.usage.total", "cpu:0", 1.0)),
        Arc::new(MockProvider::new("mock.b").with_metric("memory.used", "system:host", 2.0)),
    ]);

    // Deliberately request provider B before provider A.
    let samples = engine.sample(&[
        metric("memory.used", "system:host"),
        metric("cpu.usage.total", "cpu:0"),
    ]);

    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].metric, metric("memory.used", "system:host"));
    assert_eq!(samples[1].metric, metric("cpu.usage.total", "cpu:0"));
}

#[test]
fn an_empty_request_returns_an_empty_response() {
    let engine = engine_with(vec![Arc::new(MockProvider::new("mock.a").with_metric(
        "cpu.usage.total",
        "cpu:0",
        1.0,
    ))]);

    assert!(engine.sample(&[]).is_empty());
}

#[test]
fn only_the_requested_metrics_are_sampled() {
    // A dashboard showing five metrics must not cost a full catalog sweep.
    let engine = engine_with(vec![Arc::new(
        MockProvider::new("mock.a")
            .with_metric("cpu.usage.total", "cpu:0", 1.0)
            .with_metric("memory.used", "system:host", 2.0)
            .with_metric("network.download.rate", "network:eth0", 3.0),
    )]);

    let samples = engine.sample(&[metric("memory.used", "system:host")]);

    assert_eq!(samples.len(), 1);
    assert_eq!(engine.catalog().len(), 3);
}

#[test]
fn each_provider_is_called_once_per_request() {
    let provider = Arc::new(
        MockProvider::new("mock.a")
            .with_metric("cpu.usage.total", "cpu:0", 1.0)
            .with_metric("memory.used", "system:host", 2.0),
    );
    let engine = engine_with(vec![provider.clone()]);

    engine.sample(&[
        metric("cpu.usage.total", "cpu:0"),
        metric("memory.used", "system:host"),
        // Duplicated on purpose: the provider must still be asked once.
        metric("cpu.usage.total", "cpu:0"),
    ]);

    assert_eq!(provider.sample_call_count(), 1);
}

#[test]
fn a_repeated_reference_is_answered_for_every_occurrence() {
    let engine = engine_with(vec![Arc::new(MockProvider::new("mock.a").with_metric(
        "cpu.usage.total",
        "cpu:0",
        7.0,
    ))]);

    let reference = metric("cpu.usage.total", "cpu:0");
    let samples = engine.sample(&[reference.clone(), reference.clone()]);

    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].value, MetricValue::number(7.0));
    assert_eq!(samples[1].value, MetricValue::number(7.0));
}

#[test]
fn an_unknown_metric_is_reported_as_not_registered() {
    let engine = engine_with(vec![Arc::new(MockProvider::new("mock.a").with_metric(
        "cpu.usage.total",
        "cpu:0",
        1.0,
    ))]);

    let samples = engine.sample(&[metric("gpu.temperature.core", "gpu:card0")]);

    assert_eq!(samples.len(), 1);
    assert!(!samples[0].has_value());
    assert!(matches!(
        samples[0].availability,
        Availability::NotRegistered { .. }
    ));
}

#[test]
fn an_unknown_metric_does_not_spoil_the_known_ones() {
    let engine = engine_with(vec![Arc::new(MockProvider::new("mock.a").with_metric(
        "cpu.usage.total",
        "cpu:0",
        9.0,
    ))]);

    let samples = engine.sample(&[
        metric("gpu.temperature.core", "gpu:card0"),
        metric("cpu.usage.total", "cpu:0"),
    ]);

    assert!(matches!(
        samples[0].availability,
        Availability::NotRegistered { .. }
    ));
    assert_eq!(samples[1].value, MetricValue::number(9.0));
}

#[test]
fn a_failing_provider_does_not_affect_the_others() {
    // The headline isolation guarantee: GPU down, CPU and network still fine.
    let engine = engine_with(vec![
        Arc::new(MockProvider::new("mock.cpu").with_metric("cpu.usage.total", "cpu:0", 11.0)),
        Arc::new(
            MockProvider::new("mock.gpu")
                .with_metric("gpu.usage.core", "gpu:card0", 0.0)
                .failing_sample(MetricError::new(
                    MetricErrorCode::ProviderUnavailable,
                    "NVML handle lost",
                )),
        ),
        Arc::new(MockProvider::new("mock.net").with_metric(
            "network.download.rate",
            "network:eth0",
            33.0,
        )),
    ]);

    let samples = engine.sample(&[
        metric("cpu.usage.total", "cpu:0"),
        metric("gpu.usage.core", "gpu:card0"),
        metric("network.download.rate", "network:eth0"),
    ]);

    assert_eq!(samples[0].value, MetricValue::number(11.0));

    let Availability::ProviderError { error } = &samples[1].availability else {
        panic!("expected the GPU provider to report an error");
    };
    assert_eq!(error.code, MetricErrorCode::ProviderUnavailable);
    assert_eq!(error.message, "NVML handle lost");

    assert_eq!(samples[2].value, MetricValue::number(33.0));
}

#[test]
fn a_metric_declared_unavailable_reports_its_reason() {
    let engine = engine_with(vec![Arc::new(MockProvider::new("mock.a").with_behaviour(
        "cpu.temperature.package",
        "cpu:0",
        MockBehaviour::Unavailable(Availability::permission_denied("needs root")),
    ))]);

    let samples = engine.sample(&[metric("cpu.temperature.package", "cpu:0")]);

    assert!(!samples[0].has_value());
    assert!(matches!(
        &samples[0].availability,
        Availability::PermissionDenied { reason } if reason == "needs root"
    ));

    // An unavailable metric is still catalogued so the UI can explain it.
    assert_eq!(engine.catalog().len(), 1);
    assert_eq!(engine.status().metric_count, 1);
    assert_eq!(engine.status().available_metric_count, 0);
}

#[test]
fn a_provider_omitting_a_declared_metric_yields_a_missing_sample_error() {
    let engine = engine_with(vec![Arc::new(MockProvider::new("mock.a").with_behaviour(
        "cpu.usage.total",
        "cpu:0",
        MockBehaviour::Omit,
    ))]);

    let samples = engine.sample(&[metric("cpu.usage.total", "cpu:0")]);

    assert_eq!(samples.len(), 1, "a gap must never shorten the response");
    let Availability::ProviderError { error } = &samples[0].availability else {
        panic!("expected a provider error for the omitted sample");
    };
    assert_eq!(error.code, MetricErrorCode::MissingSample);
    assert!(error.recoverable);
}

#[test]
fn nothing_in_the_failure_paths_panics() {
    // One request exercising every failure mode at once.
    let engine = engine_with(vec![
        Arc::new(
            MockProvider::new("mock.broken")
                .with_metric("gpu.usage.core", "gpu:card0", 0.0)
                .failing_sample(MetricError::new(MetricErrorCode::Io, "device gone")),
        ),
        Arc::new(
            MockProvider::new("mock.partial")
                .with_behaviour("cpu.usage.total", "cpu:0", MockBehaviour::Omit)
                .with_behaviour(
                    "cpu.temperature.package",
                    "cpu:0",
                    MockBehaviour::Unavailable(Availability::not_detected("no sensor")),
                ),
        ),
    ]);

    let samples = engine.sample(&[
        metric("gpu.usage.core", "gpu:card0"),
        metric("cpu.usage.total", "cpu:0"),
        metric("cpu.temperature.package", "cpu:0"),
        metric("memory.used", "system:host"),
    ]);

    assert_eq!(samples.len(), 4);
    assert!(samples.iter().all(|sample| !sample.has_value()));
    assert!(samples
        .iter()
        .all(|sample| !sample.availability.is_available()));
}

// --- contract ------------------------------------------------------------

#[test]
fn the_status_reports_the_contract_version() {
    let engine = MetricsEngine::new();
    assert_eq!(engine.status().schema_version, METRICS_SCHEMA_VERSION);
    assert_eq!(METRICS_SCHEMA_VERSION, 1);
}

#[test]
fn the_status_breaks_availability_down_per_provider() {
    let engine = engine_with(vec![
        Arc::new(
            MockProvider::new("mock.good")
                .with_metric("cpu.usage.total", "cpu:0", 1.0)
                .with_metric("memory.used", "system:host", 2.0),
        ),
        Arc::new(MockProvider::new("mock.limited").with_behaviour(
            "cpu.temperature.package",
            "cpu:0",
            MockBehaviour::Unavailable(Availability::unsupported("no API")),
        )),
    ]);

    let status = engine.status();

    assert_eq!(status.metric_count, 3);
    assert_eq!(status.available_metric_count, 2);

    // Summaries are in registration order.
    assert_eq!(status.providers[0].id.as_str(), "mock.good");
    assert_eq!(status.providers[0].available_metric_count, 2);
    assert_eq!(status.providers[1].id.as_str(), "mock.limited");
    assert_eq!(status.providers[1].metric_count, 1);
    assert_eq!(status.providers[1].available_metric_count, 0);
}

#[test]
fn the_engine_is_usable_from_several_threads() {
    // Proves the Send + Sync bound the Tauri state relies on.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MetricsEngine>();

    let engine = Arc::new(engine_with(vec![Arc::new(
        MockProvider::new("mock.a").with_metric("cpu.usage.total", "cpu:0", 5.0),
    )]));

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let engine = Arc::clone(&engine);
            std::thread::spawn(move || {
                engine.sample(&[metric("cpu.usage.total", "cpu:0")])[0]
                    .value
                    .clone()
            })
        })
        .collect();

    for handle in handles {
        assert_eq!(handle.join().expect("thread"), MetricValue::number(5.0));
    }
}
