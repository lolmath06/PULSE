//! The live widget feed: **one** shared sampler for every visible widget.
//!
//! History records every 5 s into SQLite. Widgets and overlays want the
//! current value sooner, and a sparkline of the last minute should not wait
//! for a week of history. This is the separate, non-persistent path:
//!
//! ```text
//!  window A widgets ─┐ set_live_subscription (one per window, deduplicated)
//!  window B widgets ─┼──────────────► LiveHub ── union of refs ──► engine.sample (once per tick)
//!  overlay widgets ──┘                  │
//!                                       ├─► LiveRing per ref (in memory, bounded)
//!                                       └─► live-sample event ─► every window
//! ```
//!
//! # Rules
//!
//! - **One tick for everyone.** A hundred widgets asking for CPU in three
//!   windows are one reference in one engine call per tick.
//! - **Only what is shown.** The union of the subscriptions is sampled — no
//!   subscription, no sampling at all: the thread sleeps.
//! - **Never costly metrics.** `storage.health.*` (an NVMe admin command) and
//!   per-process series are refused at subscription time, with the reason.
//! - **Never persisted.** Nothing here touches the history database; the
//!   history cadence stays 5 s.
//! - **Bounded.** [`RING_CAPACITY`] points per reference, and a reference
//!   nobody subscribes to any more is dropped with its ring.
//!
//! Tauri-free: the event sink is a trait, so the Windows harness checks it and
//! tests drive it without a window.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::history::clock::Clock;
use crate::history::sampler::{SampleSource, TickSchedule};
use crate::metrics::model::{MetricRef, MetricValue};
use crate::metrics::wellknown::storage;

/// How often visible widgets are refreshed. History stays at 5 s.
pub const LIVE_CADENCE: Duration = Duration::from_secs(1);

/// Points kept per reference: five minutes at one per second.
pub const RING_CAPACITY: usize = 300;

/// The most references one window may subscribe to.
pub const MAX_REFS_PER_SUBSCRIBER: usize = 256;

/// Why a reference may not be sampled live, or `None` when it may.
pub fn live_refusal(metric: &MetricRef) -> Option<&'static str> {
    if storage::HEALTH_KEYS.contains(&metric.key.as_str()) {
        return Some(
            "drive health is an NVMe admin command; it is read on demand, never every second",
        );
    }
    let source = metric.source_id.as_str();
    if source.starts_with("process:") && source != "process:system" {
        return Some("individual processes are shown in the process table, not in widgets");
    }
    None
}

/// One reading in a ring. `None` is an unavailable sample: a gap, not a zero.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LivePoint {
    pub t: i64,
    pub v: Option<f64>,
}

/// A reference refused at subscription time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refused {
    pub metric: MetricRef,
    pub reason: String,
}

/// What a subscription became.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionResult {
    pub accepted: usize,
    pub refused: Vec<Refused>,
    pub cadence_ms: u64,
}

/// One value of one tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveValue {
    pub metric: MetricRef,
    pub v: Option<f64>,
}

/// The payload of one tick: every subscribed reference, once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTick {
    pub t: i64,
    pub values: Vec<LiveValue>,
}

/// A ring's contents, for a window that just subscribed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSeries {
    pub metric: MetricRef,
    pub points: Vec<LivePoint>,
}

/// Where ticks go: Tauri events in the app, a channel in tests.
pub trait LiveEventSink: Send + Sync {
    fn tick(&self, tick: &LiveTick);
}

#[derive(Default)]
struct HubState {
    subscriptions: HashMap<String, BTreeSet<MetricRef>>,
    rings: HashMap<MetricRef, VecDeque<LivePoint>>,
}

/// Subscriptions and rings. Pure: no thread, no clock of its own.
#[derive(Default)]
pub struct LiveHub {
    state: Mutex<HubState>,
    capacity: usize,
}

impl std::fmt::Debug for LiveHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveHub")
            .field("capacity", &self.capacity)
            .finish()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl LiveHub {
    pub fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(HubState::default()),
            capacity: capacity.max(1),
        }
    }

    /// Replaces what `subscriber` (a window) wants. An empty list removes it.
    pub fn set_subscription(&self, subscriber: &str, metrics: &[MetricRef]) -> SubscriptionResult {
        let mut refused = Vec::new();
        let mut accepted = BTreeSet::new();
        for metric in metrics {
            if let Some(reason) = live_refusal(metric) {
                refused.push(Refused {
                    metric: metric.clone(),
                    reason: reason.to_string(),
                });
            } else if accepted.len() < MAX_REFS_PER_SUBSCRIBER {
                accepted.insert(metric.clone());
            } else {
                refused.push(Refused {
                    metric: metric.clone(),
                    reason: format!("at most {MAX_REFS_PER_SUBSCRIBER} live metrics per window"),
                });
            }
        }

        let count = accepted.len();
        let mut state = lock(&self.state);
        if accepted.is_empty() {
            state.subscriptions.remove(subscriber);
        } else {
            state.subscriptions.insert(subscriber.to_string(), accepted);
        }
        Self::drop_orphans(&mut state);

        SubscriptionResult {
            accepted: count,
            refused,
            cadence_ms: LIVE_CADENCE.as_millis() as u64,
        }
    }

    /// Forgets a window entirely — it was closed.
    pub fn remove_subscriber(&self, subscriber: &str) {
        let mut state = lock(&self.state);
        state.subscriptions.remove(subscriber);
        Self::drop_orphans(&mut state);
    }

    fn drop_orphans(state: &mut HubState) {
        let wanted: BTreeSet<&MetricRef> = state.subscriptions.values().flatten().collect();
        let keep: Vec<MetricRef> = state
            .rings
            .keys()
            .filter(|metric| wanted.contains(metric))
            .cloned()
            .collect();
        state.rings.retain(|metric, _| keep.contains(metric));
    }

    /// The deduplicated set of references to sample, in a stable order.
    pub fn union(&self) -> Vec<MetricRef> {
        let state = lock(&self.state);
        let set: BTreeSet<&MetricRef> = state.subscriptions.values().flatten().collect();
        set.into_iter().cloned().collect()
    }

    pub fn subscriber_count(&self) -> usize {
        lock(&self.state).subscriptions.len()
    }

    /// Samples the union once and records it. `None` — and **no sampling** —
    /// when nobody is subscribed.
    pub fn tick(&self, source: &dyn SampleSource, now_ms: i64) -> Option<LiveTick> {
        let union = self.union();
        if union.is_empty() {
            return None;
        }
        let samples = source.sample(&union);
        let mut values = Vec::with_capacity(samples.len());
        for sample in samples {
            let v = match (&sample.value, sample.availability.is_available()) {
                (Some(MetricValue::Number(value)), true) if value.is_finite() => Some(*value),
                _ => None,
            };
            values.push(LiveValue {
                metric: sample.metric,
                v,
            });
        }

        let mut state = lock(&self.state);
        for value in &values {
            // A reference unsubscribed during sampling is not resurrected.
            if !state
                .subscriptions
                .values()
                .any(|set| set.contains(&value.metric))
            {
                continue;
            }
            let ring = state.rings.entry(value.metric.clone()).or_default();
            if ring.len() == self.capacity {
                ring.pop_front();
            }
            ring.push_back(LivePoint {
                t: now_ms,
                v: value.v,
            });
        }
        Some(LiveTick { t: now_ms, values })
    }

    /// The recorded points of `metrics`, oldest first. Unavailable readings
    /// are kept as `None`, so the gaps stay where they were.
    pub fn buffer(&self, metrics: &[MetricRef]) -> Vec<LiveSeries> {
        let state = lock(&self.state);
        metrics
            .iter()
            .map(|metric| LiveSeries {
                metric: metric.clone(),
                points: state
                    .rings
                    .get(metric)
                    .map(|ring| ring.iter().copied().collect())
                    .unwrap_or_default(),
            })
            .collect()
    }

    /// Total points held, for tests and diagnostics.
    pub fn stored_points(&self) -> usize {
        lock(&self.state).rings.values().map(VecDeque::len).sum()
    }
}

struct Worker {
    wake: mpsc::Sender<Wake>,
    handle: JoinHandle<()>,
}

enum Wake {
    Changed,
    Stop,
}

/// The live sampler thread around a [`LiveHub`].
pub struct LiveService {
    hub: Arc<LiveHub>,
    worker: Mutex<Option<Worker>>,
}

impl std::fmt::Debug for LiveService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveService")
            .field("hub", &self.hub)
            .finish()
    }
}

impl LiveService {
    /// Starts the thread. It sleeps until something is subscribed.
    pub fn start(
        source: Arc<dyn SampleSource>,
        clock: Arc<dyn Clock>,
        sink: Arc<dyn LiveEventSink>,
        cadence: Duration,
    ) -> Self {
        let hub = Arc::new(LiveHub::new(RING_CAPACITY));
        let (wake, woken) = mpsc::channel();
        let thread_hub = Arc::clone(&hub);
        let handle = std::thread::Builder::new()
            .name("pulse-live".into())
            .spawn(move || {
                run(
                    &thread_hub,
                    source.as_ref(),
                    clock.as_ref(),
                    sink.as_ref(),
                    cadence,
                    &woken,
                )
            })
            .ok();
        Self {
            hub,
            worker: Mutex::new(handle.map(|handle| Worker { wake, handle })),
        }
    }

    pub fn hub(&self) -> &LiveHub {
        &self.hub
    }

    pub fn set_subscription(&self, subscriber: &str, metrics: &[MetricRef]) -> SubscriptionResult {
        let result = self.hub.set_subscription(subscriber, metrics);
        self.wake();
        result
    }

    pub fn remove_subscriber(&self, subscriber: &str) {
        self.hub.remove_subscriber(subscriber);
        self.wake();
    }

    fn wake(&self) {
        if let Some(worker) = lock(&self.worker).as_ref() {
            let _ = worker.wake.send(Wake::Changed);
        }
    }

    /// Stops the thread. Idempotent.
    pub fn shutdown(&self) {
        if let Some(worker) = lock(&self.worker).take() {
            let _ = worker.wake.send(Wake::Stop);
            let _ = worker.handle.join();
        }
    }
}

impl Drop for LiveService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(
    hub: &LiveHub,
    source: &dyn SampleSource,
    clock: &dyn Clock,
    sink: &dyn LiveEventSink,
    cadence: Duration,
    woken: &mpsc::Receiver<Wake>,
) {
    let mut schedule = TickSchedule::new(cadence, Instant::now());
    loop {
        if hub.subscriber_count() == 0 {
            // Nothing visible: sleep until a window subscribes. No CPU at all.
            match woken.recv() {
                Ok(Wake::Changed) => {
                    schedule = TickSchedule::new(cadence, Instant::now());
                    continue;
                }
                Ok(Wake::Stop) | Err(_) => return,
            }
        }

        if let Some(tick) = hub.tick(source, clock.now_ms()) {
            sink.tick(&tick);
        }

        // Wait for the next tick. A subscription change does not trigger an
        // extra sample: the wait resumes until the scheduled instant.
        let deadline = Instant::now() + schedule.after_tick(Instant::now());
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match woken.recv_timeout(remaining) {
                Ok(Wake::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Wake::Changed) => continue,
                Err(RecvTimeoutError::Timeout) => break,
            }
        }
    }
}

#[cfg(test)]
mod tests;
