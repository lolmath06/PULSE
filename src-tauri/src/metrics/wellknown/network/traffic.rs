//! Network traffic: cumulative counters, delta arithmetic and the baseline
//! state machine.
//!
//! Throughput, packet rates, error rates and drop rates are **rates**, exactly
//! like CPU usage and disk I/O. Both operating systems expose monotonic totals
//! — octets and packets since the interface came up — so a single absolute
//! read says nothing. The measurement only exists *between* two samples.
//!
//! The platform-specific part is small: obtain eight totals per interface.
//! Everything after that — the deltas, the divisions, the rollover handling —
//! lives here, is written once, and is tested once for both operating systems.
//!
//! # Why this is not [`StorageIoTracker`]
//!
//! The arithmetic is the same shape. The *data* is not: a disk has six
//! counters including two service-time accumulators that produce a latency,
//! and an interface has eight including errors and drops that produce nothing
//! of the kind. Forcing network counters through the storage tracker would
//! mean carrying two dead fields and a latency concept that has no meaning
//! here, in exchange for saving about forty lines of arithmetic that is
//! trivial to test directly.
//!
//! What *is* shared is the state machine's semantics, deliberately, so that a
//! user sees the same distinction between "waiting for a second sample",
//! "measured zero" and "cannot be measured" on a disk and on a network
//! adapter.
//!
//! [`StorageIoTracker`]: crate::metrics::wellknown::storage::StorageIoTracker
//!
//! # Zero is a measurement, absence is not
//!
//! | Situation | What PULSE publishes |
//! |---|---|
//! | Idle interface, real interval | `0 B/s`, `0 pkt/s`, `0 err/s`, `0 drop/s` — **measurements** |
//! | First sample, no baseline | **no value**, with "waiting for another sample" |
//! | Counters went backwards | **no value**, baseline restarted |
//!
//! Publishing `0 B/s` before a baseline exists would report an unmeasured
//! interface as idle, which the user cannot distinguish from a genuinely idle
//! one.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::metrics::model::{MetricError, SourceId};

/// A snapshot of one interface's cumulative counters.
///
/// Every field is a total since the counters were last reset, in the units the
/// name states. Platforms convert into these at their own edge, so this module
/// never has to know whether the OS counted octets or split packets into
/// unicast and non-unicast.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NetworkCounters {
    /// Total bytes received.
    pub receive_bytes: u64,
    /// Total bytes transmitted.
    pub transmit_bytes: u64,
    /// Total packets received, unicast and non-unicast together.
    pub receive_packets: u64,
    /// Total packets transmitted, unicast and non-unicast together.
    pub transmit_packets: u64,
    /// Total receive errors — frames the interface could not accept because
    /// something was wrong with them: a bad CRC, a framing error, a length
    /// violation.
    pub receive_errors: u64,
    /// Total transmit errors.
    pub transmit_errors: u64,
    /// Total receive drops — frames that arrived intact and were discarded
    /// anyway, because a buffer was full or no protocol handler wanted them.
    ///
    /// **Not the same counter as an error**, and not Internet packet loss.
    /// See `docs/metrics/network.md`.
    pub receive_dropped: u64,
    /// Total transmit drops.
    pub transmit_dropped: u64,
}

impl NetworkCounters {
    /// Whether any field went backwards relative to `previous`.
    ///
    /// Any single one going backwards invalidates the whole snapshot as a
    /// baseline: an interface going down and up, a driver reset or a
    /// recreated virtual interface does not clear counters selectively, and
    /// differencing the fields that happen to still be ascending would publish
    /// a throughput derived from two different eras of the link's life.
    pub const fn went_backwards(&self, previous: &Self) -> bool {
        self.receive_bytes < previous.receive_bytes
            || self.transmit_bytes < previous.transmit_bytes
            || self.receive_packets < previous.receive_packets
            || self.transmit_packets < previous.transmit_packets
            || self.receive_errors < previous.receive_errors
            || self.transmit_errors < previous.transmit_errors
            || self.receive_dropped < previous.receive_dropped
            || self.transmit_dropped < previous.transmit_dropped
    }
}

/// Every interface's counters from one system read, plus when it was taken.
///
/// Built from a **single** transaction: one `RTM_GETLINK` dump on Linux, one
/// `GetIfTable2` on Windows. Reading each interface at a different instant
/// would make their rates disagree about the interval they cover, which on a
/// busy machine is visible.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetworkSnapshot {
    /// Monotonic milliseconds since an arbitrary origin. A monotonic clock,
    /// not a wall clock: an NTP step must not become a throughput spike or a
    /// negative interval.
    pub taken_at_ms: u64,
    pub interfaces: BTreeMap<SourceId, NetworkCounters>,
}

impl NetworkSnapshot {
    pub fn new(taken_at_ms: u64) -> Self {
        Self {
            taken_at_ms,
            interfaces: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, interface: SourceId, counters: NetworkCounters) {
        self.interfaces.insert(interface, counters);
    }

    pub fn is_empty(&self) -> bool {
        self.interfaces.is_empty()
    }
}

/// Why no rate could be computed for an interface yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeedsAnotherSample {
    /// Nothing to compare against — the first reading, or an interface that
    /// has just appeared.
    NoBaseline,
    /// No time elapsed between the two reads.
    NoElapsedTime,
    /// The counters went backwards: the interface was taken down and brought
    /// back up, the driver reset, or a virtual interface was recreated with
    /// the same identity.
    CountersWentBackwards,
}

impl NeedsAnotherSample {
    /// A short explanation for the UI.
    pub const fn reason(self) -> &'static str {
        match self {
            NeedsAnotherSample::NoBaseline => {
                "network traffic is measured between two samples; waiting for the next one"
            }
            NeedsAnotherSample::NoElapsedTime => {
                "no time elapsed since the previous sample; waiting for the next one"
            }
            NeedsAnotherSample::CountersWentBackwards => {
                "this interface's counters were reset; baseline restarted"
            }
        }
    }
}

/// The rates derived for one interface between two snapshots.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetworkRates {
    pub receive_bytes_per_second: f64,
    pub transmit_bytes_per_second: f64,
    pub receive_packets_per_second: f64,
    pub transmit_packets_per_second: f64,
    pub receive_errors_per_second: f64,
    pub transmit_errors_per_second: f64,
    pub receive_dropped_per_second: f64,
    pub transmit_dropped_per_second: f64,
}

/// What one interface's sampling attempt produced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NetworkTraffic {
    /// Usable rates over a real interval.
    Ready(NetworkRates),
    /// No usable delta yet. The baseline has been (re)stored, so the next
    /// request should succeed.
    NeedsAnotherSample(NeedsAnotherSample),
}

/// Divides a counter delta by an elapsed interval.
///
/// Returns `None` when the interval is zero — dividing would produce infinity,
/// which `MetricValue::number` rejects anyway.
pub fn per_second(delta: u64, elapsed_ms: u64) -> Option<f64> {
    if elapsed_ms == 0 {
        return None;
    }

    let rate = (delta as f64) * 1000.0 / (elapsed_ms as f64);
    rate.is_finite().then_some(rate)
}

/// Compares one interface's counters against its baseline.
///
/// Pure, so every branch is testable without a lock or a map.
fn compare(
    previous: Option<(u64, NetworkCounters)>,
    taken_at_ms: u64,
    current: NetworkCounters,
) -> NetworkTraffic {
    let Some((previous_at_ms, previous)) = previous else {
        return NetworkTraffic::NeedsAnotherSample(NeedsAnotherSample::NoBaseline);
    };

    if current.went_backwards(&previous) {
        return NetworkTraffic::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards);
    }

    // A monotonic clock cannot go backwards, but a caller could still hand us
    // two snapshots out of order; treating that as "no elapsed time" restarts
    // the baseline rather than publishing a negative interval.
    let Some(elapsed_ms) = taken_at_ms.checked_sub(previous_at_ms) else {
        return NetworkTraffic::NeedsAnotherSample(NeedsAnotherSample::NoElapsedTime);
    };

    let rate = |current: u64, previous: u64| per_second(current - previous, elapsed_ms);

    let (
        Some(receive_bytes_per_second),
        Some(transmit_bytes_per_second),
        Some(receive_packets_per_second),
        Some(transmit_packets_per_second),
        Some(receive_errors_per_second),
        Some(transmit_errors_per_second),
        Some(receive_dropped_per_second),
        Some(transmit_dropped_per_second),
    ) = (
        rate(current.receive_bytes, previous.receive_bytes),
        rate(current.transmit_bytes, previous.transmit_bytes),
        rate(current.receive_packets, previous.receive_packets),
        rate(current.transmit_packets, previous.transmit_packets),
        rate(current.receive_errors, previous.receive_errors),
        rate(current.transmit_errors, previous.transmit_errors),
        rate(current.receive_dropped, previous.receive_dropped),
        rate(current.transmit_dropped, previous.transmit_dropped),
    )
    else {
        return NetworkTraffic::NeedsAnotherSample(NeedsAnotherSample::NoElapsedTime);
    };

    NetworkTraffic::Ready(NetworkRates {
        receive_bytes_per_second,
        transmit_bytes_per_second,
        receive_packets_per_second,
        transmit_packets_per_second,
        receive_errors_per_second,
        transmit_errors_per_second,
        receive_dropped_per_second,
        transmit_dropped_per_second,
    })
}

/// The baselines held between two requests.
#[derive(Debug, Default)]
struct Baselines {
    taken_at_ms: Option<u64>,
    interfaces: BTreeMap<SourceId, NetworkCounters>,
}

/// Holds the previous counters so rates can be derived without blocking.
///
/// The same design as the CPU and storage trackers: PULSE has no sampler
/// thread, and a `sleep(100ms)` inside a Tauri command would freeze the UI for
/// every request. The provider captures a baseline when it is built, and each
/// request compares against the previous one.
///
/// # One lock, not one per interface
///
/// Every interface's baseline lives behind a **single** `Mutex<Baselines>`:
/// the whole set is written at once from one system transaction, a request
/// touches all of it anyway, and a lock per interface would add interleavings
/// without removing contention.
///
/// # Interfaces that come and go
///
/// An interface missing from a snapshot has its baseline dropped, so a VPN
/// disconnected and reconnected starts from `NoBaseline` rather than
/// differencing against counters from before it left. One that appears reports
/// `NoBaseline` once and works from the next request on.
#[derive(Debug, Default)]
pub struct NetworkIoTracker {
    baselines: Mutex<Baselines>,
}

impl NetworkIoTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores an initial baseline, discarding any previous one.
    ///
    /// Errors are swallowed on purpose: failing to prime is not fatal, it only
    /// means the first sample needs a second request.
    pub fn prime(&self, snapshot: &NetworkSnapshot) {
        if let Ok(mut guard) = self.baselines.lock() {
            guard.taken_at_ms = Some(snapshot.taken_at_ms);
            guard.interfaces = snapshot.interfaces.clone();
        }
    }

    /// Computes rates against the stored baselines, which then advance.
    ///
    /// Never panics. A poisoned mutex — only possible if another thread
    /// panicked while holding it — is reported as a structured internal error
    /// rather than unwrapped. Every outcome, including the failure paths,
    /// leaves a usable baseline behind.
    pub fn update(
        &self,
        snapshot: &NetworkSnapshot,
    ) -> Result<BTreeMap<SourceId, NetworkTraffic>, MetricError> {
        let mut guard = self.baselines.lock().map_err(|_| {
            MetricError::internal("network baseline lock was poisoned by a panicking thread")
        })?;

        let previous_at_ms = guard.taken_at_ms;

        let rates = snapshot
            .interfaces
            .iter()
            .map(|(interface, &current)| {
                let previous = previous_at_ms.and_then(|at| {
                    guard
                        .interfaces
                        .get(interface)
                        .map(|&counters| (at, counters))
                });

                (
                    interface.clone(),
                    compare(previous, snapshot.taken_at_ms, current),
                )
            })
            .collect();

        // Replacing rather than merging is what drops the baselines of
        // interfaces that are no longer present.
        guard.taken_at_ms = Some(snapshot.taken_at_ms);
        guard.interfaces = snapshot.interfaces.clone();

        Ok(rates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interface(name: &str) -> SourceId {
        SourceId::new(format!("network:if-{name}")).expect("valid")
    }

    #[allow(clippy::too_many_arguments)]
    fn counters(
        receive_bytes: u64,
        transmit_bytes: u64,
        receive_packets: u64,
        transmit_packets: u64,
        receive_errors: u64,
        transmit_errors: u64,
        receive_dropped: u64,
        transmit_dropped: u64,
    ) -> NetworkCounters {
        NetworkCounters {
            receive_bytes,
            transmit_bytes,
            receive_packets,
            transmit_packets,
            receive_errors,
            transmit_errors,
            receive_dropped,
            transmit_dropped,
        }
    }

    /// Counters with only the byte and packet fields set.
    fn traffic(rx_bytes: u64, tx_bytes: u64, rx_packets: u64, tx_packets: u64) -> NetworkCounters {
        counters(rx_bytes, tx_bytes, rx_packets, tx_packets, 0, 0, 0, 0)
    }

    fn snapshot(at: u64, entries: &[(&str, NetworkCounters)]) -> NetworkSnapshot {
        let mut snapshot = NetworkSnapshot::new(at);
        for (name, counters) in entries {
            snapshot.insert(interface(name), *counters);
        }
        snapshot
    }

    fn ready(outcome: NetworkTraffic) -> NetworkRates {
        match outcome {
            NetworkTraffic::Ready(rates) => rates,
            other => panic!("expected rates, got {other:?}"),
        }
    }

    // --- arithmetic --------------------------------------------------------

    #[test]
    fn per_second_scales_a_delta_by_the_interval() {
        assert_eq!(per_second(1000, 1000), Some(1000.0));
        assert_eq!(per_second(1000, 500), Some(2000.0));
        assert_eq!(per_second(1000, 2000), Some(500.0));
        assert_eq!(per_second(0, 1000), Some(0.0), "idle is a real rate");
    }

    #[test]
    fn per_second_refuses_a_zero_interval_rather_than_dividing() {
        assert_eq!(per_second(1000, 0), None);
        assert_eq!(per_second(0, 0), None);
    }

    #[test]
    fn per_second_handles_a_fractional_interval() {
        // 1 MiB over 250 ms is 4 MiB/s; the interval must not be rounded to
        // whole seconds first.
        assert_eq!(per_second(1024 * 1024, 250), Some(4.0 * 1024.0 * 1024.0));
        assert_eq!(per_second(3, 1500), Some(2.0));
    }

    #[test]
    fn per_second_stays_finite_for_counters_near_the_top_of_u64() {
        let rate = per_second(u64::MAX / 2, 1).expect("finite");
        assert!(rate.is_finite());
        assert!(rate > 0.0);
    }

    // --- the baseline state machine ---------------------------------------

    #[test]
    fn the_first_sample_has_no_baseline_and_invents_nothing() {
        let tracker = NetworkIoTracker::new();
        let first = tracker
            .update(&snapshot(1000, &[("wlp59s0f0", traffic(0, 0, 0, 0))]))
            .expect("no panic");

        assert_eq!(
            first[&interface("wlp59s0f0")],
            NetworkTraffic::NeedsAnotherSample(NeedsAnotherSample::NoBaseline)
        );
    }

    #[test]
    fn a_normal_delta_produces_every_rate() {
        let tracker = NetworkIoTracker::new();
        tracker.prime(&snapshot(
            0,
            &[("wlp59s0f0", counters(0, 0, 0, 0, 0, 0, 0, 0))],
        ));

        // Over one second: 12 MiB in, 1 MiB out, 8400 packets in, 2100 out,
        // 2 receive errors, 1 transmit error, 5 receive drops, 0 transmit.
        let rates = ready(
            tracker
                .update(&snapshot(
                    1000,
                    &[(
                        "wlp59s0f0",
                        counters(12 * 1024 * 1024, 1024 * 1024, 8400, 2100, 2, 1, 5, 0),
                    )],
                ))
                .expect("no panic")[&interface("wlp59s0f0")],
        );

        assert_eq!(rates.receive_bytes_per_second, 12.0 * 1024.0 * 1024.0);
        assert_eq!(rates.transmit_bytes_per_second, 1024.0 * 1024.0);
        assert_eq!(rates.receive_packets_per_second, 8400.0);
        assert_eq!(rates.transmit_packets_per_second, 2100.0);
        assert_eq!(rates.receive_errors_per_second, 2.0);
        assert_eq!(rates.transmit_errors_per_second, 1.0);
        assert_eq!(rates.receive_dropped_per_second, 5.0);
        assert_eq!(rates.transmit_dropped_per_second, 0.0);
    }

    #[test]
    fn an_idle_interface_reports_zero_on_every_rate() {
        // Over a real interval these are measurements, not absences. An
        // unplugged Ethernet port genuinely moves nothing.
        let tracker = NetworkIoTracker::new();
        let idle = counters(4096, 8192, 10, 20, 0, 0, 1, 0);
        tracker.prime(&snapshot(0, &[("enp58s0", idle)]));

        let rates = ready(
            tracker
                .update(&snapshot(1000, &[("enp58s0", idle)]))
                .expect("no panic")[&interface("enp58s0")],
        );

        assert_eq!(rates.receive_bytes_per_second, 0.0);
        assert_eq!(rates.transmit_bytes_per_second, 0.0);
        assert_eq!(rates.receive_packets_per_second, 0.0);
        assert_eq!(rates.transmit_packets_per_second, 0.0);
        assert_eq!(rates.receive_errors_per_second, 0.0);
        assert_eq!(rates.transmit_dropped_per_second, 0.0);
    }

    #[test]
    fn a_receive_only_interval_leaves_the_transmit_rates_at_zero() {
        let tracker = NetworkIoTracker::new();
        tracker.prime(&snapshot(0, &[("wlp59s0f0", traffic(0, 0, 0, 0))]));

        let rates = ready(
            tracker
                .update(&snapshot(1000, &[("wlp59s0f0", traffic(65536, 0, 64, 0))]))
                .expect("no panic")[&interface("wlp59s0f0")],
        );

        assert_eq!(rates.receive_bytes_per_second, 65536.0);
        assert_eq!(rates.receive_packets_per_second, 64.0);
        assert_eq!(rates.transmit_bytes_per_second, 0.0);
        assert_eq!(rates.transmit_packets_per_second, 0.0);
    }

    #[test]
    fn a_transmit_only_interval_leaves_the_receive_rates_at_zero() {
        let tracker = NetworkIoTracker::new();
        tracker.prime(&snapshot(0, &[("wlp59s0f0", traffic(0, 0, 0, 0))]));

        let rates = ready(
            tracker
                .update(&snapshot(500, &[("wlp59s0f0", traffic(0, 4096, 0, 8))]))
                .expect("no panic")[&interface("wlp59s0f0")],
        );

        assert_eq!(rates.transmit_bytes_per_second, 8192.0);
        assert_eq!(rates.transmit_packets_per_second, 16.0);
        assert_eq!(rates.receive_bytes_per_second, 0.0);
    }

    #[test]
    fn errors_alone_are_measured_without_any_traffic() {
        // A failing cable produces errors on an interface carrying nothing.
        let tracker = NetworkIoTracker::new();
        tracker.prime(&snapshot(
            0,
            &[("enp58s0", counters(0, 0, 0, 0, 0, 0, 0, 0))],
        ));

        let rates = ready(
            tracker
                .update(&snapshot(
                    1000,
                    &[("enp58s0", counters(0, 0, 0, 0, 17, 3, 0, 0))],
                ))
                .expect("no panic")[&interface("enp58s0")],
        );

        assert_eq!(rates.receive_errors_per_second, 17.0);
        assert_eq!(rates.transmit_errors_per_second, 3.0);
        assert_eq!(rates.receive_bytes_per_second, 0.0);
        assert_eq!(rates.receive_dropped_per_second, 0.0);
    }

    #[test]
    fn drops_alone_are_measured_and_are_not_errors() {
        // Two different counters: a drop is an intact frame nobody wanted, an
        // error is a frame that arrived broken.
        let tracker = NetworkIoTracker::new();
        tracker.prime(&snapshot(
            0,
            &[("enp58s0", counters(0, 0, 0, 0, 0, 0, 0, 0))],
        ));

        let rates = ready(
            tracker
                .update(&snapshot(
                    1000,
                    &[("enp58s0", counters(0, 0, 0, 0, 0, 0, 42, 7))],
                ))
                .expect("no panic")[&interface("enp58s0")],
        );

        assert_eq!(rates.receive_dropped_per_second, 42.0);
        assert_eq!(rates.transmit_dropped_per_second, 7.0);
        assert_eq!(rates.receive_errors_per_second, 0.0);
        assert_eq!(rates.transmit_errors_per_second, 0.0);
    }

    #[test]
    fn two_reads_in_the_same_millisecond_wait_rather_than_divide_by_zero() {
        let tracker = NetworkIoTracker::new();
        tracker.prime(&snapshot(1000, &[("wlp59s0f0", traffic(0, 0, 0, 0))]));

        assert_eq!(
            tracker
                .update(&snapshot(1000, &[("wlp59s0f0", traffic(1, 1, 1, 1))]))
                .expect("no panic")[&interface("wlp59s0f0")],
            NetworkTraffic::NeedsAnotherSample(NeedsAnotherSample::NoElapsedTime)
        );
    }

    #[test]
    fn an_interface_bounced_down_and_up_restarts_its_baseline() {
        // Taking an interface down and back up clears its counters. Without
        // this, the next sample would publish a negative delta wrapped into a
        // petabit per second.
        let tracker = NetworkIoTracker::new();
        tracker.prime(&snapshot(
            0,
            &[("wlp59s0f0", traffic(1 << 40, 1 << 38, 1 << 24, 1 << 22))],
        ));

        let outcome = tracker
            .update(&snapshot(1000, &[("wlp59s0f0", traffic(4096, 512, 8, 2))]))
            .expect("no panic")[&interface("wlp59s0f0")];

        assert_eq!(
            outcome,
            NetworkTraffic::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards)
        );

        // …and the baseline advanced, so the next interval works normally.
        let rates = ready(
            tracker
                .update(&snapshot(2000, &[("wlp59s0f0", traffic(8192, 512, 16, 2))]))
                .expect("no panic")[&interface("wlp59s0f0")],
        );
        assert_eq!(rates.receive_bytes_per_second, 4096.0);
    }

    #[test]
    fn one_counter_going_backwards_invalidates_the_whole_snapshot() {
        // A reset does not pick and choose which totals it clears, so a
        // snapshot with one regressed field is not half-usable.
        let tracker = NetworkIoTracker::new();
        tracker.prime(&snapshot(
            0,
            &[("enp58s0", counters(100, 100, 10, 10, 5, 5, 5, 5))],
        ));

        let outcome = tracker
            .update(&snapshot(
                1000,
                // Only `transmit_dropped` regressed.
                &[("enp58s0", counters(200, 200, 20, 20, 6, 6, 6, 1))],
            ))
            .expect("no panic")[&interface("enp58s0")];

        assert_eq!(
            outcome,
            NetworkTraffic::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards)
        );
    }

    #[test]
    fn huge_counters_produce_a_finite_rate() {
        let tracker = NetworkIoTracker::new();
        let low = traffic(u64::MAX - 1_000_000, 0, u64::MAX - 1000, 0);
        let high = traffic(u64::MAX, 0, u64::MAX, 0);

        tracker.prime(&snapshot(0, &[("wlp59s0f0", low)]));
        let rates = ready(
            tracker
                .update(&snapshot(1000, &[("wlp59s0f0", high)]))
                .expect("no panic")[&interface("wlp59s0f0")],
        );

        assert_eq!(rates.receive_bytes_per_second, 1_000_000.0);
        assert_eq!(rates.receive_packets_per_second, 1000.0);
        assert!(rates.receive_bytes_per_second.is_finite());
    }

    #[test]
    fn a_vpn_that_disconnects_and_returns_starts_from_a_fresh_baseline() {
        let tracker = NetworkIoTracker::new();
        tracker.prime(&snapshot(
            0,
            &[("wg0", traffic(1_000_000, 500_000, 1000, 800))],
        ));

        // Disconnected: absent from this snapshot, so its baseline is dropped.
        let without = tracker
            .update(&snapshot(1000, &[("wlp59s0f0", traffic(0, 0, 0, 0))]))
            .expect("no panic");
        assert!(!without.contains_key(&interface("wg0")));

        // Reconnected with counters that restarted at zero. Had the baseline
        // survived, this would be a rollback; instead it is simply new.
        let back = tracker
            .update(&snapshot(2000, &[("wg0", traffic(4096, 1024, 4, 2))]))
            .expect("no panic");

        assert_eq!(
            back[&interface("wg0")],
            NetworkTraffic::NeedsAnotherSample(NeedsAnotherSample::NoBaseline)
        );
    }

    #[test]
    fn a_new_interface_does_not_disturb_the_others() {
        let tracker = NetworkIoTracker::new();
        tracker.prime(&snapshot(0, &[("wlp59s0f0", traffic(0, 0, 0, 0))]));

        let outcome = tracker
            .update(&snapshot(
                1000,
                &[
                    ("wlp59s0f0", traffic(2048, 0, 2, 0)),
                    ("wg0", traffic(99, 99, 9, 9)),
                ],
            ))
            .expect("no panic");

        assert_eq!(
            ready(outcome[&interface("wlp59s0f0")]).receive_bytes_per_second,
            2048.0
        );
        assert_eq!(
            outcome[&interface("wg0")],
            NetworkTraffic::NeedsAnotherSample(NeedsAnotherSample::NoBaseline)
        );
    }

    #[test]
    fn an_empty_snapshot_clears_every_baseline_without_panicking() {
        let tracker = NetworkIoTracker::new();
        tracker.prime(&snapshot(0, &[("wlp59s0f0", traffic(1, 1, 1, 1))]));

        assert!(tracker
            .update(&NetworkSnapshot::new(1000))
            .expect("no panic")
            .is_empty());

        assert_eq!(
            tracker
                .update(&snapshot(2000, &[("wlp59s0f0", traffic(2, 2, 2, 2))]))
                .expect("no panic")[&interface("wlp59s0f0")],
            NetworkTraffic::NeedsAnotherSample(NeedsAnotherSample::NoBaseline)
        );
    }

    #[test]
    fn every_waiting_reason_explains_itself() {
        for reason in [
            NeedsAnotherSample::NoBaseline,
            NeedsAnotherSample::NoElapsedTime,
            NeedsAnotherSample::CountersWentBackwards,
        ] {
            assert!(!reason.reason().is_empty());
        }
    }
}
