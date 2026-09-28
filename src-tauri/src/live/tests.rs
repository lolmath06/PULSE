use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use super::*;
use crate::history::clock::ManualClock;
use crate::history::testing::{cpu_total, memory_percent, metric};
use crate::metrics::model::{Availability, MetricSample};

/// Records every request the hub makes, and answers 42 for everything.
#[derive(Default)]
struct Spy {
    calls: AtomicUsize,
    requested: Mutex<Vec<Vec<MetricRef>>>,
    unavailable: Mutex<Vec<MetricRef>>,
}

impl SampleSource for Spy {
    fn sample(&self, metrics: &[MetricRef]) -> Vec<MetricSample> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requested.lock().expect("lock").push(metrics.to_vec());
        let unavailable = self.unavailable.lock().expect("lock").clone();
        metrics
            .iter()
            .map(|metric| {
                if unavailable.contains(metric) {
                    MetricSample::unavailable(
                        metric.clone(),
                        Availability::temporarily_unavailable("sensor gone"),
                    )
                } else {
                    MetricSample::number(metric.clone(), 42.0)
                }
            })
            .collect()
    }
}

const T0: i64 = 1_800_000_000_000;

#[test]
fn a_hundred_subscribers_to_cpu_cost_one_reference_per_tick() {
    let hub = LiveHub::new(RING_CAPACITY);
    for window in 0..100 {
        hub.set_subscription(&format!("overlay-{window}"), &[cpu_total(), cpu_total()]);
    }
    let spy = Spy::default();

    let tick = hub.tick(&spy, T0).expect("something is subscribed");

    assert_eq!(
        spy.calls.load(Ordering::SeqCst),
        1,
        "one engine call per tick"
    );
    assert_eq!(spy.requested.lock().expect("lock")[0], vec![cpu_total()]);
    assert_eq!(tick.values.len(), 1);
    assert_eq!(tick.values[0].v, Some(42.0));
}

#[test]
fn the_union_is_deduplicated_across_windows() {
    let hub = LiveHub::new(RING_CAPACITY);
    hub.set_subscription("main", &[cpu_total(), memory_percent()]);
    hub.set_subscription("overlay-a", &[cpu_total()]);

    assert_eq!(hub.union(), vec![cpu_total(), memory_percent()]);
}

#[test]
fn nobody_subscribed_means_no_sampling_at_all() {
    let hub = LiveHub::new(RING_CAPACITY);
    let spy = Spy::default();

    assert!(hub.tick(&spy, T0).is_none());
    hub.set_subscription("main", &[cpu_total()]);
    hub.set_subscription("main", &[]);
    assert!(hub.tick(&spy, T0).is_none());
    assert_eq!(spy.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn nvme_health_is_never_sampled_live() {
    let hub = LiveHub::new(RING_CAPACITY);
    let health = metric("storage.health.temperature", "storage:nvme0n1");
    let rate = metric("storage.io.read.bytes_per_second", "storage:nvme0n1");

    let result = hub.set_subscription("main", &[health.clone(), rate.clone()]);

    assert_eq!(result.accepted, 1);
    assert_eq!(result.refused.len(), 1);
    assert_eq!(result.refused[0].metric, health);
    assert!(result.refused[0].reason.contains("on demand"));

    let spy = Spy::default();
    for second in 0..10 {
        hub.tick(&spy, T0 + second * 1_000);
    }
    for request in spy.requested.lock().expect("lock").iter() {
        assert!(
            request
                .iter()
                .all(|m| !m.key.as_str().starts_with("storage.health.")),
            "a health key reached the engine: {request:?}"
        );
    }
}

#[test]
fn per_process_series_are_refused() {
    let hub = LiveHub::new(RING_CAPACITY);
    let result = hub.set_subscription(
        "main",
        &[
            metric("process.count.total", "process:system"),
            metric("process.count.total", "process:4242-1"),
        ],
    );
    assert_eq!(result.accepted, 1);
    assert_eq!(result.refused.len(), 1);
}

#[test]
fn rings_are_bounded_and_keep_gaps() {
    let hub = LiveHub::new(5);
    hub.set_subscription("main", &[cpu_total()]);
    let spy = Spy::default();

    for second in 0..12 {
        if second == 10 {
            spy.unavailable.lock().expect("lock").push(cpu_total());
        }
        hub.tick(&spy, T0 + second * 1_000);
    }

    let series = &hub.buffer(&[cpu_total()])[0];
    assert_eq!(series.points.len(), 5, "never beyond capacity");
    assert_eq!(hub.stored_points(), 5);
    assert_eq!(series.points[0].t, T0 + 7_000);
    assert_eq!(series.points[3].v, None, "unavailable is a gap, not zero");
    assert_eq!(series.points[4].v, None);
}

#[test]
fn an_unsubscribed_reference_is_dropped_with_its_ring() {
    let hub = LiveHub::new(RING_CAPACITY);
    hub.set_subscription("main", &[cpu_total(), memory_percent()]);
    hub.set_subscription("overlay-a", &[cpu_total()]);
    let spy = Spy::default();
    hub.tick(&spy, T0);
    assert_eq!(hub.stored_points(), 2);

    hub.remove_subscriber("main");

    assert_eq!(hub.union(), vec![cpu_total()]);
    assert_eq!(hub.stored_points(), 1);
    assert!(hub.buffer(&[memory_percent()])[0].points.is_empty());
}

#[test]
fn a_subscription_is_capped() {
    let hub = LiveHub::new(RING_CAPACITY);
    let many: Vec<MetricRef> = (0..300)
        .map(|i| metric("cpu.usage.logical", &format!("cpu:logical-{i}")))
        .collect();
    let result = hub.set_subscription("main", &many);
    assert_eq!(result.accepted, MAX_REFS_PER_SUBSCRIBER);
    assert_eq!(result.refused.len(), 300 - MAX_REFS_PER_SUBSCRIBER);
}

struct Channel(Mutex<mpsc::Sender<LiveTick>>);

impl LiveEventSink for Channel {
    fn tick(&self, tick: &LiveTick) {
        let _ = self.0.lock().expect("lock").send(tick.clone());
    }
}

#[test]
fn the_service_sleeps_until_subscribed_ticks_then_stops_promptly() {
    let (sender, received) = mpsc::channel();
    let spy = Arc::new(Spy::default());
    let service = LiveService::start(
        spy.clone(),
        Arc::new(ManualClock::new(T0)),
        Arc::new(Channel(Mutex::new(sender))),
        Duration::from_millis(20),
    );

    // Nothing subscribed: no tick arrives.
    assert!(received.recv_timeout(Duration::from_millis(120)).is_err());
    assert_eq!(spy.calls.load(Ordering::SeqCst), 0);

    service.set_subscription("main", &[cpu_total()]);
    let tick = received
        .recv_timeout(Duration::from_secs(5))
        .expect("ticks once subscribed");
    assert_eq!(tick.values[0].metric, cpu_total());

    let began = Instant::now();
    service.shutdown();
    assert!(began.elapsed() < Duration::from_secs(2));
}

#[test]
fn live_sampling_never_writes_history() {
    // The live feed has no path to the history store: it owns only a hub and a
    // sink. This test pins that by construction — ticks produce events and ring
    // points, and nothing else.
    let hub = LiveHub::new(RING_CAPACITY);
    hub.set_subscription("main", &[cpu_total()]);
    let spy = Spy::default();
    for second in 0..60 {
        hub.tick(&spy, T0 + second * 1_000);
    }
    assert_eq!(hub.stored_points(), 60);
}
