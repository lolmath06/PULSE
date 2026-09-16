//! Windows CPU metrics.
//!
//! Three native APIs, each doing one job, all unprivileged:
//!
//! | Metric | API | Module |
//! |---|---|---|
//! | `cpu.usage.total` | `GetSystemTimes` | this file |
//! | `cpu.usage.logical` | `NtQuerySystemInformationEx(SystemProcessorPerformanceInformation)` | [`super::cpu_perf`] |
//! | `cpu.frequency.*` | `CallNtPowerInformation(ProcessorInformation)` | [`super::cpu_freq`] |
//! | `cpu.count.*` | `GetLogicalProcessorInformationEx` | [`super::cpu_topology`] |
//!
//! Each module argues its own choice of API. They are kept apart so that one
//! failing degrades one metric family: a machine where the frequency call is
//! unavailable still reports usage and topology, and vice versa.
//!
//! Three of the four are guaranteed Win32 entry points. The fourth —
//! `NtQuerySystemInformationEx`, which serves `cpu.usage.logical` — is an
//! **optional capability resolved at runtime** (see [`super::ntdll`]), because
//! importing it at load time would let its absence stop PULSE from starting
//! rather than cost one metric family.
//!
//! The arithmetic is deliberately separated from the FFI. Everything outside
//! the `imp` modules is pure and compiles on every platform, so the Windows CPU
//! semantics are unit-tested from Fedora; only the thin `unsafe` wrappers need
//! a Windows host.

use std::collections::BTreeMap;

use crate::metrics::model::Availability;
use crate::metrics::wellknown::cpu::{CpuCounters, CpuTopology, LogicalId, LogicalProcessor};

use super::cpu_freq::ProcessorFrequencies;
use super::cpu_topology::ProcessorMap;

/// Identifier of the Windows CPU provider.
///
/// **One provider owns every CPU metric on the machine**, not one per
/// processor: they all share one topology, one baseline, and one call per
/// sample.
pub const PROVIDER_ID: &str = "windows.cpu";

/// Combines the two halves of a `FILETIME` into one 64-bit count.
///
/// A `FILETIME` is a 64-bit value in 100-nanosecond units, split across two
/// 32-bit fields because the struct predates guaranteed 64-bit alignment.
/// Microsoft's documentation is explicit that it must not be cast directly to
/// a `u64`; the halves are recombined instead.
pub const fn filetime_to_u64(filetime_low: u32, filetime_high: u32) -> u64 {
    ((filetime_high as u64) << 32) | (filetime_low as u64)
}

/// Derives platform-neutral CPU counters from Windows' three time totals.
///
/// **Windows counts idle time inside `kernel`**, which is the trap here: the
/// Linux formula would report a completely idle machine as heavily busy.
///
/// ```text
/// total = kernel + user      (kernel already contains idle)
/// busy  = total - idle
/// ```
///
/// Returns `None` when the values are inconsistent — idle exceeding the total,
/// or an arithmetic overflow — so a nonsensical reading becomes an error
/// rather than a percentage above 100.
///
/// Identical to the per-processor conversion in [`super::cpu_perf`], because
/// the counters mean the same thing at both scopes. That is what makes
/// `cpu.usage.total` and `cpu.usage.logical` comparable.
pub fn counters_from_system_times(idle: u64, kernel: u64, user: u64) -> Option<CpuCounters> {
    let total = kernel.checked_add(user)?;
    let busy = total.checked_sub(idle)?;

    Some(CpuCounters::new(busy, total))
}

/// Builds the platform-neutral topology from what each API reported.
///
/// Pure, and free of every Windows type, so the degradation rules it encodes
/// are unit-tested on Fedora — including the case this function exists for:
/// **the per-processor counter capability being absent**.
///
/// The three availabilities move independently:
///
/// - `logical_usage` applies to every logical processor at once, because the
///   one API behind it either resolved or did not;
/// - each processor's two frequency availabilities come from whether that
///   processor appeared in the frequency reply, so one silent processor costs
///   only its own metrics.
///
/// Every logical processor is described whatever those say, so the catalog
/// keeps publishing `cpu.usage.logical@cpu:logical-N` for each of them — with
/// an honest availability instead of being silently dropped. A dashboard
/// therefore keeps exactly the same references on a machine where the
/// capability is missing.
pub fn describe_topology(
    map: &ProcessorMap,
    logical_usage: &Availability,
    frequencies: &BTreeMap<LogicalId, ProcessorFrequencies>,
) -> CpuTopology {
    let frequency_missing =
        Availability::unsupported("this system reports no frequency for this logical processor");

    let processors = (0..map.logical_count())
        .map(|ordinal| {
            let id = LogicalId::new(ordinal);
            let reported = frequencies.get(&id).copied().unwrap_or_default();

            let mut processor = LogicalProcessor::available(id);
            processor.usage = logical_usage.clone();

            if reported.current_hz.is_none() {
                processor = processor.with_frequency_current(frequency_missing.clone());
            }
            if reported.max_hz.is_none() {
                processor = processor.with_frequency_max(frequency_missing.clone());
            }

            processor
        })
        .collect();

    CpuTopology::new(processors, map.physical_core_count(), map.package_count())
}

/// The parts that call into the Windows API.
#[cfg(target_os = "windows")]
mod imp {
    use std::sync::Arc;

    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::GetSystemTimes;

    use crate::metrics::model::{
        Availability, MetricDefinition, MetricError, MetricErrorCode, MetricRef, MetricSample,
        ProviderId,
    };
    use crate::metrics::providers::MetricProvider;
    use crate::metrics::wellknown::availability_for;
    use crate::metrics::wellknown::cpu::{
        self, CpuCounters, CpuSnapshot, CpuTopology, CpuUsage, CpuUsageReport, CpuUsageTracker,
        LogicalId,
    };

    use super::super::cpu_freq::{self, ProcessorFrequencies};
    use super::super::cpu_perf::{self, imp::NtProcessorTimes};
    use super::super::cpu_topology::{self, ProcessorMap};
    use super::{counters_from_system_times, describe_topology, filetime_to_u64, PROVIDER_ID};

    /// Calls `GetSystemTimes` and converts the result.
    ///
    /// # Safety
    ///
    /// The single `unsafe` call writes three `FILETIME` values through pointers
    /// to local variables alive for the whole call. `GetSystemTimes` accepts
    /// exactly these three out-parameters and writes nothing else. The return
    /// value is checked before the outputs are read, so a failed call never
    /// yields uninitialised data.
    fn read_total_counters() -> Result<CpuCounters, MetricError> {
        let mut idle = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut kernel = idle;
        let mut user = idle;

        // SAFETY: see the function docs. All three pointers reference live
        // locals, and the result is checked before the values are read.
        let ok = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };

        if ok == 0 {
            return Err(MetricError::new(
                MetricErrorCode::Io,
                format!(
                    "GetSystemTimes failed ({})",
                    std::io::Error::last_os_error()
                ),
            ));
        }

        let idle = filetime_to_u64(idle.dwLowDateTime, idle.dwHighDateTime);
        let kernel = filetime_to_u64(kernel.dwLowDateTime, kernel.dwHighDateTime);
        let user = filetime_to_u64(user.dwLowDateTime, user.dwHighDateTime);

        counters_from_system_times(idle, kernel, user).ok_or_else(|| {
            MetricError::new(
                MetricErrorCode::Parse,
                format!(
                    "inconsistent CPU times from GetSystemTimes \
                     (idle={idle}, kernel={kernel}, user={user})"
                ),
            )
            .with_recoverable(true)
        })
    }

    /// Reads every CPU counter available, in one pass.
    ///
    /// The two halves are independent: a failing per-processor call still
    /// yields the aggregate, which is the whole reason `cpu.usage.total` uses
    /// the documented `GetSystemTimes` rather than summing the `Nt` array.
    fn read_snapshot(
        map: &ProcessorMap,
        source: Option<&NtProcessorTimes>,
    ) -> (CpuSnapshot, Option<MetricError>) {
        let mut snapshot = CpuSnapshot::new();
        let mut error = None;

        // Always attempted, and on its own documented API: this is what keeps
        // `cpu.usage.total` alive when the optional capability below is not.
        match read_total_counters() {
            Ok(total) => snapshot = snapshot.with_total(total),
            Err(failure) => error = Some(failure),
        }

        // Only when the capability resolved at startup. One call per processor
        // group, never one per processor — and none at all when absent.
        if let Some(source) = source {
            match cpu_perf::read_all_groups(source, map) {
                Ok(groups) => {
                    for (id, counters) in cpu_perf::counters_by_ordinal(&groups, map) {
                        snapshot.insert_logical(id, counters);
                    }
                }
                Err(failure) => error = error.or(Some(failure)),
            }
        }

        (snapshot, error)
    }

    /// What the provider discovered about this machine at startup.
    #[derive(Debug)]
    struct CpuInventory {
        /// The `(group, index)` ↔ ordinal mapping, shared by every CPU metric.
        map: ProcessorMap,
        topology: CpuTopology,
        /// The per-processor counter capability, when this Windows offers it.
        ///
        /// Resolved **once**, here, and reused for every sample: there is no
        /// `GetProcAddress` per refresh, none per processor, and no mutable
        /// global. `None` means the capability is absent, which costs
        /// `cpu.usage.logical` and nothing else.
        usage_source: Option<NtProcessorTimes>,
    }

    impl CpuInventory {
        /// Enumerates the machine, probing each API so the catalog can state
        /// up front what this system supports.
        ///
        /// Every step degrades independently. No topology means no processors
        /// to publish per-processor metrics for, but `cpu.usage.total` still
        /// works; no frequency API costs only the frequency metrics.
        fn discover() -> Self {
            let map = cpu_topology::imp::read_processor_map().unwrap_or_default();

            // The optional capability. Resolving it is also the probe, so a
            // success here means the entry point exists *and* answers.
            // A failure is one availability, never a panic and never fatal.
            let (usage_source, logical_usage) = match NtProcessorTimes::resolve() {
                Ok(source) => (Some(source), Availability::Available),
                Err(error) => {
                    eprintln!(
                        "PULSE: per-processor CPU counters unavailable on this system: {}",
                        error.message
                    );
                    (None, availability_for(error))
                }
            };

            // One call, at startup, to learn which processors report a
            // frequency at all. The values themselves are re-read on every
            // sample. Independent of the above: a missing counter capability
            // must not cost the frequencies.
            let frequencies = cpu_freq::imp::read_all(&map)
                .map(|readings| cpu_freq::frequencies_by_ordinal(&readings, &map))
                .unwrap_or_default();

            Self {
                topology: describe_topology(&map, &logical_usage, &frequencies),
                map,
                usage_source,
            }
        }
    }

    /// Publishes every CPU metric on Windows.
    ///
    /// Requires no administrator rights: all four APIs are available to any
    /// process.
    #[derive(Debug)]
    pub struct WindowsCpuProvider {
        id: ProviderId,
        inventory: CpuInventory,
        tracker: CpuUsageTracker,
    }

    impl WindowsCpuProvider {
        /// Builds the provider, enumerates the machine and captures a CPU
        /// baseline immediately.
        ///
        /// Same reasoning as on Linux: priming at construction avoids a
        /// blocking sleep inside the first request.
        pub fn new() -> Self {
            let inventory = CpuInventory::discover();
            let tracker = CpuUsageTracker::new();

            let (snapshot, _) = read_snapshot(&inventory.map, inventory.usage_source.as_ref());
            if !snapshot.is_empty() {
                tracker.prime(&snapshot);
            }

            Self {
                id: ProviderId::new(PROVIDER_ID).expect("provider id must be valid"),
                inventory,
                tracker,
            }
        }

        /// The topology this provider published its catalog from.
        pub fn topology(&self) -> &CpuTopology {
            &self.inventory.topology
        }

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
                        None => Availability::temporarily_unavailable(
                            "this logical processor was not reported by the last counter read",
                        ),
                    },
                ),
            }
        }

        fn count_sample(
            &self,
            reference: &MetricRef,
            count: Option<u32>,
            missing: &str,
        ) -> MetricSample {
            match count {
                Some(count) => MetricSample::number(reference.clone(), f64::from(count)),
                None => MetricSample::unavailable(
                    reference.clone(),
                    Availability::not_detected(missing),
                ),
            }
        }

        fn frequency_sample(
            &self,
            reference: &MetricRef,
            hertz: Option<u64>,
            error: Option<&MetricError>,
        ) -> MetricSample {
            match hertz {
                Some(hertz) => MetricSample::number(reference.clone(), hertz as f64),
                None => MetricSample::unavailable(
                    reference.clone(),
                    match error {
                        Some(error) => availability_for(error.clone()),
                        None => Availability::unsupported(
                            "this system reports no frequency for this logical processor",
                        ),
                    },
                ),
            }
        }
    }

    impl Default for WindowsCpuProvider {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MetricProvider for WindowsCpuProvider {
        fn id(&self) -> &ProviderId {
            &self.id
        }

        fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError> {
            Ok(cpu::definitions(&self.id, &self.inventory.topology))
        }

        fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError> {
            // Each API is called at most once per request, and only when a
            // metric it serves was actually asked for.
            let wants = |keys: &[&str]| {
                requested
                    .iter()
                    .any(|reference| keys.contains(&reference.key.as_str()))
            };

            let mut usage_error = None;
            let report: CpuUsageReport = if wants(&[cpu::USAGE_TOTAL, cpu::USAGE_LOGICAL]) {
                let (snapshot, error) =
                    read_snapshot(&self.inventory.map, self.inventory.usage_source.as_ref());
                usage_error = error;

                match self.tracker.update(&snapshot) {
                    Ok(report) => report,
                    Err(error) => {
                        usage_error = Some(error);
                        CpuUsageReport::default()
                    }
                }
            } else {
                CpuUsageReport::default()
            };

            let mut frequency_error = None;
            let frequencies = if wants(&[cpu::FREQUENCY_CURRENT, cpu::FREQUENCY_MAX]) {
                // One call returns the whole array; asking per processor would
                // be N calls for the same data.
                match cpu_freq::imp::read_all(&self.inventory.map) {
                    Ok(readings) => {
                        cpu_freq::frequencies_by_ordinal(&readings, &self.inventory.map)
                    }
                    Err(error) => {
                        frequency_error = Some(error);
                        Default::default()
                    }
                }
            } else {
                Default::default()
            };

            let frequency_of = |id: LogicalId| -> ProcessorFrequencies {
                frequencies.get(&id).copied().unwrap_or_default()
            };

            let samples = requested
                .iter()
                .map(|reference| {
                    let logical_id = LogicalId::from_source(&reference.source_id);

                    match (reference.key.as_str(), logical_id) {
                        (cpu::USAGE_TOTAL, _) => {
                            self.usage_sample(reference, report.total, usage_error.as_ref())
                        }
                        (cpu::USAGE_LOGICAL, Some(id)) => {
                            self.usage_sample(reference, report.logical(id), usage_error.as_ref())
                        }

                        (cpu::COUNT_LOGICAL, _) => self.count_sample(
                            reference,
                            Some(self.inventory.topology.logical_count()),
                            "no logical processor could be enumerated",
                        ),
                        (cpu::COUNT_PHYSICAL, _) => self.count_sample(
                            reference,
                            self.inventory.topology.physical_core_count,
                            "this system exposes no CPU core topology",
                        ),
                        (cpu::COUNT_PACKAGE, _) => self.count_sample(
                            reference,
                            self.inventory.topology.package_count,
                            "this system exposes no CPU package topology",
                        ),

                        (cpu::FREQUENCY_CURRENT, Some(id)) => self.frequency_sample(
                            reference,
                            frequency_of(id).current_hz,
                            frequency_error.as_ref(),
                        ),
                        (cpu::FREQUENCY_MAX, Some(id)) => self.frequency_sample(
                            reference,
                            frequency_of(id).max_hz,
                            frequency_error.as_ref(),
                        ),

                        // The engine only passes references this provider
                        // declared, so this is unreachable in practice;
                        // answering honestly beats an unwrap.
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

    /// Builds the Windows CPU provider.
    pub fn provider() -> Arc<dyn MetricProvider> {
        Arc::new(WindowsCpuProvider::new())
    }
}

#[cfg(target_os = "windows")]
pub use imp::{provider, WindowsCpuProvider};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::model::ProviderId;
    use crate::metrics::wellknown::cpu::{
        self, CpuSnapshot, CpuUsage, CpuUsageTracker, NeedsAnotherSample,
    };

    use super::super::cpu_topology::{GroupMask, ProcessorRelation};
    use super::super::ntdll;

    /// Four logical processors on two SMT cores in one package.
    fn map_with_smt() -> ProcessorMap {
        ProcessorMap::from_relations(&[
            ProcessorRelation::core(vec![GroupMask::new(0, 0b0011)]),
            ProcessorRelation::core(vec![GroupMask::new(0, 0b1100)]),
            ProcessorRelation::package(vec![GroupMask::new(0, 0b1111)]),
        ])
    }

    /// Frequencies as a working `CallNtPowerInformation` would report them.
    fn working_frequencies() -> BTreeMap<LogicalId, ProcessorFrequencies> {
        (0..4)
            .map(|ordinal| {
                (
                    LogicalId::new(ordinal),
                    ProcessorFrequencies {
                        current_hz: Some(3_200_000_000),
                        max_hz: Some(4_800_000_000),
                    },
                )
            })
            .collect()
    }

    /// The availability the provider derives when the optional `ntdll` entry
    /// point cannot be resolved.
    fn capability_absent() -> Availability {
        crate::metrics::wellknown::availability_for(ntdll::symbol_unavailable_error())
    }

    fn definitions_for(topology: &CpuTopology) -> Vec<crate::metrics::model::MetricDefinition> {
        cpu::definitions(&ProviderId::new(PROVIDER_ID).expect("valid"), topology)
    }

    fn availability_of(
        definitions: &[crate::metrics::model::MetricDefinition],
        reference: &crate::metrics::model::MetricRef,
    ) -> Availability {
        definitions
            .iter()
            .find(|definition| &definition.metric == reference)
            .unwrap_or_else(|| panic!("{reference} must be declared"))
            .availability
            .clone()
    }

    // --- the per-processor capability being absent ------------------------
    //
    // These are the point of the dynamic resolution: on a Windows that does
    // not export `NtQuerySystemInformationEx`, exactly one metric family goes
    // unavailable and everything else keeps working.

    #[test]
    fn a_missing_capability_costs_only_the_per_processor_usage() {
        let map = map_with_smt();
        let topology = describe_topology(&map, &capability_absent(), &working_frequencies());
        let definitions = definitions_for(&topology);

        for ordinal in 0..4 {
            let id = LogicalId::new(ordinal);

            // The one casualty.
            assert_eq!(
                availability_of(&definitions, &cpu::usage_logical_ref(id)).status_str(),
                "unsupported",
                "CPU {ordinal} per-processor usage should be unavailable"
            );

            // Frequencies come from a different API and are untouched.
            assert!(
                availability_of(&definitions, &cpu::frequency_current_ref(id)).is_available(),
                "CPU {ordinal} current frequency must survive"
            );
            assert!(
                availability_of(&definitions, &cpu::frequency_max_ref(id)).is_available(),
                "CPU {ordinal} maximum frequency must survive"
            );
        }
    }

    #[test]
    fn a_missing_capability_leaves_aggregate_usage_declarable() {
        // `cpu.usage.total` comes from GetSystemTimes, which is a guaranteed
        // kernel32 entry point and entirely independent of the ntdll one.
        let topology = describe_topology(&map_with_smt(), &capability_absent(), &BTreeMap::new());

        assert!(
            availability_of(&definitions_for(&topology), &cpu::usage_total_ref()).is_available(),
            "total CPU usage must not depend on the optional capability"
        );
    }

    #[test]
    fn a_missing_capability_leaves_topology_declarable() {
        let map = map_with_smt();
        let topology = describe_topology(&map, &capability_absent(), &BTreeMap::new());
        let definitions = definitions_for(&topology);

        for reference in [
            cpu::count_logical_ref(),
            cpu::count_physical_ref(),
            cpu::count_package_ref(),
        ] {
            assert!(
                availability_of(&definitions, &reference).is_available(),
                "{reference} must survive a missing per-processor capability"
            );
        }

        // And the counts are still the real ones.
        assert_eq!(topology.logical_count(), 4);
        assert_eq!(topology.physical_core_count, Some(2));
        assert_eq!(topology.package_count, Some(1));
    }

    #[test]
    fn a_missing_capability_leaves_frequencies_declarable() {
        let topology = describe_topology(
            &map_with_smt(),
            &capability_absent(),
            &working_frequencies(),
        );
        let definitions = definitions_for(&topology);

        let available = definitions
            .iter()
            .filter(|definition| {
                matches!(
                    definition.metric.key.as_str(),
                    cpu::FREQUENCY_CURRENT | cpu::FREQUENCY_MAX
                ) && definition.availability.is_available()
            })
            .count();

        assert_eq!(available, 8, "both frequencies of all four processors");
    }

    #[test]
    fn the_catalog_keeps_exactly_the_same_references_either_way() {
        // The contract that lets a saved dashboard survive moving to a machine
        // without the capability: the metrics are still there, just honest
        // about being unreadable.
        let map = map_with_smt();
        let frequencies = working_frequencies();

        let healthy = describe_topology(&map, &Availability::Available, &frequencies);
        let degraded = describe_topology(&map, &capability_absent(), &frequencies);

        let references = |topology: &CpuTopology| -> Vec<String> {
            definitions_for(topology)
                .iter()
                .map(|definition| definition.metric.to_string())
                .collect()
        };

        assert_eq!(references(&healthy), references(&degraded));
        assert_eq!(
            references(&healthy).len(),
            4 + 3 * 4,
            "4 machine-wide metrics plus three per logical processor"
        );
    }

    #[test]
    fn a_missing_capability_never_fabricates_a_zero_percent() {
        // The whole availability model in one assertion: an unreadable metric
        // carries no value at all, so no widget can render it as an idle core.
        let topology = describe_topology(
            &map_with_smt(),
            &capability_absent(),
            &working_frequencies(),
        );

        for definition in definitions_for(&topology) {
            if definition.metric.key.as_str() != cpu::USAGE_LOGICAL {
                continue;
            }

            assert!(!definition.availability.is_available());
            // Not transient either: the UI must not suggest waiting for a
            // capability this Windows does not have.
            assert!(!definition.availability.is_transient());
        }
    }

    #[test]
    fn one_provider_still_owns_every_cpu_metric_when_the_capability_is_absent() {
        // Provider count stays conceptually 2 (one CPU, one memory): a missing
        // capability changes availabilities, never the provider topology.
        let topology = describe_topology(
            &map_with_smt(),
            &capability_absent(),
            &working_frequencies(),
        );

        let definitions = definitions_for(&topology);
        let mut owners: Vec<&str> = definitions
            .iter()
            .map(|definition| definition.provider_id.as_str())
            .collect();
        owners.sort_unstable();
        owners.dedup();

        assert_eq!(owners, [PROVIDER_ID]);
    }

    #[test]
    fn the_capability_being_present_reports_everything_as_available() {
        // The other side of the same function: nothing is degraded when the
        // entry point resolves.
        let topology = describe_topology(
            &map_with_smt(),
            &Availability::Available,
            &working_frequencies(),
        );

        assert!(definitions_for(&topology)
            .iter()
            .all(|definition| definition.availability.is_available()));
    }

    #[test]
    fn a_processor_missing_a_frequency_keeps_its_usage() {
        // Frequency availability stays per processor even while the usage
        // availability is machine-wide.
        let mut frequencies = working_frequencies();
        frequencies.remove(&LogicalId::new(2));

        let topology = describe_topology(&map_with_smt(), &Availability::Available, &frequencies);
        let definitions = definitions_for(&topology);
        let id = LogicalId::new(2);

        assert!(availability_of(&definitions, &cpu::usage_logical_ref(id)).is_available());
        assert_eq!(
            availability_of(&definitions, &cpu::frequency_current_ref(id)).status_str(),
            "unsupported"
        );
        // And its neighbour is untouched.
        assert!(
            availability_of(&definitions, &cpu::frequency_current_ref(LogicalId::new(3)))
                .is_available()
        );
    }

    #[test]
    fn a_machine_with_no_discoverable_processors_still_declares_the_system_metrics() {
        // Topology API failing entirely: no per-processor rows, but the
        // machine-wide metrics remain.
        let topology = describe_topology(
            &ProcessorMap::default(),
            &capability_absent(),
            &BTreeMap::new(),
        );
        let definitions = definitions_for(&topology);

        assert_eq!(definitions.len(), 4);
        assert!(availability_of(&definitions, &cpu::usage_total_ref()).is_available());
    }

    #[test]
    fn recombines_the_two_halves_of_a_filetime() {
        assert_eq!(filetime_to_u64(0, 0), 0);
        assert_eq!(filetime_to_u64(1, 0), 1);
        assert_eq!(filetime_to_u64(0, 1), 1_u64 << 32);
        assert_eq!(filetime_to_u64(u32::MAX, 0), u32::MAX as u64);
        assert_eq!(filetime_to_u64(u32::MAX, u32::MAX), u64::MAX);
        // A concrete interleaved value.
        assert_eq!(
            filetime_to_u64(0x89AB_CDEF, 0x0123_4567),
            0x0123_4567_89AB_CDEF
        );
    }

    #[test]
    fn subtracts_idle_from_kernel_rather_than_adding_it() {
        // The Windows-specific trap: idle is already inside kernel.
        // kernel=800 (of which 700 idle), user=200 -> total 1000, busy 300.
        let counters = counters_from_system_times(700, 800, 200).expect("consistent");

        assert_eq!(counters.total, 1000);
        assert_eq!(counters.busy, 300);
        assert!(counters.is_consistent());
    }

    #[test]
    fn a_fully_idle_machine_reports_zero_busy() {
        let counters = counters_from_system_times(1000, 1000, 0).expect("consistent");

        assert_eq!(counters.total, 1000);
        assert_eq!(counters.busy, 0);
    }

    #[test]
    fn a_fully_busy_machine_reports_all_busy() {
        let counters = counters_from_system_times(0, 400, 600).expect("consistent");

        assert_eq!(counters.total, 1000);
        assert_eq!(counters.busy, 1000);
    }

    #[test]
    fn rejects_idle_larger_than_the_total() {
        assert!(counters_from_system_times(2000, 1000, 0).is_none());
    }

    #[test]
    fn rejects_an_overflowing_total() {
        assert!(counters_from_system_times(0, u64::MAX, 1).is_none());
    }

    #[test]
    fn the_aggregate_and_per_processor_conversions_agree() {
        // Same counters, two code paths: they must not drift apart, or
        // cpu.usage.total and cpu.usage.logical would mean different things.
        use super::super::cpu_perf::ProcessorTimes;

        for (idle, kernel, user) in [
            (700_u64, 800_u64, 200_u64),
            (0, 1000, 0),
            (1000, 1000, 0),
            (123, 456, 789),
        ] {
            assert_eq!(
                counters_from_system_times(idle, kernel, user),
                ProcessorTimes::new(idle, kernel, user).to_counters(),
                "the two conversions disagreed for ({idle}, {kernel}, {user})"
            );
        }
    }

    #[test]
    fn a_delta_between_two_readings_gives_a_usage_percentage() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(&CpuSnapshot::total_only(
            counters_from_system_times(700, 800, 200).expect("consistent"),
        ));

        // Later: kernel +300 (of which 250 idle), user +50.
        // total delta 350, busy delta 100 -> ~28.57%.
        let report = tracker
            .update(&CpuSnapshot::total_only(
                counters_from_system_times(950, 1100, 250).expect("consistent"),
            ))
            .expect("no error");

        let Some(CpuUsage::Ready(percent)) = report.total else {
            panic!("expected a usable percentage, got {:?}", report.total);
        };
        assert!((percent - 28.571_428).abs() < 1e-4, "got {percent}");
    }

    #[test]
    fn identical_readings_wait_instead_of_reporting_idle() {
        let counters = counters_from_system_times(700, 800, 200).expect("consistent");
        let tracker = CpuUsageTracker::new();
        tracker.prime(&CpuSnapshot::total_only(counters));

        assert_eq!(
            tracker
                .update(&CpuSnapshot::total_only(counters))
                .expect("no error")
                .total,
            Some(CpuUsage::NeedsAnotherSample(
                NeedsAnotherSample::NoElapsedTime
            ))
        );
    }

    #[test]
    fn rewound_counters_reset_the_baseline() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(&CpuSnapshot::total_only(
            counters_from_system_times(7000, 8000, 2000).expect("consistent"),
        ));

        assert_eq!(
            tracker
                .update(&CpuSnapshot::total_only(
                    counters_from_system_times(700, 800, 200).expect("consistent")
                ))
                .expect("no error")
                .total,
            Some(CpuUsage::NeedsAnotherSample(
                NeedsAnotherSample::CountersWentBackwards
            ))
        );
    }

    #[test]
    fn usage_is_always_clamped_into_range() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(&CpuSnapshot::total_only(
            counters_from_system_times(0, 1000, 0).expect("consistent"),
        ));

        // Every additional tick is busy: 100%, never more.
        let Some(CpuUsage::Ready(percent)) = tracker
            .update(&CpuSnapshot::total_only(
                counters_from_system_times(0, 2000, 0).expect("consistent"),
            ))
            .expect("no error")
            .total
        else {
            panic!("expected a percentage");
        };

        assert!((0.0..=100.0).contains(&percent));
        assert_eq!(percent, 100.0);
    }
}
