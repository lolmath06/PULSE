//! Storage I/O: cumulative counters, delta arithmetic and the baseline state
//! machine.
//!
//! Disk throughput, IOPS and latency are **rates**, exactly like CPU usage.
//! Both operating systems expose monotonic totals — bytes transferred and
//! operations completed since boot — so a single absolute read says nothing.
//! The measurement only exists *between* two samples.
//!
//! The platform-specific part is small: obtain four totals per device, plus
//! the cumulative time the OS spent servicing reads and writes. Everything
//! after that — the deltas, the divisions, the rollover handling and the
//! distinction between "zero activity" and "no baseline yet" — lives here, is
//! written once, and is tested once for both operating systems.
//!
//! # Zero is sometimes a measurement and sometimes a lie
//!
//! This is the subtlety the whole module is built around:
//!
//! | Metric | Idle device over a real interval |
//! |---|---|
//! | Throughput | `0 B/s` — **a true measurement.** Nothing was transferred |
//! | IOPS | `0 IOPS` — **a true measurement.** No operation completed |
//! | Latency | **No value.** Latency is an average over completed operations, and there were none to average |
//!
//! Publishing `0 ms` for an idle disk would claim it responds instantly. There
//! is no reading; [`StorageIoRates`] says so.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::metrics::model::{MetricError, SourceId};

/// Bytes in one traditional kernel I/O sector.
///
/// `/proc/diskstats` and `/sys/block/<dev>/stat` report sectors read and
/// written in **fixed 512-byte units**, whatever the device's logical block
/// size. Multiplying them by a 4096-byte logical block size — the obvious
/// mistake, and one that quietly reports eight times the real throughput on a
/// 4Kn drive — is what the tests in this module and in
/// `platform::linux::storage::diskstats` exist to prevent.
pub const KERNEL_SECTOR_BYTES: u64 = 512;

/// A snapshot of one device's cumulative I/O counters.
///
/// Every field is a total since the counters were last reset, in the units the
/// name states. Platforms convert into these units at their own edge, so this
/// module never has to know whether the OS counted sectors, bytes or ticks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StorageIoCounters {
    /// Total bytes read.
    pub read_bytes: u64,
    /// Total bytes written.
    pub write_bytes: u64,
    /// Total read operations completed.
    pub read_operations: u64,
    /// Total write operations completed.
    pub write_operations: u64,
    /// Total time spent servicing reads, in milliseconds.
    ///
    /// This is the operating system's own accounting: the wall-clock time
    /// requests spent in flight, queueing included. It is **not** the device's
    /// internal media latency, and the published metric says so.
    pub read_time_ms: u64,
    /// Total time spent servicing writes, in milliseconds.
    pub write_time_ms: u64,
}

impl StorageIoCounters {
    /// Whether any field went backwards relative to `previous`.
    ///
    /// Any single one going backwards invalidates the whole snapshot as a
    /// baseline: a counter reset, a suspend/resume cycle or a device that
    /// disappeared and came back does not reset the counters selectively, and
    /// differencing the fields that happen to still be ascending would publish
    /// a throughput derived from two different eras of the device's life.
    pub const fn went_backwards(&self, previous: &Self) -> bool {
        self.read_bytes < previous.read_bytes
            || self.write_bytes < previous.write_bytes
            || self.read_operations < previous.read_operations
            || self.write_operations < previous.write_operations
            || self.read_time_ms < previous.read_time_ms
            || self.write_time_ms < previous.write_time_ms
    }
}

/// Every device's counters from one system read, plus when it was taken.
///
/// Built from a **single** pass: on Linux one `/proc/diskstats` read yields
/// every device at once. Reading each device at a different instant would make
/// their rates disagree about the interval they cover.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StorageIoSnapshot {
    /// Monotonic milliseconds since an arbitrary origin. A monotonic clock,
    /// not a wall clock: an NTP step or a daylight-saving jump must not turn
    /// into a throughput spike or a negative interval.
    pub taken_at_ms: u64,
    pub devices: BTreeMap<SourceId, StorageIoCounters>,
}

impl StorageIoSnapshot {
    pub fn new(taken_at_ms: u64) -> Self {
        Self {
            taken_at_ms,
            devices: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, device: SourceId, counters: StorageIoCounters) {
        self.devices.insert(device, counters);
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }
}

/// Why no rate could be computed for a device yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeedsAnotherSample {
    /// Nothing to compare against — the first reading, or a device that has
    /// just appeared.
    NoBaseline,
    /// No time elapsed between the two reads. Two requests within the same
    /// millisecond.
    NoElapsedTime,
    /// The counters went backwards, so the previous baseline is meaningless:
    /// a counter reset, a suspend/resume, or a device unplugged and plugged
    /// back in.
    CountersWentBackwards,
}

impl NeedsAnotherSample {
    /// A short explanation for the UI.
    pub const fn reason(self) -> &'static str {
        match self {
            NeedsAnotherSample::NoBaseline => {
                "disk activity is measured between two samples; waiting for the next one"
            }
            NeedsAnotherSample::NoElapsedTime => {
                "no time elapsed since the previous sample; waiting for the next one"
            }
            NeedsAnotherSample::CountersWentBackwards => {
                "the device's I/O counters were reset; baseline restarted"
            }
        }
    }
}

/// The rates derived for one device between two snapshots.
///
/// `None` on the whole struct means no rate exists yet; `None` on `latency`
/// alone means the interval was real but contained no completed operation to
/// average.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StorageIoRates {
    pub read_bytes_per_second: f64,
    pub write_bytes_per_second: f64,
    pub read_iops: f64,
    pub write_iops: f64,
    /// Mean time a read took, in milliseconds. `None` when no read completed
    /// during the interval — **not** `0.0`, which would claim the device
    /// answered instantly.
    pub read_latency_ms: Option<f64>,
    /// Mean time a write took, in milliseconds. `None` for the same reason.
    pub write_latency_ms: Option<f64>,
}

/// What one device's sampling attempt produced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StorageIo {
    /// Usable rates over a real interval.
    Ready(StorageIoRates),
    /// No usable delta yet. The baseline has been (re)stored, so the next
    /// request should succeed.
    ///
    /// Deliberately **not** zeroes: an unmeasured disk reported as idle is a
    /// lie the user cannot distinguish from a genuinely idle one.
    NeedsAnotherSample(NeedsAnotherSample),
}

/// Divides a counter delta by an elapsed interval.
///
/// Returns `None` when the interval is zero — dividing would produce infinity,
/// and `MetricValue::number` would reject it anyway.
pub fn per_second(delta: u64, elapsed_ms: u64) -> Option<f64> {
    if elapsed_ms == 0 {
        return None;
    }

    let rate = (delta as f64) * 1000.0 / (elapsed_ms as f64);
    rate.is_finite().then_some(rate)
}

/// Mean service time per completed operation, in milliseconds.
///
/// `None` when no operation completed, which is the point: a mean over zero
/// samples does not exist. It is **not** zero, and it is **not** the previous
/// interval's value carried forward.
pub fn average_latency_ms(delta_time_ms: u64, delta_operations: u64) -> Option<f64> {
    if delta_operations == 0 {
        return None;
    }

    let latency = (delta_time_ms as f64) / (delta_operations as f64);
    latency.is_finite().then_some(latency)
}

/// Compares one device's counters against its baseline.
///
/// Pure, so every branch is testable without a lock or a map.
fn compare(
    previous: Option<(u64, StorageIoCounters)>,
    taken_at_ms: u64,
    current: StorageIoCounters,
) -> StorageIo {
    let Some((previous_at_ms, previous)) = previous else {
        return StorageIo::NeedsAnotherSample(NeedsAnotherSample::NoBaseline);
    };

    if current.went_backwards(&previous) {
        return StorageIo::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards);
    }

    // A monotonic clock cannot go backwards, but a caller could still hand us
    // two snapshots out of order; treating that as "no elapsed time" restarts
    // the baseline instead of publishing a negative interval.
    let Some(elapsed_ms) = taken_at_ms.checked_sub(previous_at_ms) else {
        return StorageIo::NeedsAnotherSample(NeedsAnotherSample::NoElapsedTime);
    };

    let read_operations = current.read_operations - previous.read_operations;
    let write_operations = current.write_operations - previous.write_operations;

    let (
        Some(read_bytes_per_second),
        Some(write_bytes_per_second),
        Some(read_iops),
        Some(write_iops),
    ) = (
        per_second(current.read_bytes - previous.read_bytes, elapsed_ms),
        per_second(current.write_bytes - previous.write_bytes, elapsed_ms),
        per_second(read_operations, elapsed_ms),
        per_second(write_operations, elapsed_ms),
    )
    else {
        return StorageIo::NeedsAnotherSample(NeedsAnotherSample::NoElapsedTime);
    };

    StorageIo::Ready(StorageIoRates {
        read_bytes_per_second,
        write_bytes_per_second,
        read_iops,
        write_iops,
        read_latency_ms: average_latency_ms(
            current.read_time_ms - previous.read_time_ms,
            read_operations,
        ),
        write_latency_ms: average_latency_ms(
            current.write_time_ms - previous.write_time_ms,
            write_operations,
        ),
    })
}

/// The baselines held between two requests.
#[derive(Debug, Default)]
struct Baselines {
    taken_at_ms: Option<u64>,
    devices: BTreeMap<SourceId, StorageIoCounters>,
}

/// Holds the previous I/O counters so rates can be derived without blocking.
///
/// The same design as [`CpuUsageTracker`]: PULSE has no sampler thread, and a
/// `sleep(100ms)` inside a Tauri command would freeze the UI for every
/// request. The provider captures a baseline when it is built, and each
/// request compares against the previous one. The first request after startup
/// therefore reports [`StorageIo::NeedsAnotherSample`], which the interface
/// shows honestly rather than as `0 B/s`.
///
/// # One lock, not one per disk
///
/// Every device's baseline lives behind a **single** `Mutex<Baselines>`, for
/// the same reason as the CPU tracker: the whole set is written at once from
/// one system read, a request touches all of it anyway, and a lock per disk
/// would add interleavings without removing contention.
///
/// # Devices that come and go
///
/// A device missing from a snapshot has its baseline dropped, so a USB disk
/// unplugged and reconnected starts from `NoBaseline` instead of differencing
/// against counters from before it left. A device that appears reports
/// `NoBaseline` once and works from the next request on.
///
/// [`CpuUsageTracker`]: crate::metrics::wellknown::cpu::CpuUsageTracker
#[derive(Debug, Default)]
pub struct StorageIoTracker {
    baselines: Mutex<Baselines>,
}

impl StorageIoTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores an initial baseline, discarding any previous one.
    ///
    /// Errors are swallowed on purpose: failing to prime is not fatal, it only
    /// means the first sample needs a second request.
    pub fn prime(&self, snapshot: &StorageIoSnapshot) {
        if let Ok(mut guard) = self.baselines.lock() {
            guard.taken_at_ms = Some(snapshot.taken_at_ms);
            guard.devices = snapshot.devices.clone();
        }
    }

    /// Computes rates against the stored baselines, which then advance.
    ///
    /// Never panics. A poisoned mutex — only possible if another thread
    /// panicked while holding it — is reported as a structured internal error
    /// rather than unwrapped.
    ///
    /// Every outcome, including the failure paths, leaves a usable baseline
    /// behind: that is what makes the *next* request succeed.
    pub fn update(
        &self,
        snapshot: &StorageIoSnapshot,
    ) -> Result<BTreeMap<SourceId, StorageIo>, MetricError> {
        let mut guard = self.baselines.lock().map_err(|_| {
            MetricError::internal("storage I/O baseline lock was poisoned by a panicking thread")
        })?;

        let previous_at_ms = guard.taken_at_ms;

        let rates = snapshot
            .devices
            .iter()
            .map(|(device, &current)| {
                let previous = previous_at_ms
                    .and_then(|at| guard.devices.get(device).map(|&counters| (at, counters)));

                (
                    device.clone(),
                    compare(previous, snapshot.taken_at_ms, current),
                )
            })
            .collect();

        // Replacing rather than merging is what drops the baselines of devices
        // that are no longer present.
        guard.taken_at_ms = Some(snapshot.taken_at_ms);
        guard.devices = snapshot.devices.clone();

        Ok(rates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str) -> SourceId {
        SourceId::new(format!("storage:dev-{name}")).expect("valid")
    }

    fn counters(
        read_bytes: u64,
        write_bytes: u64,
        read_operations: u64,
        write_operations: u64,
        read_time_ms: u64,
        write_time_ms: u64,
    ) -> StorageIoCounters {
        StorageIoCounters {
            read_bytes,
            write_bytes,
            read_operations,
            write_operations,
            read_time_ms,
            write_time_ms,
        }
    }

    fn snapshot(at: u64, entries: &[(&str, StorageIoCounters)]) -> StorageIoSnapshot {
        let mut snapshot = StorageIoSnapshot::new(at);
        for (name, counters) in entries {
            snapshot.insert(device(name), *counters);
        }
        snapshot
    }

    fn ready(outcome: StorageIo) -> StorageIoRates {
        match outcome {
            StorageIo::Ready(rates) => rates,
            other => panic!("expected rates, got {other:?}"),
        }
    }

    // --- arithmetic -------------------------------------------------------

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
        // 1 MiB over 250 ms is 4 MiB/s, and the arithmetic must not round the
        // interval to whole seconds first.
        assert_eq!(per_second(1024 * 1024, 250), Some(4.0 * 1024.0 * 1024.0));
        assert_eq!(per_second(3, 1500), Some(2.0));
    }

    #[test]
    fn per_second_stays_finite_for_counters_near_the_top_of_u64() {
        let rate = per_second(u64::MAX / 2, 1).expect("finite");
        assert!(rate.is_finite());
        assert!(rate > 0.0);
    }

    #[test]
    fn average_latency_divides_service_time_by_completed_operations() {
        assert_eq!(average_latency_ms(100, 100), Some(1.0));
        assert_eq!(average_latency_ms(73, 100), Some(0.73));
        assert_eq!(average_latency_ms(36, 2), Some(18.0));
    }

    #[test]
    fn average_latency_has_no_value_when_nothing_completed() {
        // The distinction the whole module exists for: no operations means no
        // average, which is not the same as an average of zero.
        assert_eq!(average_latency_ms(0, 0), None);
        // Even when the OS accounted some in-flight time, an interval in which
        // nothing *completed* has no mean to report.
        assert_eq!(average_latency_ms(42, 0), None);
    }

    #[test]
    fn average_latency_of_zero_is_reported_when_operations_did_complete() {
        // Fast operations on an SSD genuinely round to 0 ms of accounted time.
        assert_eq!(average_latency_ms(0, 50), Some(0.0));
    }

    // --- the baseline state machine ---------------------------------------

    #[test]
    fn the_first_sample_has_no_baseline_and_invents_nothing() {
        let tracker = StorageIoTracker::new();
        let first = tracker
            .update(&snapshot(1000, &[("nvme0n1", counters(0, 0, 0, 0, 0, 0))]))
            .expect("no panic");

        assert_eq!(
            first[&device("nvme0n1")],
            StorageIo::NeedsAnotherSample(NeedsAnotherSample::NoBaseline)
        );
    }

    #[test]
    fn a_normal_delta_produces_every_rate() {
        let tracker = StorageIoTracker::new();
        tracker.prime(&snapshot(0, &[("nvme0n1", counters(0, 0, 0, 0, 0, 0))]));

        // Over one second: 1 MiB read in 100 reads taking 73 ms in total,
        // 512 KiB written in 20 writes taking 24 ms.
        let rates = ready(
            tracker
                .update(&snapshot(
                    1000,
                    &[(
                        "nvme0n1",
                        counters(1024 * 1024, 512 * 1024, 100, 20, 73, 24),
                    )],
                ))
                .expect("no panic")[&device("nvme0n1")],
        );

        assert_eq!(rates.read_bytes_per_second, 1024.0 * 1024.0);
        assert_eq!(rates.write_bytes_per_second, 512.0 * 1024.0);
        assert_eq!(rates.read_iops, 100.0);
        assert_eq!(rates.write_iops, 20.0);
        assert_eq!(rates.read_latency_ms, Some(0.73));
        assert_eq!(rates.write_latency_ms, Some(1.2));
    }

    #[test]
    fn an_idle_device_reports_zero_throughput_and_zero_iops_but_no_latency() {
        // Three different answers for the same interval, and all three are
        // correct: nothing moved, nothing completed, nothing to average.
        let tracker = StorageIoTracker::new();
        let idle = counters(4096, 8192, 10, 20, 5, 7);
        tracker.prime(&snapshot(0, &[("nvme0n1", idle)]));

        let rates = ready(
            tracker
                .update(&snapshot(1000, &[("nvme0n1", idle)]))
                .expect("no panic")[&device("nvme0n1")],
        );

        assert_eq!(rates.read_bytes_per_second, 0.0);
        assert_eq!(rates.write_bytes_per_second, 0.0);
        assert_eq!(rates.read_iops, 0.0);
        assert_eq!(rates.write_iops, 0.0);
        assert_eq!(rates.read_latency_ms, None, "0 ms would be fabricated");
        assert_eq!(rates.write_latency_ms, None);
    }

    #[test]
    fn a_read_only_workload_leaves_the_write_latency_unmeasured() {
        let tracker = StorageIoTracker::new();
        tracker.prime(&snapshot(0, &[("sda", counters(0, 0, 0, 0, 0, 0))]));

        let rates = ready(
            tracker
                .update(&snapshot(
                    1000,
                    &[("sda", counters(65536, 0, 16, 0, 32, 0))],
                ))
                .expect("no panic")[&device("sda")],
        );

        assert_eq!(rates.read_iops, 16.0);
        assert_eq!(rates.read_latency_ms, Some(2.0));
        assert_eq!(rates.write_iops, 0.0, "zero writes is a measurement");
        assert_eq!(rates.write_latency_ms, None, "and has no latency");
    }

    #[test]
    fn a_write_only_workload_leaves_the_read_latency_unmeasured() {
        let tracker = StorageIoTracker::new();
        tracker.prime(&snapshot(0, &[("sda", counters(0, 0, 0, 0, 0, 0))]));

        let rates = ready(
            tracker
                .update(&snapshot(500, &[("sda", counters(0, 4096, 0, 4, 0, 40))]))
                .expect("no panic")[&device("sda")],
        );

        assert_eq!(rates.write_bytes_per_second, 8192.0);
        assert_eq!(rates.write_latency_ms, Some(10.0));
        assert_eq!(rates.read_bytes_per_second, 0.0);
        assert_eq!(rates.read_latency_ms, None);
    }

    #[test]
    fn two_reads_in_the_same_millisecond_wait_rather_than_divide_by_zero() {
        let tracker = StorageIoTracker::new();
        tracker.prime(&snapshot(1000, &[("nvme0n1", counters(0, 0, 0, 0, 0, 0))]));

        assert_eq!(
            tracker
                .update(&snapshot(1000, &[("nvme0n1", counters(1, 1, 1, 1, 1, 1))]))
                .expect("no panic")[&device("nvme0n1")],
            StorageIo::NeedsAnotherSample(NeedsAnotherSample::NoElapsedTime)
        );
    }

    #[test]
    fn a_counter_reset_restarts_the_baseline_instead_of_publishing_a_spike() {
        let tracker = StorageIoTracker::new();
        tracker.prime(&snapshot(
            0,
            &[(
                "sda",
                counters(1 << 40, 1 << 40, 1 << 20, 1 << 20, 900, 900),
            )],
        ));

        // The device was reset: everything is back near zero. Differencing
        // would yield a negative delta, and wrapping it would publish a
        // terabyte per second.
        let outcome = tracker
            .update(&snapshot(1000, &[("sda", counters(4096, 0, 1, 0, 1, 0))]))
            .expect("no panic")[&device("sda")];

        assert_eq!(
            outcome,
            StorageIo::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards)
        );

        // …and the baseline advanced, so the next interval works normally.
        let rates = ready(
            tracker
                .update(&snapshot(2000, &[("sda", counters(8192, 0, 2, 0, 3, 0))]))
                .expect("no panic")[&device("sda")],
        );
        assert_eq!(rates.read_bytes_per_second, 4096.0);
    }

    #[test]
    fn one_counter_going_backwards_invalidates_the_whole_snapshot() {
        // A reset does not pick and choose which totals it clears, so a
        // snapshot with one regressed field is not half-usable.
        let tracker = StorageIoTracker::new();
        tracker.prime(&snapshot(0, &[("sda", counters(100, 100, 10, 10, 10, 10))]));

        let outcome = tracker
            .update(&snapshot(
                1000,
                // Only `write_time_ms` regressed.
                &[("sda", counters(200, 200, 20, 20, 20, 5))],
            ))
            .expect("no panic")[&device("sda")];

        assert_eq!(
            outcome,
            StorageIo::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards)
        );
    }

    #[test]
    fn huge_counters_produce_a_finite_rate() {
        let tracker = StorageIoTracker::new();
        let low = counters(u64::MAX - 1_000_000, 0, u64::MAX - 1000, 0, 0, 0);
        let high = counters(u64::MAX, 0, u64::MAX, 0, 0, 0);

        tracker.prime(&snapshot(0, &[("nvme0n1", low)]));
        let rates = ready(
            tracker
                .update(&snapshot(1000, &[("nvme0n1", high)]))
                .expect("no panic")[&device("nvme0n1")],
        );

        assert_eq!(rates.read_bytes_per_second, 1_000_000.0);
        assert_eq!(rates.read_iops, 1000.0);
        assert!(rates.read_bytes_per_second.is_finite());
    }

    #[test]
    fn a_device_that_disappears_and_returns_starts_from_a_fresh_baseline() {
        let tracker = StorageIoTracker::new();
        tracker.prime(&snapshot(
            0,
            &[("usb", counters(1_000_000, 0, 1000, 0, 500, 0))],
        ));

        // Unplugged: absent from this snapshot, so its baseline is dropped.
        let without = tracker
            .update(&snapshot(1000, &[("nvme0n1", counters(0, 0, 0, 0, 0, 0))]))
            .expect("no panic");
        assert!(!without.contains_key(&device("usb")));

        // Reconnected with counters that restarted at zero. Had the baseline
        // survived, this would be a rollback; instead it is simply new.
        let back = tracker
            .update(&snapshot(2000, &[("usb", counters(4096, 0, 4, 0, 2, 0))]))
            .expect("no panic");

        assert_eq!(
            back[&device("usb")],
            StorageIo::NeedsAnotherSample(NeedsAnotherSample::NoBaseline)
        );
    }

    #[test]
    fn a_new_device_does_not_disturb_the_others() {
        let tracker = StorageIoTracker::new();
        tracker.prime(&snapshot(0, &[("nvme0n1", counters(0, 0, 0, 0, 0, 0))]));

        let outcome = tracker
            .update(&snapshot(
                1000,
                &[
                    ("nvme0n1", counters(2048, 0, 2, 0, 4, 0)),
                    ("usb", counters(99, 99, 9, 9, 9, 9)),
                ],
            ))
            .expect("no panic");

        assert_eq!(
            ready(outcome[&device("nvme0n1")]).read_bytes_per_second,
            2048.0
        );
        assert_eq!(
            outcome[&device("usb")],
            StorageIo::NeedsAnotherSample(NeedsAnotherSample::NoBaseline)
        );
    }

    #[test]
    fn an_empty_snapshot_clears_every_baseline_without_panicking() {
        let tracker = StorageIoTracker::new();
        tracker.prime(&snapshot(0, &[("nvme0n1", counters(1, 1, 1, 1, 1, 1))]));

        assert!(tracker
            .update(&StorageIoSnapshot::new(1000))
            .expect("no panic")
            .is_empty());

        assert_eq!(
            tracker
                .update(&snapshot(2000, &[("nvme0n1", counters(2, 2, 2, 2, 2, 2))]))
                .expect("no panic")[&device("nvme0n1")],
            StorageIo::NeedsAnotherSample(NeedsAnotherSample::NoBaseline)
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

    #[test]
    fn a_kernel_sector_is_512_bytes_whatever_the_devices_block_size() {
        // The classic bug this constant guards: multiplying diskstats sectors
        // by a 4096-byte logical block size reports eight times the real
        // throughput.
        assert_eq!(KERNEL_SECTOR_BYTES, 512);
        assert_eq!(1024 * KERNEL_SECTOR_BYTES, 524_288);
    }
}
