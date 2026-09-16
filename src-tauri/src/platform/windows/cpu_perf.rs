//! Per-logical-processor CPU times on Windows.
//!
//! # Choosing the API
//!
//! Windows offers several ways to get per-processor utilisation, and the easy
//! one is not the right one. What PULSE needs: native, unprivileged, no
//! subprocess, no localised strings, one call per sample rather than one per
//! processor, correct identification of each logical processor, correct on
//! machines with more than 64 processors, and testable arithmetic.
//!
//! | Option | Verdict |
//! |---|---|
//! | `GetSystemTimes` | Machine-wide only. No per-processor breakdown at all. Retained for `cpu.usage.total`. |
//! | PDH, `\Processor(_Total)\% Processor Time` | Counter paths are **localised** — on a French install the object is `Processeur`. `PdhLookupPerfNameByIndex` works around it, but adds a registry-backed name lookup, a query/counter handle lifecycle, and mandatory double sampling. Also the legacy `Processor` object is capped at 64 processors. |
//! | PDH, `\Processor Information(*)\% Processor Time` | Group-aware, and instance names encode `group,index`. Still localised, still a heavier object model, and instance names must be string-parsed to be attributed to a processor. |
//! | WMI `Win32_PerfFormattedData_*` | Requires the WMI service, costs tens of milliseconds per query, and is explicitly out of scope. |
//! | PowerShell `Get-Counter`, `typeperf`, `wmic` | Subprocesses. Forbidden, and for good reason. |
//! | `QueryIdleProcessorCycleTime` | Cycle counts, not time. On a hybrid CPU the cycle rate differs between P-cores and E-cores, so the ratio is not a utilisation percentage. |
//! | **`NtQuerySystemInformation(Ex)` with `SystemProcessorPerformanceInformation`** | **Chosen.** |
//!
//! ## Why this one
//!
//! It returns a flat array of `SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION` — one
//! entry per logical processor, carrying exactly the `IdleTime`, `KernelTime`
//! and `UserTime` totals the shared delta model already consumes. One call per
//! processor group covers the whole machine. There are no strings anywhere, so
//! nothing depends on the system language. It needs no privileges. And the
//! numbers are the same ones Task Manager and Process Explorer display, which
//! is what a user will compare PULSE against.
//!
//! The arithmetic is identical to `GetSystemTimes`', so `cpu.usage.total` and
//! `cpu.usage.logical` are the same measurement at two scopes rather than two
//! definitions that happen to share a name.
//!
//! ## The cost of the choice, and how it is contained
//!
//! `NtQuerySystemInformation` lives in `ntdll` and Microsoft documents it as
//! "may be altered or unavailable in future versions". In practice
//! `SystemProcessorPerformanceInformation` has been stable since Windows NT
//! and is what every Windows system monitor uses. PULSE still treats it as an
//! **optional capability**, never as a guarantee:
//!
//! - the entry point is **resolved at runtime**, not imported at load time, so
//!   a Windows that does not export it cannot stop PULSE from starting. See
//!   [`super::ntdll`] for why that distinction matters.
//! - every call goes through [`ProcessorTimesSource`], so the logic below is
//!   exercised on Fedora against a fake source, including the failure paths;
//! - a failed, short or malformed reply degrades to "per-processor usage
//!   unavailable", never a panic and never a fabricated number;
//! - `cpu.usage.total` comes from the fully documented `GetSystemTimes`, and
//!   topology and frequency come from `kernel32` and `powrprof`, so all three
//!   keep working even when this capability is entirely absent.
//!
//! ## Processor groups
//!
//! The plain `NtQuerySystemInformation` form reports only the calling thread's
//! processor group — on a 128-processor machine it silently returns 64
//! entries. `NtQuerySystemInformationEx` takes a group number as its input
//! buffer and answers for that group, so PULSE calls it once per group listed
//! by [`ProcessorMap::groups`] and stitches the results together through the
//! ordinal mapping. On the single-group machines that are the overwhelming
//! majority, that is one call.
//!
//! [`ProcessorMap::groups`]: super::cpu_topology::ProcessorMap::groups

use std::collections::BTreeMap;

use crate::metrics::model::{MetricError, MetricErrorCode};
use crate::metrics::wellknown::cpu::{CpuCounters, LogicalId};

use super::cpu_topology::{ProcessorMap, ProcessorNumber};

/// One logical processor's cumulative time counters, in 100-nanosecond units.
///
/// The plain-Rust form of `SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION`, with the
/// fields PULSE uses and none of the reserved ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcessorTimes {
    pub idle: u64,
    /// **Includes idle time**, exactly as `GetSystemTimes`' kernel figure does.
    pub kernel: u64,
    pub user: u64,
}

impl ProcessorTimes {
    pub const fn new(idle: u64, kernel: u64, user: u64) -> Self {
        Self { idle, kernel, user }
    }

    /// Converts to the platform-neutral counters.
    ///
    /// **Windows counts idle time inside `kernel`**, which is the trap here:
    /// the Linux formula would report a completely idle processor as heavily
    /// busy.
    ///
    /// ```text
    /// total = kernel + user      (kernel already contains idle)
    /// busy  = total - idle
    /// ```
    ///
    /// Returns `None` when the values are inconsistent — idle exceeding the
    /// total, or an arithmetic overflow — so a nonsensical reading becomes an
    /// unavailable metric rather than a percentage above 100.
    pub fn to_counters(self) -> Option<CpuCounters> {
        let total = self.kernel.checked_add(self.user)?;
        let busy = total.checked_sub(self.idle)?;

        Some(CpuCounters::new(busy, total))
    }
}

/// The per-processor times read for one processor group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupTimes {
    pub group: u16,
    /// Entries in the order the API returned them: index `i` is bit `i` of the
    /// group's affinity mask.
    pub times: Vec<ProcessorTimes>,
}

impl GroupTimes {
    pub fn new(group: u16, times: Vec<ProcessorTimes>) -> Self {
        Self { group, times }
    }
}

/// Something that can answer a per-group processor performance query.
///
/// The seam between "how the counters are obtained" and "what is done with
/// them". On Windows the only implementation wraps the dynamically resolved
/// `ntdll` entry point; in tests it is a fake, which is what lets the
/// stitching, the short-reply handling and the failure paths below be verified
/// from Fedora.
pub trait ProcessorTimesSource: Send + Sync {
    /// Reads up to `capacity` processors' counters from one processor group.
    ///
    /// Returning fewer entries than `capacity` is allowed and handled; the
    /// processors that were reported are still published.
    fn read_group(&self, group: u16, capacity: usize) -> Result<Vec<ProcessorTimes>, MetricError>;
}

/// Turns a reply's byte count into a number of whole records.
///
/// A reply that is not a whole number of records means the buffer was
/// misinterpreted; truncating rather than checking would silently attribute
/// one processor's times to another. A reply longer than the buffer is clamped
/// rather than trusted.
///
/// Kept free of any Windows type so the arithmetic is testable anywhere.
pub fn records_in_reply(
    returned_bytes: usize,
    record_size: usize,
    capacity: usize,
) -> Result<usize, MetricError> {
    if record_size == 0 {
        return Err(MetricError::internal(
            "processor performance record size cannot be zero",
        ));
    }

    // `%` rather than `usize::is_multiple_of`, which is newer than this
    // crate's declared `rust-version`.
    if returned_bytes % record_size != 0 {
        return Err(MetricError::new(
            MetricErrorCode::Parse,
            format!(
                "processor performance reply of {returned_bytes} bytes is not a whole number \
                 of {record_size}-byte records"
            ),
        ));
    }

    Ok((returned_bytes / record_size).min(capacity))
}

/// Reads per-processor times for every group in the machine.
///
/// **One call per processor group, never one per processor**: a 128-processor
/// machine costs two calls.
///
/// A group that fails is skipped rather than failing the whole read, so a
/// problem on one group still leaves the others measurable. Only when *no*
/// group could be read at all does this report failure — and even then it is
/// one metric family, not the provider.
pub fn read_all_groups(
    source: &dyn ProcessorTimesSource,
    map: &ProcessorMap,
) -> Result<Vec<GroupTimes>, MetricError> {
    let groups = map.groups();
    if groups.is_empty() {
        return Err(MetricError::new(
            MetricErrorCode::NotDetected,
            "no processor group was discovered",
        ));
    }

    let mut readings = Vec::with_capacity(groups.len());
    let mut last_error = None;

    for group in groups {
        match source.read_group(group, map.processors_in_group(group) as usize) {
            Ok(times) => readings.push(GroupTimes::new(group, times)),
            Err(error) => last_error = Some(error),
        }
    }

    if readings.is_empty() {
        return Err(last_error.unwrap_or_else(|| {
            MetricError::new(
                MetricErrorCode::ProviderUnavailable,
                "no processor group could be read",
            )
        }));
    }

    Ok(readings)
}

/// Attributes every group's readings to PULSE ordinals.
///
/// The array Windows returns is indexed by position within the group, so entry
/// `i` of group `g` is the logical processor `(g, i)`. Resolving that through
/// the [`ProcessorMap`] is what keeps topology, usage and frequency pointing at
/// the same `cpu:logical-N`.
///
/// Entries that map to no known processor are dropped rather than guessed at:
/// an array longer than the group (which a race with CPU hotplug can produce)
/// must not shift every subsequent processor's identity.
pub fn counters_by_ordinal(
    groups: &[GroupTimes],
    map: &ProcessorMap,
) -> BTreeMap<LogicalId, CpuCounters> {
    let mut counters = BTreeMap::new();

    for group in groups {
        for (index, times) in group.times.iter().enumerate() {
            let Ok(index) = u8::try_from(index) else {
                continue;
            };
            let Some(id) = map.ordinal_of(ProcessorNumber::new(group.group, index)) else {
                continue;
            };
            let Some(entry) = times.to_counters() else {
                continue;
            };

            counters.insert(id, entry);
        }
    }

    counters
}

/// The parts that call into `ntdll`.
#[cfg(target_os = "windows")]
pub mod imp {
    use windows_sys::Win32::System::WindowsProgramming::SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION;

    use crate::metrics::model::{MetricError, MetricErrorCode};

    use super::super::ntdll::imp::NtCpuApi;
    use super::{records_in_reply, ProcessorTimes, ProcessorTimesSource};

    /// The real per-processor counter source, backed by the dynamically
    /// resolved `ntdll` entry point.
    ///
    /// Holding one is proof the capability exists: it cannot be constructed
    /// without a successful resolve *and* a successful probe. The provider
    /// therefore stores an `Option<NtProcessorTimes>` and the `None` case is
    /// the whole "this Windows does not offer per-processor counters" story —
    /// no flags, no re-checking, no repeated `GetProcAddress`.
    #[derive(Debug, Clone, Copy)]
    pub struct NtProcessorTimes {
        api: NtCpuApi,
    }

    impl NtProcessorTimes {
        /// Resolves the entry point once.
        ///
        /// Returns a structured error — never a panic — when `ntdll` cannot be
        /// looked up, when it does not export the symbol, or when the resolved
        /// symbol refuses the query.
        pub fn resolve() -> Result<Self, MetricError> {
            Ok(Self {
                api: NtCpuApi::resolve()?,
            })
        }
    }

    impl ProcessorTimesSource for NtProcessorTimes {
        /// Reads one processor group's per-processor times.
        ///
        /// Contains **no `unsafe`**: the buffer is ordinary initialised Rust
        /// memory, the call goes through [`NtCpuApi`]'s safe wrapper, and only
        /// the records the reply accounts for are converted.
        fn read_group(
            &self,
            group: u16,
            capacity: usize,
        ) -> Result<Vec<ProcessorTimes>, MetricError> {
            // Initialised up front rather than reserved-and-`set_len`, so no
            // element is ever observed uninitialised whatever the callee does.
            // `SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION` is plain old data, so
            // this costs one memset of a few hundred bytes.
            let mut buffer =
                vec![SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION::default(); capacity.max(1)];
            let mut returned_bytes: u32 = 0;

            let status =
                self.api
                    .query_processor_performance(group, &mut buffer, &mut returned_bytes);

            if status < 0 {
                return Err(MetricError::new(
                    MetricErrorCode::ProviderUnavailable,
                    format!(
                        "NtQuerySystemInformationEx(SystemProcessorPerformanceInformation) \
                         failed for processor group {group} with status {status:#010x}"
                    ),
                ));
            }

            let count = records_in_reply(
                returned_bytes as usize,
                core::mem::size_of::<SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION>(),
                buffer.len(),
            )?;

            Ok(buffer[..count].iter().filter_map(convert).collect())
        }
    }

    /// Converts the raw FFI record into the safe form.
    ///
    /// The counters are unsigned durations declared as `i64`; a negative value
    /// would mean the buffer was misread, so it is refused rather than cast
    /// into an enormous `u64`.
    fn convert(raw: &SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION) -> Option<ProcessorTimes> {
        Some(ProcessorTimes::new(
            u64::try_from(raw.IdleTime).ok()?,
            u64::try_from(raw.KernelTime).ok()?,
            u64::try_from(raw.UserTime).ok()?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::cpu_topology::{GroupMask, ProcessorRelation};
    use super::*;

    fn single_group_map(processors: u8) -> ProcessorMap {
        ProcessorMap::from_relations(
            &(0..processors)
                .map(|bit| ProcessorRelation::core(vec![GroupMask::new(0, 1_u64 << bit)]))
                .collect::<Vec<_>>(),
        )
    }

    fn two_group_map(per_group: u8) -> ProcessorMap {
        ProcessorMap::from_relations(
            &(0..2_u16)
                .flat_map(|group| {
                    (0..per_group).map(move |bit| {
                        ProcessorRelation::core(vec![GroupMask::new(group, 1_u64 << bit)])
                    })
                })
                .collect::<Vec<_>>(),
        )
    }

    /// A scripted stand-in for the `ntdll` entry point.
    ///
    /// This is what makes the failure paths testable from Fedora: the real
    /// source can only exist on a Windows that exports the symbol, but every
    /// decision around it — stitching groups together, tolerating a short
    /// reply, surviving one group failing — is exercised here.
    struct FakeSource {
        /// Replies per group, in group order.
        replies: BTreeMap<u16, Result<Vec<ProcessorTimes>, MetricError>>,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl FakeSource {
        fn new() -> Self {
            Self {
                replies: BTreeMap::new(),
                calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }

        fn answering(mut self, group: u16, times: Vec<ProcessorTimes>) -> Self {
            self.replies.insert(group, Ok(times));
            self
        }

        fn failing(mut self, group: u16, error: MetricError) -> Self {
            self.replies.insert(group, Err(error));
            self
        }

        fn call_count(&self) -> usize {
            self.calls.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl ProcessorTimesSource for FakeSource {
        fn read_group(
            &self,
            group: u16,
            _capacity: usize,
        ) -> Result<Vec<ProcessorTimes>, MetricError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            match self.replies.get(&group) {
                Some(Ok(times)) => Ok(times.clone()),
                Some(Err(error)) => Err(error.clone()),
                None => Err(MetricError::new(
                    MetricErrorCode::ProviderUnavailable,
                    "group not scripted",
                )),
            }
        }
    }

    fn busy(count: usize) -> Vec<ProcessorTimes> {
        vec![ProcessorTimes::new(0, 100, 0); count]
    }

    // --- record accounting ------------------------------------------------

    #[test]
    fn counts_whole_records_in_a_reply() {
        assert_eq!(records_in_reply(0, 48, 4), Ok(0));
        assert_eq!(records_in_reply(48, 48, 4), Ok(1));
        assert_eq!(records_in_reply(192, 48, 4), Ok(4));
    }

    #[test]
    fn clamps_a_reply_longer_than_the_buffer() {
        // Cannot happen if the API honours the length it was given, but
        // trusting it would index past the buffer.
        assert_eq!(records_in_reply(480, 48, 4), Ok(4));
    }

    #[test]
    fn rejects_a_reply_that_is_not_a_whole_number_of_records() {
        // Truncating here would silently attribute one processor's times to
        // another, which is worse than reporting nothing.
        let error = records_in_reply(50, 48, 4).expect_err("must be rejected");

        assert_eq!(error.code, MetricErrorCode::Parse);
        assert!(error.message.contains("whole number"));
    }

    #[test]
    fn a_zero_record_size_is_an_internal_error_not_a_division_by_zero() {
        let error = records_in_reply(48, 0, 4).expect_err("must be rejected");
        assert_eq!(error.code, MetricErrorCode::Internal);
    }

    // --- reading every group through the source ---------------------------

    #[test]
    fn reads_one_group_per_group_never_one_per_processor() {
        // The performance promise: a 128-processor machine costs two calls.
        let map = two_group_map(64);
        let source = FakeSource::new()
            .answering(0, busy(64))
            .answering(1, busy(64));

        let groups = read_all_groups(&source, &map).expect("read");

        assert_eq!(groups.len(), 2);
        assert_eq!(source.call_count(), 2, "one call per group");
        assert_eq!(counters_by_ordinal(&groups, &map).len(), 128);
    }

    #[test]
    fn a_single_failing_group_does_not_cost_the_others() {
        let map = two_group_map(4);
        let source = FakeSource::new().answering(0, busy(4)).failing(
            1,
            MetricError::new(MetricErrorCode::ProviderUnavailable, "group 1 refused"),
        );

        let groups = read_all_groups(&source, &map).expect("group 0 still readable");

        assert_eq!(groups.len(), 1);
        let counters = counters_by_ordinal(&groups, &map);
        assert_eq!(counters.len(), 4);
        assert!(counters.contains_key(&LogicalId::new(0)));
        // Group 1's processors are simply absent — never invented as 0%.
        assert!(!counters.contains_key(&LogicalId::new(4)));
    }

    #[test]
    fn every_group_failing_is_an_error_rather_than_a_silent_empty_reading() {
        let map = two_group_map(4);
        let source = FakeSource::new()
            .failing(
                0,
                MetricError::new(MetricErrorCode::ProviderUnavailable, "refused"),
            )
            .failing(
                1,
                MetricError::new(MetricErrorCode::ProviderUnavailable, "refused"),
            );

        let error = read_all_groups(&source, &map).expect_err("nothing could be read");

        assert_eq!(error.code, MetricErrorCode::ProviderUnavailable);
    }

    #[test]
    fn a_short_reply_publishes_the_processors_it_did_cover() {
        // "résultat API invalide": fewer entries than the group holds.
        let map = single_group_map(4);
        let source = FakeSource::new().answering(0, busy(2));

        let groups = read_all_groups(&source, &map).expect("read");
        let counters = counters_by_ordinal(&groups, &map);

        assert_eq!(counters.len(), 2);
        assert!(counters.contains_key(&LogicalId::new(1)));
        assert!(!counters.contains_key(&LogicalId::new(2)));
    }

    #[test]
    fn an_empty_reply_yields_no_counters_and_no_panic() {
        let map = single_group_map(4);
        let source = FakeSource::new().answering(0, Vec::new());

        let groups = read_all_groups(&source, &map).expect("read");

        assert_eq!(groups.len(), 1);
        assert!(counters_by_ordinal(&groups, &map).is_empty());
    }

    #[test]
    fn a_machine_with_no_discovered_group_is_reported_as_not_detected() {
        let source = FakeSource::new();

        let error =
            read_all_groups(&source, &ProcessorMap::default()).expect_err("no group to read from");

        assert_eq!(error.code, MetricErrorCode::NotDetected);
        assert_eq!(source.call_count(), 0, "nothing to call");
    }

    // --- the Windows-specific arithmetic ----------------------------------

    #[test]
    fn subtracts_idle_from_kernel_rather_than_adding_it() {
        // The Windows trap: idle is already inside kernel.
        // kernel=800 (of which 700 idle), user=200 -> total 1000, busy 300.
        let counters = ProcessorTimes::new(700, 800, 200)
            .to_counters()
            .expect("consistent");

        assert_eq!(counters.total, 1000);
        assert_eq!(counters.busy, 300);
        assert!(counters.is_consistent());
    }

    #[test]
    fn a_fully_idle_processor_reports_zero_busy() {
        let counters = ProcessorTimes::new(1000, 1000, 0)
            .to_counters()
            .expect("consistent");

        assert_eq!(counters.total, 1000);
        assert_eq!(counters.busy, 0);
    }

    #[test]
    fn a_fully_busy_processor_reports_all_busy() {
        let counters = ProcessorTimes::new(0, 400, 600)
            .to_counters()
            .expect("consistent");

        assert_eq!(counters.total, 1000);
        assert_eq!(counters.busy, 1000);
    }

    #[test]
    fn rejects_idle_larger_than_the_total() {
        assert_eq!(ProcessorTimes::new(2000, 1000, 0).to_counters(), None);
    }

    #[test]
    fn rejects_an_overflowing_total() {
        assert_eq!(ProcessorTimes::new(0, u64::MAX, 1).to_counters(), None);
    }

    #[test]
    fn a_zeroed_reading_is_consistent_but_carries_no_elapsed_time() {
        let counters = ProcessorTimes::default().to_counters().expect("consistent");

        assert_eq!(counters, CpuCounters::new(0, 0));
        // The tracker turns this into "waiting for the next sample", never 0%.
    }

    // --- attribution to ordinals ------------------------------------------

    #[test]
    fn array_position_becomes_the_logical_processor_it_belongs_to() {
        let map = single_group_map(4);
        let groups = vec![GroupTimes::new(
            0,
            vec![
                ProcessorTimes::new(0, 100, 0),   // 100% busy
                ProcessorTimes::new(100, 100, 0), // idle
                ProcessorTimes::new(50, 100, 0),  // half
                ProcessorTimes::new(0, 50, 50),   // 100% busy
            ],
        )];

        let counters = counters_by_ordinal(&groups, &map);

        assert_eq!(counters.len(), 4);
        assert_eq!(counters[&LogicalId::new(0)], CpuCounters::new(100, 100));
        assert_eq!(counters[&LogicalId::new(1)], CpuCounters::new(0, 100));
        assert_eq!(counters[&LogicalId::new(2)], CpuCounters::new(50, 100));
        assert_eq!(counters[&LogicalId::new(3)], CpuCounters::new(100, 100));
    }

    #[test]
    fn a_second_group_continues_the_ordinals_rather_than_restarting_them() {
        // The >64-processor correctness property, in miniature: group 1's
        // entry 0 must be CPU 4, not CPU 0 overwriting group 0's reading.
        let map = two_group_map(4);
        let groups = vec![
            GroupTimes::new(0, vec![ProcessorTimes::new(0, 100, 0); 4]),
            GroupTimes::new(1, vec![ProcessorTimes::new(100, 100, 0); 4]),
        ];

        let counters = counters_by_ordinal(&groups, &map);

        assert_eq!(counters.len(), 8);
        // Group 0 was busy, group 1 idle — and they did not collide.
        assert_eq!(counters[&LogicalId::new(0)].busy, 100);
        assert_eq!(counters[&LogicalId::new(3)].busy, 100);
        assert_eq!(counters[&LogicalId::new(4)].busy, 0);
        assert_eq!(counters[&LogicalId::new(7)].busy, 0);
    }

    #[test]
    fn a_machine_with_more_than_sixty_four_processors_is_attributed_correctly() {
        let map = two_group_map(64);
        let groups = vec![
            GroupTimes::new(0, vec![ProcessorTimes::new(0, 100, 0); 64]),
            GroupTimes::new(1, vec![ProcessorTimes::new(0, 100, 0); 64]),
        ];

        let counters = counters_by_ordinal(&groups, &map);

        assert_eq!(counters.len(), 128);
        assert!(counters.contains_key(&LogicalId::new(63)));
        assert!(
            counters.contains_key(&LogicalId::new(64)),
            "the 65th processor must not be lost to a single-group assumption"
        );
        assert!(counters.contains_key(&LogicalId::new(127)));
    }

    #[test]
    fn a_reply_longer_than_the_group_does_not_shift_every_identity() {
        // A race with CPU hotplug can return more entries than expected.
        // Dropping the surplus is right; renumbering everything is not.
        let map = single_group_map(2);
        let groups = vec![GroupTimes::new(
            0,
            vec![
                ProcessorTimes::new(0, 100, 0),
                ProcessorTimes::new(50, 100, 0),
                ProcessorTimes::new(90, 100, 0),
            ],
        )];

        let counters = counters_by_ordinal(&groups, &map);

        assert_eq!(counters.len(), 2);
        assert_eq!(counters[&LogicalId::new(0)].busy, 100);
        assert_eq!(counters[&LogicalId::new(1)].busy, 50);
    }

    #[test]
    fn a_short_reply_reports_the_processors_it_did_cover() {
        // One processor missing must not cost the others their usage.
        let map = single_group_map(4);
        let groups = vec![GroupTimes::new(0, vec![ProcessorTimes::new(0, 100, 0); 2])];

        let counters = counters_by_ordinal(&groups, &map);

        assert_eq!(counters.len(), 2);
        assert!(counters.contains_key(&LogicalId::new(0)));
        assert!(!counters.contains_key(&LogicalId::new(2)));
    }

    #[test]
    fn an_inconsistent_entry_is_dropped_and_the_rest_survive() {
        let map = single_group_map(3);
        let groups = vec![GroupTimes::new(
            0,
            vec![
                ProcessorTimes::new(0, 100, 0),
                ProcessorTimes::new(9_000, 100, 0), // idle above total
                ProcessorTimes::new(50, 100, 0),
            ],
        )];

        let counters = counters_by_ordinal(&groups, &map);

        assert_eq!(counters.len(), 2);
        assert!(!counters.contains_key(&LogicalId::new(1)));
        assert!(counters.contains_key(&LogicalId::new(2)));
    }

    #[test]
    fn an_unknown_group_is_ignored_rather_than_misattributed() {
        let map = single_group_map(2);
        let groups = vec![GroupTimes::new(7, vec![ProcessorTimes::new(0, 100, 0); 2])];

        assert!(counters_by_ordinal(&groups, &map).is_empty());
    }

    #[test]
    fn an_empty_reading_yields_an_empty_map_not_a_panic() {
        assert!(counters_by_ordinal(&[], &single_group_map(4)).is_empty());
        assert!(
            counters_by_ordinal(&[GroupTimes::new(0, Vec::new())], &single_group_map(4)).is_empty()
        );
    }

    #[test]
    fn the_result_is_ordered_numerically_by_ordinal() {
        let map = two_group_map(4);
        // Groups supplied out of order, as a failed-then-retried read might.
        let groups = vec![
            GroupTimes::new(1, vec![ProcessorTimes::new(0, 100, 0); 4]),
            GroupTimes::new(0, vec![ProcessorTimes::new(0, 100, 0); 4]),
        ];

        let ordinals: Vec<u32> = counters_by_ordinal(&groups, &map)
            .keys()
            .map(|id| id.get())
            .collect();

        assert_eq!(ordinals, (0..8).collect::<Vec<u32>>());
    }

    // --- the shared delta model, on Windows numbers ------------------------

    #[test]
    fn two_readings_produce_a_usage_percentage_per_processor() {
        use crate::metrics::wellknown::cpu::{CpuSnapshot, CpuUsage, CpuUsageTracker};

        let map = single_group_map(2);
        let tracker = CpuUsageTracker::new();

        let first = counters_by_ordinal(
            &[GroupTimes::new(
                0,
                vec![
                    ProcessorTimes::new(700, 800, 200),
                    ProcessorTimes::new(700, 800, 200),
                ],
            )],
            &map,
        );
        let mut snapshot = CpuSnapshot::new();
        for (&id, &counters) in &first {
            snapshot.insert_logical(id, counters);
        }
        tracker.prime(&snapshot);

        // CPU 0 busy, CPU 1 idle, over the same interval.
        let second = counters_by_ordinal(
            &[GroupTimes::new(
                0,
                vec![
                    ProcessorTimes::new(700, 1000, 300),  // +300 total, +300 busy
                    ProcessorTimes::new(1000, 1100, 200), // +300 total, +0 busy
                ],
            )],
            &map,
        );
        let mut snapshot = CpuSnapshot::new();
        for (&id, &counters) in &second {
            snapshot.insert_logical(id, counters);
        }

        let report = tracker.update(&snapshot).expect("no error");

        assert_eq!(
            report.logical(LogicalId::new(0)),
            Some(CpuUsage::Ready(100.0))
        );
        assert_eq!(
            report.logical(LogicalId::new(1)),
            Some(CpuUsage::Ready(0.0))
        );
    }

    #[test]
    fn identical_readings_wait_rather_than_reporting_idle() {
        use crate::metrics::wellknown::cpu::{
            CpuSnapshot, CpuUsage, CpuUsageTracker, NeedsAnotherSample,
        };

        let map = single_group_map(1);
        let reading = counters_by_ordinal(
            &[GroupTimes::new(0, vec![ProcessorTimes::new(700, 800, 200)])],
            &map,
        );
        let mut snapshot = CpuSnapshot::new();
        for (&id, &counters) in &reading {
            snapshot.insert_logical(id, counters);
        }

        let tracker = CpuUsageTracker::new();
        tracker.prime(&snapshot);

        assert_eq!(
            tracker
                .update(&snapshot)
                .expect("ok")
                .logical(LogicalId::new(0)),
            Some(CpuUsage::NeedsAnotherSample(
                NeedsAnotherSample::NoElapsedTime
            ))
        );
    }

    #[test]
    fn rewound_counters_reset_that_processors_baseline() {
        use crate::metrics::wellknown::cpu::{
            CpuSnapshot, CpuUsage, CpuUsageTracker, NeedsAnotherSample,
        };

        let map = single_group_map(1);
        let snapshot_of = |times: ProcessorTimes| {
            let mut snapshot = CpuSnapshot::new();
            for (&id, &counters) in
                counters_by_ordinal(&[GroupTimes::new(0, vec![times])], &map).iter()
            {
                snapshot.insert_logical(id, counters);
            }
            snapshot
        };

        let tracker = CpuUsageTracker::new();
        tracker.prime(&snapshot_of(ProcessorTimes::new(7000, 8000, 2000)));

        assert_eq!(
            tracker
                .update(&snapshot_of(ProcessorTimes::new(700, 800, 200)))
                .expect("ok")
                .logical(LogicalId::new(0)),
            Some(CpuUsage::NeedsAnotherSample(
                NeedsAnotherSample::CountersWentBackwards
            ))
        );

        // And it recovers on the next request.
        assert!(matches!(
            tracker
                .update(&snapshot_of(ProcessorTimes::new(800, 1000, 300)))
                .expect("ok")
                .logical(LogicalId::new(0)),
            Some(CpuUsage::Ready(_))
        ));
    }
}
