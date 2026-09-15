//! Linux CPU usage, read from `/proc/stat`.
//!
//! Parsing is a pure function over the file's text so it can be tested with
//! fixtures — including malformed ones — without a Linux host and without
//! racing the real kernel counters.

use std::fs;
use std::sync::Arc;

use crate::metrics::model::{
    Availability, MetricDefinition, MetricError, MetricErrorCode, MetricRef, MetricSample,
    ProviderId,
};
use crate::metrics::providers::MetricProvider;
use crate::metrics::wellknown::availability_for;
use crate::metrics::wellknown::cpu::{self, CpuCounters, CpuUsage, CpuUsageTracker};

const PROC_STAT: &str = "/proc/stat";

/// Identifier of the Linux CPU provider.
pub const PROVIDER_ID: &str = "linux.cpu";

/// The aggregate `cpu` line of `/proc/stat`, in jiffies.
///
/// Field order is fixed by the kernel and documented in `proc(5)`. Fields
/// beyond `guest_nice` have been added over time and are ignored rather than
/// treated as an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcStatCpu {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
    pub guest: u64,
    pub guest_nice: u64,
}

impl ProcStatCpu {
    /// Time the CPU was not doing work.
    ///
    /// `iowait` counts as idle: the CPU is genuinely not executing anything
    /// while a task waits for I/O.
    pub const fn idle_all(&self) -> u64 {
        self.idle + self.iowait
    }

    /// Total CPU time across all states.
    ///
    /// **`guest` and `guest_nice` are excluded on purpose.** The kernel already
    /// includes guest time inside `user`, and guest_nice inside `nice`; adding
    /// them again inflates the total and makes a busy host look idle. This is
    /// the classic `/proc/stat` bug.
    pub const fn total(&self) -> u64 {
        self.user
            + self.nice
            + self.system
            + self.idle
            + self.iowait
            + self.irq
            + self.softirq
            + self.steal
    }

    /// Total minus idle.
    pub const fn busy(&self) -> u64 {
        self.total() - self.idle_all()
    }

    /// Platform-neutral counters for the shared usage arithmetic.
    pub const fn counters(&self) -> CpuCounters {
        CpuCounters::new(self.busy(), self.total())
    }
}

/// Parses the aggregate `cpu` line out of `/proc/stat` content.
///
/// Accepts the documented format and tolerates the two things that vary in
/// practice: extra trailing fields added by newer kernels, and irregular
/// whitespace. The per-core `cpu0`, `cpu1`… lines are skipped — this phase
/// reports aggregate usage only.
pub fn parse_proc_stat(content: &str) -> Result<ProcStatCpu, MetricError> {
    let line = content
        .lines()
        .find(|line| {
            let mut parts = line.split_ascii_whitespace();
            // Exactly "cpu", not "cpu0": the aggregate line only.
            parts.next() == Some("cpu")
        })
        .ok_or_else(|| {
            MetricError::new(
                MetricErrorCode::Parse,
                "no aggregate 'cpu' line found in /proc/stat",
            )
        })?;

    let mut fields = line.split_ascii_whitespace().skip(1);

    // The first four fields have been present since Linux 2.x; anything less
    // is not a /proc/stat we understand.
    let mut required = || -> Result<u64, MetricError> {
        let raw = fields.next().ok_or_else(|| {
            MetricError::new(
                MetricErrorCode::Parse,
                "the 'cpu' line of /proc/stat has fewer than four fields",
            )
        })?;

        raw.parse::<u64>().map_err(|error| {
            MetricError::new(
                MetricErrorCode::Parse,
                format!("invalid value '{raw}' in /proc/stat: {error}"),
            )
        })
    };

    let user = required()?;
    let nice = required()?;
    let system = required()?;
    let idle = required()?;

    // Fields added in later kernels. Absent means zero; malformed is ignored
    // rather than fatal, since the four fields above already give a usable
    // measurement.
    let mut optional = || fields.next().and_then(|raw| raw.parse::<u64>().ok());

    Ok(ProcStatCpu {
        user,
        nice,
        system,
        idle,
        iowait: optional().unwrap_or(0),
        irq: optional().unwrap_or(0),
        softirq: optional().unwrap_or(0),
        steal: optional().unwrap_or(0),
        guest: optional().unwrap_or(0),
        guest_nice: optional().unwrap_or(0),
    })
}

/// Reads and parses `/proc/stat`.
fn read_counters() -> Result<CpuCounters, MetricError> {
    let content = fs::read_to_string(PROC_STAT).map_err(|error| {
        MetricError::new(
            MetricErrorCode::Io,
            format!("could not read {PROC_STAT}: {error}"),
        )
    })?;

    Ok(parse_proc_stat(&content)?.counters())
}

/// Publishes `cpu.usage.total` on Linux.
///
/// Requires no elevated privileges: `/proc/stat` is world-readable.
#[derive(Debug)]
pub struct LinuxCpuProvider {
    id: ProviderId,
    tracker: CpuUsageTracker,
}

impl LinuxCpuProvider {
    /// Builds the provider and captures a CPU baseline immediately.
    ///
    /// Priming here rather than on first request means the very first sample
    /// usually already has a delta to work with. If the read fails, the
    /// provider still works — the first request simply reports
    /// `temporarilyUnavailable` and primes then.
    pub fn new() -> Self {
        let tracker = CpuUsageTracker::new();

        if let Ok(counters) = read_counters() {
            tracker.prime(counters);
        }

        Self {
            id: ProviderId::new(PROVIDER_ID).expect("provider id must be valid"),
            tracker,
        }
    }
}

impl Default for LinuxCpuProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for LinuxCpuProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError> {
        Ok(cpu::definitions(&self.id))
    }

    fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError> {
        // One read serves every requested reference; today that is only
        // `cpu.usage.total`, but the shape holds when per-core metrics arrive.
        let outcome = read_counters().and_then(|counters| self.tracker.update(counters));

        let samples = requested
            .iter()
            .map(|reference| match &outcome {
                Ok(CpuUsage::Ready(percent)) => MetricSample::number(reference.clone(), *percent),
                Ok(CpuUsage::NeedsAnotherSample(reason)) => MetricSample::unavailable(
                    reference.clone(),
                    Availability::temporarily_unavailable(reason.reason()),
                ),
                Err(error) => {
                    MetricSample::unavailable(reference.clone(), availability_for(error.clone()))
                }
            })
            .collect();

        Ok(samples)
    }
}

/// Builds the Linux CPU provider.
pub fn provider() -> Arc<dyn MetricProvider> {
    Arc::new(LinuxCpuProvider::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::wellknown::cpu::NeedsAnotherSample;

    /// A realistic `/proc/stat`, abbreviated.
    const REAL_PROC_STAT: &str = "\
cpu  123456 789 45678 9876543 12345 678 910 11 22 33
cpu0 30000 200 11000 2469135 3000 170 230 3 5 8
cpu1 30500 190 11200 2469140 3100 168 225 2 6 9
intr 123456789 0 0 0
ctxt 987654321
btime 1700000000
processes 123456
procs_running 2
procs_blocked 0
";

    #[test]
    fn parses_a_real_proc_stat() {
        let parsed = parse_proc_stat(REAL_PROC_STAT).expect("valid");

        assert_eq!(parsed.user, 123_456);
        assert_eq!(parsed.nice, 789);
        assert_eq!(parsed.system, 45_678);
        assert_eq!(parsed.idle, 9_876_543);
        assert_eq!(parsed.iowait, 12_345);
        assert_eq!(parsed.irq, 678);
        assert_eq!(parsed.softirq, 910);
        assert_eq!(parsed.steal, 11);
        assert_eq!(parsed.guest, 22);
        assert_eq!(parsed.guest_nice, 33);
    }

    #[test]
    fn reads_the_aggregate_line_not_the_per_core_ones() {
        let parsed = parse_proc_stat(REAL_PROC_STAT).expect("valid");
        // cpu0's user is 30000; the aggregate's is 123456.
        assert_eq!(parsed.user, 123_456);
    }

    #[test]
    fn guest_time_is_not_counted_twice() {
        // The classic /proc/stat bug: the kernel already includes `guest`
        // inside `user` and `guest_nice` inside `nice`. Adding them again
        // inflates the total and makes a busy machine look idle.
        let with_guest = parse_proc_stat("cpu 100 100 100 100 0 0 0 0 50 50\n").expect("valid");
        let without_guest = parse_proc_stat("cpu 100 100 100 100 0 0 0 0 0 0\n").expect("valid");

        assert_eq!(with_guest.total(), without_guest.total());
        assert_eq!(with_guest.total(), 400);
        assert_eq!(with_guest.busy(), 300);
    }

    #[test]
    fn iowait_counts_as_idle() {
        let parsed = parse_proc_stat("cpu 100 0 100 700 100 0 0 0\n").expect("valid");

        assert_eq!(parsed.idle_all(), 800);
        assert_eq!(parsed.total(), 1000);
        assert_eq!(parsed.busy(), 200);
    }

    #[test]
    fn tolerates_irregular_whitespace() {
        let parsed = parse_proc_stat("cpu     100   200\t300    400\n").expect("valid");

        assert_eq!(parsed.user, 100);
        assert_eq!(parsed.nice, 200);
        assert_eq!(parsed.system, 300);
        assert_eq!(parsed.idle, 400);
    }

    #[test]
    fn accepts_a_kernel_with_only_the_four_original_fields() {
        let parsed = parse_proc_stat("cpu 100 200 300 400\n").expect("valid");

        assert_eq!(parsed.iowait, 0);
        assert_eq!(parsed.steal, 0);
        assert_eq!(parsed.total(), 1000);
        assert_eq!(parsed.busy(), 600);
    }

    #[test]
    fn ignores_extra_fields_a_future_kernel_might_add() {
        let parsed =
            parse_proc_stat("cpu 100 100 100 100 0 0 0 0 0 0 555 666 777\n").expect("valid");

        assert_eq!(parsed.total(), 400);
    }

    #[test]
    fn rejects_content_with_no_aggregate_cpu_line() {
        for content in ["", "intr 1 2 3\nctxt 4\n", "cpu0 1 2 3 4\ncpu1 1 2 3 4\n"] {
            let error = parse_proc_stat(content).expect_err("must be rejected");
            assert_eq!(error.code, MetricErrorCode::Parse);
        }
    }

    #[test]
    fn rejects_a_truncated_cpu_line() {
        for content in ["cpu\n", "cpu 100\n", "cpu 100 200 300\n"] {
            let error = parse_proc_stat(content).expect_err("must be rejected");
            assert_eq!(error.code, MetricErrorCode::Parse);
        }
    }

    #[test]
    fn rejects_a_non_numeric_required_field() {
        for content in [
            "cpu abc 200 300 400\n",
            "cpu 100 - 300 400\n",
            "cpu 100 200 300 4.5\n",
            "cpu 100 200 300 -400\n",
        ] {
            let error = parse_proc_stat(content).expect_err("must be rejected");
            assert_eq!(error.code, MetricErrorCode::Parse);
            assert!(error.message.contains("invalid value") || error.message.contains("fewer"));
        }
    }

    #[test]
    fn a_malformed_optional_field_does_not_lose_the_measurement() {
        // The four required fields are enough to measure usage; refusing the
        // whole read because `steal` is odd would be worse than ignoring it.
        let parsed = parse_proc_stat("cpu 100 100 100 100 xx\n").expect("valid");

        assert_eq!(parsed.idle, 100);
        assert_eq!(parsed.iowait, 0);
    }

    #[test]
    fn counters_feed_the_shared_usage_arithmetic() {
        let parsed = parse_proc_stat("cpu 100 0 100 700 100 0 0 0\n").expect("valid");
        let counters = parsed.counters();

        assert_eq!(counters.busy, 200);
        assert_eq!(counters.total, 1000);
        assert!(counters.is_consistent());
    }

    #[test]
    fn a_progressing_counter_produces_a_usage_between_zero_and_one_hundred() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(
            parse_proc_stat("cpu 100 0 100 700 100 0 0 0\n")
                .expect("valid")
                .counters(),
        );

        // 100 more busy jiffies, 300 more idle: 100 busy out of 400 elapsed.
        let outcome = tracker
            .update(
                parse_proc_stat("cpu 200 0 100 1000 100 0 0 0\n")
                    .expect("valid")
                    .counters(),
            )
            .expect("no error");

        assert_eq!(outcome, CpuUsage::Ready(25.0));
    }

    #[test]
    fn an_unchanged_counter_waits_instead_of_reporting_idle() {
        let line = "cpu 100 0 100 700 100 0 0 0\n";
        let tracker = CpuUsageTracker::new();
        tracker.prime(parse_proc_stat(line).expect("valid").counters());

        assert_eq!(
            tracker
                .update(parse_proc_stat(line).expect("valid").counters())
                .expect("no error"),
            CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoElapsedTime)
        );
    }

    #[test]
    fn a_rewound_counter_resets_the_baseline() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(
            parse_proc_stat("cpu 1000 0 1000 7000 0 0 0 0\n")
                .expect("valid")
                .counters(),
        );

        assert_eq!(
            tracker
                .update(
                    parse_proc_stat("cpu 100 0 100 700 0 0 0 0\n")
                        .expect("valid")
                        .counters()
                )
                .expect("no error"),
            CpuUsage::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards)
        );
    }

    #[test]
    fn io_errors_are_transient_not_unsupported() {
        // A momentarily unreadable /proc must not tell the user their machine
        // lacks a CPU sensor.
        let availability = availability_for(MetricError::new(MetricErrorCode::Io, "busy"));
        assert!(matches!(
            availability,
            Availability::TemporarilyUnavailable { .. }
        ));

        let availability = availability_for(MetricError::new(
            MetricErrorCode::PermissionDenied,
            "denied",
        ));
        assert!(matches!(
            availability,
            Availability::PermissionDenied { .. }
        ));

        let availability = availability_for(MetricError::new(MetricErrorCode::Parse, "bad"));
        assert!(matches!(availability, Availability::ProviderError { .. }));
    }

    // --- host tests: assert invariants only, never specific values --------
    // Gated to Linux: the parsing tests above run on every platform, but these
    // read the real /proc, which only exists here.

    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_proc_stat_on_this_host_parses() {
        let counters = read_counters().expect("/proc/stat must be readable without privileges");

        assert!(counters.total > 0);
        assert!(counters.is_consistent());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_provider_declares_exactly_the_cpu_metric() {
        let provider = LinuxCpuProvider::new();
        let definitions = provider.describe().expect("describe");

        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].metric, cpu::usage_total_ref());
        assert_eq!(provider.id().as_str(), "linux.cpu");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_provider_produces_a_plausible_sample_on_this_host() {
        let provider = LinuxCpuProvider::new();
        let reference = cpu::usage_total_ref();

        // The first sample may legitimately still be waiting for a delta.
        let samples = provider
            .sample(std::slice::from_ref(&reference))
            .expect("sample");
        assert_eq!(samples.len(), 1);

        // Busy-wait briefly so the jiffy counter advances, then try again.
        let mut spin = 0_u64;
        for i in 0..8_000_000_u64 {
            spin = spin.wrapping_add(i);
        }
        assert!(spin > 0);

        let samples = provider.sample(&[reference]).expect("sample");
        let sample = &samples[0];

        match &sample.availability {
            Availability::Available => {
                let percent = sample
                    .value
                    .as_ref()
                    .and_then(|value| value.as_number())
                    .expect("an available CPU sample carries a number");
                assert!(
                    (0.0..=100.0).contains(&percent),
                    "CPU usage out of range: {percent}"
                );
            }
            // Acceptable: an idle enough machine may advance no jiffies.
            Availability::TemporarilyUnavailable { .. } => {}
            other => panic!("unexpected CPU availability: {other:?}"),
        }

        // Never a fabricated zero.
        if !sample.availability.is_available() {
            assert!(sample.value.is_none());
        }
    }
}
