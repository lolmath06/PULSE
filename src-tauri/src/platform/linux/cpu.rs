//! Linux CPU metrics: usage from `/proc/stat`, frequency and topology from
//! `/sys/devices/system/cpu/`.
//!
//! Parsing is a pure function over the files' text so it can be tested with
//! fixtures — including malformed ones — without a Linux host and without
//! racing the real kernel counters.
//!
//! # One read feeds every usage metric
//!
//! `/proc/stat` contains the aggregate `cpu` line *and* a `cpuN` line for
//! every online logical processor. A refresh therefore reads it **once** and
//! derives `cpu.usage.total` and all N `cpu.usage.logical` values from that
//! single snapshot. Reading it once per metric would be thirty-three reads
//! instead of one, and — worse — would sample the processors at slightly
//! different instants, so the per-processor figures would not reconcile with
//! the aggregate.

use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;

use crate::metrics::model::{
    Availability, MetricDefinition, MetricError, MetricErrorCode, MetricRef, MetricSample,
    ProviderId,
};
use crate::metrics::providers::MetricProvider;
use crate::metrics::wellknown::availability_for;
use crate::metrics::wellknown::cpu::{
    self, CpuCounters, CpuSnapshot, CpuTopology, CpuUsage, CpuUsageReport, CpuUsageTracker,
    LogicalId, LogicalProcessor,
};

use super::cpu_sysfs;

const PROC_STAT: &str = "/proc/stat";

/// Identifier of the Linux CPU provider.
///
/// **One provider owns every CPU metric on the machine**, not one per
/// processor. Thirty-two providers would each re-read `/proc/stat`, would each
/// appear in the engine status, and would gain nothing: they all read the same
/// file and share one baseline.
pub const PROVIDER_ID: &str = "linux.cpu";

/// One CPU time line of `/proc/stat`, in jiffies.
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

/// Everything the CPU time section of `/proc/stat` says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcStat {
    /// The aggregate `cpu` line.
    pub aggregate: Option<ProcStatCpu>,
    /// The `cpuN` lines, keyed by the kernel's own CPU number.
    pub logical: BTreeMap<LogicalId, ProcStatCpu>,
}

impl ProcStat {
    /// Converts the parsed lines into the shared snapshot type.
    ///
    /// **The identical formula is applied to the aggregate and to every
    /// logical processor**, which is what makes `cpu.usage.total` and the
    /// `cpu.usage.logical` values comparable instead of two different
    /// definitions of "busy" that happen to have similar names.
    pub fn to_snapshot(&self) -> CpuSnapshot {
        let mut snapshot = CpuSnapshot::new();

        if let Some(aggregate) = self.aggregate {
            snapshot = snapshot.with_total(aggregate.counters());
        }
        for (&id, line) in &self.logical {
            snapshot.insert_logical(id, line.counters());
        }

        snapshot
    }
}

/// Parses one CPU time line's fields, given everything after the label.
///
/// The first four fields have been present since Linux 2.x; anything less is
/// not a `/proc/stat` we understand. Fields added in later kernels are
/// optional — absent means zero, and malformed is ignored rather than fatal,
/// since the four required fields already give a usable measurement.
fn parse_cpu_fields<'a>(
    mut fields: impl Iterator<Item = &'a str>,
    label: &str,
) -> Result<ProcStatCpu, MetricError> {
    let mut required = || -> Result<u64, MetricError> {
        let raw = fields.next().ok_or_else(|| {
            MetricError::new(
                MetricErrorCode::Parse,
                format!("the '{label}' line of /proc/stat has fewer than four fields"),
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

/// Parses the aggregate `cpu` line out of `/proc/stat` content.
///
/// Kept as its own entry point because `cpu.usage.total` must survive a
/// machine whose per-processor lines are unreadable.
pub fn parse_proc_stat(content: &str) -> Result<ProcStatCpu, MetricError> {
    parse_proc_stat_all(content)?.aggregate.ok_or_else(|| {
        MetricError::new(
            MetricErrorCode::Parse,
            "no aggregate 'cpu' line found in /proc/stat",
        )
    })
}

/// Parses every CPU time line out of `/proc/stat` content.
///
/// Accepts the documented format and tolerates what varies in practice: extra
/// trailing fields added by newer kernels, irregular whitespace, and lines
/// other than the CPU ones.
///
/// **A malformed `cpuN` line is skipped, not fatal.** Losing one processor's
/// usage is bad; losing the whole machine's because one line was truncated
/// mid-read — `/proc/stat` is generated on the fly and is not atomic — would
/// be worse. A malformed *aggregate* line is an error, because nothing else
/// can stand in for it.
pub fn parse_proc_stat_all(content: &str) -> Result<ProcStat, MetricError> {
    let mut parsed = ProcStat::default();
    let mut aggregate_error: Option<MetricError> = None;

    for line in content.lines() {
        let mut parts = line.split_ascii_whitespace();
        let Some(label) = parts.next() else {
            continue;
        };
        let Some(suffix) = label.strip_prefix("cpu") else {
            // `intr`, `ctxt`, `btime`… — everything after the CPU section.
            continue;
        };

        if suffix.is_empty() {
            match parse_cpu_fields(parts, "cpu") {
                Ok(line) => parsed.aggregate = Some(line),
                Err(error) => aggregate_error = Some(error),
            }
            continue;
        }

        // `cpu0`, `cpu31`… The kernel's number becomes PULSE's ordinal
        // directly, so `cpu7` is `cpu:logical-7` and lines up with what
        // `htop` and `taskset` call it.
        let Ok(ordinal) = suffix.parse::<u32>() else {
            continue;
        };
        if let Ok(line) = parse_cpu_fields(parts, label) {
            parsed.logical.insert(LogicalId::new(ordinal), line);
        }
    }

    match (parsed.aggregate, aggregate_error) {
        (None, Some(error)) => Err(error),
        _ => Ok(parsed),
    }
}

/// Reads and parses `/proc/stat`.
fn read_proc_stat() -> Result<ProcStat, MetricError> {
    let content = fs::read_to_string(PROC_STAT).map_err(|error| {
        MetricError::new(
            MetricErrorCode::Io,
            format!("could not read {PROC_STAT}: {error}"),
        )
    })?;

    parse_proc_stat_all(&content)
}

/// What the provider discovered about this machine at startup.
///
/// Topology and the hardware maximum frequencies are **static for the life of
/// the process**: cores do not change socket and `cpuinfo_max_freq` does not
/// move. Reading them once at construction keeps a refresh down to one
/// `/proc/stat` read plus one `scaling_cur_freq` read per requested processor,
/// instead of re-walking sysfs for numbers that cannot have changed.
#[derive(Debug)]
struct CpuInventory {
    topology: CpuTopology,
    /// Hardware maxima in hertz, for the processors that expose one.
    max_frequency_hz: BTreeMap<LogicalId, u64>,
}

impl CpuInventory {
    /// Enumerates the machine.
    ///
    /// Every step degrades independently. No online list still yields the
    /// processors `/proc/stat` reports; no topology files still yield usage
    /// and frequency; no `cpufreq` still yields usage.
    fn discover() -> Self {
        let online = Self::online_processors();
        let (physical_core_count, package_count) = cpu_sysfs::read_topology_counts(&online);

        let mut max_frequency_hz = BTreeMap::new();
        let mut processors = Vec::with_capacity(online.len());

        for id in online {
            let mut processor = LogicalProcessor::available(id);

            match cpu_sysfs::read_max_frequency(id) {
                Ok(hertz) => {
                    max_frequency_hz.insert(id, hertz);
                }
                Err(error) => {
                    processor =
                        processor.with_frequency_max(cpu_sysfs::frequency_availability(&error));
                }
            }

            // Probed once so the catalog can state up front whether this
            // processor has a readable current frequency. The value itself is
            // always re-read at sample time — it changes constantly.
            if let Err(error) = cpu_sysfs::read_current_frequency(id) {
                processor =
                    processor.with_frequency_current(cpu_sysfs::frequency_availability(&error));
            }

            processors.push(processor);
        }

        Self {
            topology: CpuTopology::new(processors, physical_core_count, package_count),
            max_frequency_hz,
        }
    }

    /// The logical processors to publish.
    ///
    /// `/sys/devices/system/cpu/online` is the kernel's authoritative answer.
    /// When it cannot be read — an unusual kernel, a restricted container —
    /// the `cpuN` lines of `/proc/stat` describe the same set, so PULSE falls
    /// back to them rather than reporting a machine with no processors.
    fn online_processors() -> Vec<LogicalId> {
        if let Ok(online) = cpu_sysfs::read_online_cpus() {
            if !online.is_empty() {
                return online;
            }
        }

        read_proc_stat()
            .map(|stat| stat.logical.keys().copied().collect())
            .unwrap_or_default()
    }
}

/// Publishes every CPU metric on Linux.
///
/// Requires no elevated privileges: `/proc/stat` and everything used under
/// `/sys/devices/system/cpu/` are world-readable.
#[derive(Debug)]
pub struct LinuxCpuProvider {
    id: ProviderId,
    inventory: CpuInventory,
    tracker: CpuUsageTracker,
}

impl LinuxCpuProvider {
    /// Builds the provider, enumerates the machine and captures a CPU
    /// baseline immediately.
    ///
    /// Priming here rather than on first request means the very first sample
    /// usually already has a delta to work with. If the read fails, the
    /// provider still works — the first request simply reports
    /// `temporarilyUnavailable` and primes then.
    pub fn new() -> Self {
        let tracker = CpuUsageTracker::new();

        if let Ok(stat) = read_proc_stat() {
            tracker.prime(&stat.to_snapshot());
        }

        Self {
            id: ProviderId::new(PROVIDER_ID).expect("provider id must be valid"),
            inventory: CpuInventory::discover(),
            tracker,
        }
    }

    /// The topology this provider published its catalog from.
    pub fn topology(&self) -> &CpuTopology {
        &self.inventory.topology
    }

    /// Answers one reference from an already-taken usage report.
    fn usage_sample(
        &self,
        reference: &MetricRef,
        usage: Option<CpuUsage>,
        read_error: Option<&MetricError>,
    ) -> MetricSample {
        match usage {
            Some(CpuUsage::Ready(percent)) => MetricSample::number(reference.clone(), percent),
            Some(CpuUsage::NeedsAnotherSample(reason)) => MetricSample::unavailable(
                reference.clone(),
                Availability::temporarily_unavailable(reason.reason()),
            ),
            None => MetricSample::unavailable(
                reference.clone(),
                match read_error {
                    Some(error) => availability_for(error.clone()),
                    // The read succeeded but said nothing about this
                    // processor: it went offline between the catalog being
                    // built and this request.
                    None => Availability::temporarily_unavailable(
                        "this logical processor is not currently reported by the kernel",
                    ),
                },
            ),
        }
    }

    /// Answers one `cpu.count.*` reference.
    fn count_sample(
        &self,
        reference: &MetricRef,
        count: Option<u32>,
        missing: &str,
    ) -> MetricSample {
        match count {
            Some(count) => MetricSample::number(reference.clone(), f64::from(count)),
            None => {
                MetricSample::unavailable(reference.clone(), Availability::not_detected(missing))
            }
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
        Ok(cpu::definitions(&self.id, &self.inventory.topology))
    }

    fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError> {
        // `/proc/stat` is read at most once per request, and only when a usage
        // metric was actually asked for. The tracker is then fed the *whole*
        // snapshot regardless of what was requested, so every processor's
        // baseline stays aligned with the same instant.
        let needs_usage = requested.iter().any(|reference| {
            matches!(
                reference.key.as_str(),
                cpu::USAGE_TOTAL | cpu::USAGE_LOGICAL
            )
        });

        let mut read_error: Option<MetricError> = None;
        let report: CpuUsageReport = if needs_usage {
            match read_proc_stat().and_then(|stat| self.tracker.update(&stat.to_snapshot())) {
                Ok(report) => report,
                Err(error) => {
                    read_error = Some(error);
                    CpuUsageReport::default()
                }
            }
        } else {
            CpuUsageReport::default()
        };

        let samples = requested
            .iter()
            .map(|reference| {
                let logical_id = LogicalId::from_source(&reference.source_id);

                match (reference.key.as_str(), logical_id) {
                    (cpu::USAGE_TOTAL, _) => {
                        self.usage_sample(reference, report.total, read_error.as_ref())
                    }
                    (cpu::USAGE_LOGICAL, Some(id)) => {
                        self.usage_sample(reference, report.logical(id), read_error.as_ref())
                    }

                    (cpu::COUNT_LOGICAL, _) => self.count_sample(
                        reference,
                        Some(self.inventory.topology.logical_count()),
                        "no logical processor could be enumerated",
                    ),
                    (cpu::COUNT_PHYSICAL, _) => self.count_sample(
                        reference,
                        self.inventory.topology.physical_core_count,
                        "this kernel exposes no CPU core topology",
                    ),
                    (cpu::COUNT_PACKAGE, _) => self.count_sample(
                        reference,
                        self.inventory.topology.package_count,
                        "this kernel exposes no CPU package topology",
                    ),

                    // Re-read every time: this is the number that moves.
                    (cpu::FREQUENCY_CURRENT, Some(id)) => {
                        match cpu_sysfs::read_current_frequency(id) {
                            Ok(hertz) => MetricSample::number(reference.clone(), hertz as f64),
                            Err(error) => MetricSample::unavailable(
                                reference.clone(),
                                cpu_sysfs::frequency_availability(&error),
                            ),
                        }
                    }

                    // Static: read once at startup, served from memory.
                    (cpu::FREQUENCY_MAX, Some(id)) => {
                        match self.inventory.max_frequency_hz.get(&id) {
                            Some(&hertz) => MetricSample::number(reference.clone(), hertz as f64),
                            None => MetricSample::unavailable(
                                reference.clone(),
                                Availability::unsupported(
                                    "this logical processor exposes no hardware maximum frequency",
                                ),
                            ),
                        }
                    }

                    // The engine only passes references this provider
                    // declared, so this is unreachable in practice; answering
                    // honestly beats an unwrap.
                    _ => MetricSample::unavailable(
                        reference.clone(),
                        Availability::not_registered(format!(
                            "'{reference}' is not a metric this provider knows how to read"
                        )),
                    ),
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

    fn logical(ordinal: u32) -> LogicalId {
        LogicalId::new(ordinal)
    }

    // --- aggregate parsing, unchanged from Phase 2 ------------------------

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
    fn guest_time_is_not_counted_twice_per_logical_processor_either() {
        // The same trap, on the lines Phase 3 added. A guest-heavy VM would
        // otherwise show every logical processor as idle.
        let parsed = parse_proc_stat_all("cpu0 100 100 100 100 0 0 0 0 50 50\n").expect("valid");
        let line = parsed.logical[&logical(0)];

        assert_eq!(line.total(), 400);
        assert_eq!(line.busy(), 300);
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

        let all = parse_proc_stat_all("cpu0\t100   200\t\t300  400\n").expect("valid");
        assert_eq!(all.logical[&logical(0)].user, 100);
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
        for content in ["", "intr 1 2 3\nctxt 4\n"] {
            let error = parse_proc_stat(content).expect_err("must be rejected");
            assert_eq!(error.code, MetricErrorCode::Parse);
        }
    }

    #[test]
    fn rejects_a_truncated_aggregate_line() {
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

    // --- per-logical parsing ---------------------------------------------

    #[test]
    fn parses_the_aggregate_and_every_per_processor_line_together() {
        let parsed = parse_proc_stat_all(REAL_PROC_STAT).expect("valid");

        assert_eq!(parsed.aggregate.expect("aggregate line").user, 123_456);
        assert_eq!(parsed.logical.len(), 2);
        assert_eq!(parsed.logical[&logical(0)].user, 30_000);
        assert_eq!(parsed.logical[&logical(1)].user, 30_500);
    }

    #[test]
    fn the_kernels_cpu_number_becomes_the_pulse_ordinal() {
        // cpu7 -> cpu:logical-7, so PULSE agrees with htop and taskset.
        let parsed = parse_proc_stat_all("cpu7 1 2 3 4\n").expect("valid");

        assert!(parsed.logical.contains_key(&logical(7)));
        assert_eq!(
            cpu::usage_logical_ref(logical(7)).source_id.as_str(),
            "cpu:logical-7"
        );
    }

    #[test]
    fn handles_non_contiguous_cpu_numbers() {
        // A machine with CPUs 4-7 offlined.
        let content = "\
cpu  400 0 0 400
cpu0 100 0 0 100
cpu1 100 0 0 100
cpu2 100 0 0 100
cpu3 100 0 0 100
cpu8 100 0 0 100
cpu9 100 0 0 100
cpu10 100 0 0 100
cpu11 100 0 0 100
";
        let parsed = parse_proc_stat_all(content).expect("valid");

        let ordinals: Vec<u32> = parsed.logical.keys().map(|id| id.get()).collect();
        assert_eq!(ordinals, [0, 1, 2, 3, 8, 9, 10, 11]);
        assert!(!parsed.logical.contains_key(&logical(4)));
    }

    #[test]
    fn logical_lines_are_ordered_numerically_not_lexicographically() {
        let content = "cpu1 1 1 1 1\ncpu10 1 1 1 1\ncpu2 1 1 1 1\ncpu11 1 1 1 1\ncpu0 1 1 1 1\n";
        let parsed = parse_proc_stat_all(content).expect("valid");

        let ordinals: Vec<u32> = parsed.logical.keys().map(|id| id.get()).collect();
        assert_eq!(ordinals, [0, 1, 2, 10, 11]);
    }

    #[test]
    fn a_machine_with_a_hundred_and_twenty_eight_processors_parses() {
        let mut content = String::from("cpu 100 0 0 100\n");
        for ordinal in 0..128 {
            content.push_str(&format!("cpu{ordinal} 1 0 0 1\n"));
        }

        let parsed = parse_proc_stat_all(&content).expect("valid");
        assert_eq!(parsed.logical.len(), 128);
        assert!(parsed.logical.contains_key(&logical(127)));
    }

    #[test]
    fn a_malformed_per_processor_line_costs_only_that_processor() {
        // /proc/stat is generated on the fly and is not read atomically; a
        // truncated cpuN line must not blank out the whole machine.
        let content = "cpu 400 0 0 400\ncpu0 100 0 0 100\ncpu1 oops\ncpu2 100 0 0 100\n";
        let parsed = parse_proc_stat_all(content).expect("valid");

        assert!(parsed.aggregate.is_some());
        assert_eq!(parsed.logical.len(), 2);
        assert!(parsed.logical.contains_key(&logical(0)));
        assert!(!parsed.logical.contains_key(&logical(1)));
        assert!(parsed.logical.contains_key(&logical(2)));
    }

    #[test]
    fn per_processor_lines_survive_a_missing_aggregate() {
        let parsed = parse_proc_stat_all("cpu0 1 2 3 4\ncpu1 1 2 3 4\n").expect("valid");

        assert!(parsed.aggregate.is_none());
        assert_eq!(parsed.logical.len(), 2);
        // But the aggregate-only entry point still reports the absence.
        assert!(parse_proc_stat("cpu0 1 2 3 4\n").is_err());
    }

    #[test]
    fn lines_that_only_look_like_cpu_lines_are_ignored() {
        let content = "cpu 100 0 0 100\ncpufreq 1 2 3 4\ncpu_x 1 2 3 4\ncpuidle 1 2 3 4\n";
        let parsed = parse_proc_stat_all(content).expect("valid");

        assert!(parsed.aggregate.is_some());
        assert!(
            parsed.logical.is_empty(),
            "only 'cpu<number>' names a logical processor"
        );
    }

    #[test]
    fn blank_lines_and_trailing_content_are_harmless() {
        let parsed = parse_proc_stat_all("\ncpu 100 0 0 100\n\ncpu0 50 0 0 50\n\n").expect("valid");

        assert!(parsed.aggregate.is_some());
        assert_eq!(parsed.logical.len(), 1);
    }

    // --- snapshot conversion ---------------------------------------------

    #[test]
    fn the_snapshot_applies_one_formula_to_every_processor() {
        let parsed = parse_proc_stat_all(
            "cpu 200 0 200 1400 200 0 0 0\ncpu0 100 0 100 700 100 0 0 0\ncpu1 100 0 100 700 100 0 0 0\n",
        )
        .expect("valid");
        let snapshot = parsed.to_snapshot();

        // Aggregate: busy 400, total 2000. Each processor: busy 200, total 1000.
        assert_eq!(snapshot.total, Some(CpuCounters::new(400, 2000)));
        assert_eq!(snapshot.logical[&logical(0)], CpuCounters::new(200, 1000));
        assert_eq!(snapshot.logical[&logical(1)], CpuCounters::new(200, 1000));

        // The per-processor figures reconcile with the aggregate, which is
        // only true because the same definition of "busy" is used for both.
        let summed_busy: u64 = snapshot.logical.values().map(|c| c.busy).sum();
        assert_eq!(summed_busy, snapshot.total.expect("total").busy);
    }

    #[test]
    fn an_aggregate_only_snapshot_is_valid() {
        let snapshot = parse_proc_stat_all("cpu 100 0 100 800\n")
            .expect("valid")
            .to_snapshot();

        assert!(snapshot.total.is_some());
        assert!(snapshot.logical.is_empty());
        assert!(!snapshot.is_empty());
    }

    // --- tracker integration ---------------------------------------------

    #[test]
    fn a_progressing_counter_produces_a_usage_between_zero_and_one_hundred() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(
            &parse_proc_stat_all("cpu 100 0 100 700 100 0 0 0\ncpu0 100 0 100 700 100 0 0 0\n")
                .expect("valid")
                .to_snapshot(),
        );

        // 100 more busy jiffies, 300 more idle: 100 busy out of 400 elapsed.
        let report = tracker
            .update(
                &parse_proc_stat_all(
                    "cpu 200 0 100 1000 100 0 0 0\ncpu0 200 0 100 1000 100 0 0 0\n",
                )
                .expect("valid")
                .to_snapshot(),
            )
            .expect("no error");

        assert_eq!(report.total, Some(CpuUsage::Ready(25.0)));
        assert_eq!(report.logical(logical(0)), Some(CpuUsage::Ready(25.0)));
    }

    #[test]
    fn an_unchanged_counter_waits_instead_of_reporting_idle() {
        let content = "cpu 100 0 100 700 100 0 0 0\ncpu0 100 0 100 700 100 0 0 0\n";
        let tracker = CpuUsageTracker::new();
        let snapshot = parse_proc_stat_all(content).expect("valid").to_snapshot();
        tracker.prime(&snapshot);

        let report = tracker.update(&snapshot).expect("no error");

        assert_eq!(
            report.total,
            Some(CpuUsage::NeedsAnotherSample(
                NeedsAnotherSample::NoElapsedTime
            ))
        );
        assert_eq!(
            report.logical(logical(0)),
            Some(CpuUsage::NeedsAnotherSample(
                NeedsAnotherSample::NoElapsedTime
            ))
        );
    }

    #[test]
    fn a_rewound_counter_resets_the_baseline() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(
            &parse_proc_stat_all("cpu 1000 0 1000 7000 0 0 0 0\n")
                .expect("valid")
                .to_snapshot(),
        );

        let report = tracker
            .update(
                &parse_proc_stat_all("cpu 100 0 100 700 0 0 0 0\n")
                    .expect("valid")
                    .to_snapshot(),
            )
            .expect("no error");

        assert_eq!(
            report.total,
            Some(CpuUsage::NeedsAnotherSample(
                NeedsAnotherSample::CountersWentBackwards
            ))
        );
    }

    #[test]
    fn a_processor_appearing_between_two_reads_is_handled() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(
            &parse_proc_stat_all("cpu 100 0 0 100\ncpu0 100 0 0 100\n")
                .expect("valid")
                .to_snapshot(),
        );

        // cpu1 was brought back online.
        let report = tracker
            .update(
                &parse_proc_stat_all("cpu 300 0 0 300\ncpu0 200 0 0 200\ncpu1 100 0 0 100\n")
                    .expect("valid")
                    .to_snapshot(),
            )
            .expect("no error");

        assert!(matches!(
            report.logical(logical(0)),
            Some(CpuUsage::Ready(_))
        ));
        assert_eq!(
            report.logical(logical(1)),
            Some(CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoBaseline)),
            "a newly online processor is never reported as 0%"
        );
    }

    #[test]
    fn a_processor_disappearing_between_two_reads_is_handled() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(
            &parse_proc_stat_all("cpu 200 0 0 200\ncpu0 100 0 0 100\ncpu1 100 0 0 100\n")
                .expect("valid")
                .to_snapshot(),
        );

        let report = tracker
            .update(
                &parse_proc_stat_all("cpu 400 0 0 400\ncpu0 200 0 0 200\n")
                    .expect("valid")
                    .to_snapshot(),
            )
            .expect("no error");

        assert!(report.logical(logical(1)).is_none());
        assert!(matches!(
            report.logical(logical(0)),
            Some(CpuUsage::Ready(_))
        ));
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
    // read the real /proc and /sys, which only exist here.

    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_proc_stat_on_this_host_parses() {
        let stat = read_proc_stat().expect("/proc/stat must be readable without privileges");
        let snapshot = stat.to_snapshot();

        let total = snapshot.total.expect("an aggregate line must exist");
        assert!(total.total > 0);
        assert!(total.is_consistent());

        assert!(
            !snapshot.logical.is_empty(),
            "a running machine has at least one logical processor"
        );
        for (id, counters) in &snapshot.logical {
            assert!(counters.is_consistent(), "cpu{id} reported busy > total");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_provider_declares_four_plus_three_n_metrics_on_this_host() {
        let provider = LinuxCpuProvider::new();
        let definitions = provider.describe().expect("describe");
        let logical_count = provider.topology().logical_count() as usize;

        assert!(logical_count > 0);
        assert_eq!(definitions.len(), 4 + 3 * logical_count);
        assert_eq!(provider.id().as_str(), "linux.cpu");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn this_hosts_topology_is_internally_consistent() {
        let provider = LinuxCpuProvider::new();
        let topology = provider.topology();

        let logical = topology.logical_count();
        assert!(logical > 0);

        if let Some(cores) = topology.physical_core_count {
            assert!(cores >= 1);
            assert!(
                cores <= logical,
                "physical cores ({cores}) cannot exceed logical processors ({logical})"
            );
        }
        if let Some(packages) = topology.package_count {
            assert!(packages >= 1);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_provider_produces_plausible_samples_on_this_host() {
        let provider = LinuxCpuProvider::new();
        let topology_snapshot = provider.topology().clone();
        let first = topology_snapshot
            .logical()
            .first()
            .expect("at least one logical processor")
            .id;

        let requested = vec![
            cpu::usage_total_ref(),
            cpu::count_logical_ref(),
            cpu::usage_logical_ref(first),
            cpu::frequency_current_ref(first),
            cpu::frequency_max_ref(first),
        ];

        // Burn a little CPU so the jiffy counters advance between the
        // construction baseline and this request.
        let mut spin = 0_u64;
        for i in 0..8_000_000_u64 {
            spin = spin.wrapping_add(i);
        }
        assert!(spin > 0);

        let samples = provider.sample(&requested).expect("sample");
        assert_eq!(samples.len(), requested.len());

        for (sample, reference) in samples.iter().zip(requested.iter()) {
            assert_eq!(&sample.metric, reference);

            // Never a fabricated value where none could be read.
            if !sample.availability.is_available() {
                assert!(sample.value.is_none(), "{reference} invented a value");
                continue;
            }

            let value = sample
                .value
                .as_ref()
                .and_then(|value| value.as_number())
                .unwrap_or_else(|| panic!("{reference} is available but carries no number"));

            match reference.key.as_str() {
                cpu::USAGE_TOTAL | cpu::USAGE_LOGICAL => {
                    assert!((0.0..=100.0).contains(&value), "{reference} = {value}");
                }
                cpu::COUNT_LOGICAL => {
                    assert_eq!(value, f64::from(topology_snapshot.logical_count()));
                }
                cpu::FREQUENCY_CURRENT | cpu::FREQUENCY_MAX => {
                    // Hertz, never kHz: a CPU running at 3.2 GHz must read as
                    // 3_200_000_000, not 3_200_000.
                    assert!(
                        (100_000_000.0..=20_000_000_000.0).contains(&value),
                        "{reference} = {value} Hz is not a plausible CPU frequency"
                    );
                }
                other => panic!("unexpected key {other}"),
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_frequency_that_cannot_be_read_never_costs_a_usage_metric() {
        // Ask for every metric of every processor at once and check that the
        // request is answered in full, whatever sysfs does or does not expose.
        let provider = LinuxCpuProvider::new();
        let topology = provider.topology().clone();

        let requested: Vec<MetricRef> = provider
            .describe()
            .expect("describe")
            .into_iter()
            .map(|definition| definition.metric)
            .collect();

        let samples = provider.sample(&requested).expect("sample");
        assert_eq!(samples.len(), requested.len());

        let usable = |key: &str| {
            samples
                .iter()
                .filter(|sample| {
                    sample.metric.key.as_str() == key
                        && (sample.availability.is_available()
                            || sample.availability.is_transient())
                })
                .count()
        };

        // Usage is readable for every processor regardless of cpufreq.
        assert_eq!(
            usable(cpu::USAGE_LOGICAL),
            topology.logical_count() as usize,
            "a cpufreq problem must never cost a usage metric"
        );
    }
}
