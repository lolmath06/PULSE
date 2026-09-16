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
//! ## The cost of the choice
//!
//! `NtQuerySystemInformation` lives in `ntdll` and Microsoft documents it as
//! "may be altered or unavailable in future versions". In practice
//! `SystemProcessorPerformanceInformation` has been stable since Windows NT
//! and is what every Windows system monitor uses. PULSE contains the risk:
//!
//! - the call is isolated in one `unsafe` function that does nothing but fill
//!   a buffer and convert it to safe structs;
//! - a failed or short reply degrades to "per-processor usage unavailable",
//!   never a panic and never a fabricated number;
//! - `cpu.usage.total` comes from the fully documented `GetSystemTimes`, so it
//!   keeps working even if this call ever stops.
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
    use windows_sys::Wdk::System::SystemInformation::{
        NtQuerySystemInformation, SystemProcessorPerformanceInformation, SYSTEM_INFORMATION_CLASS,
    };
    use windows_sys::Win32::Foundation::NTSTATUS;
    use windows_sys::Win32::System::WindowsProgramming::SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION;

    use crate::metrics::model::{MetricError, MetricErrorCode};

    use super::super::cpu_topology::ProcessorMap;
    use super::{GroupTimes, ProcessorTimes};

    // The group-aware form of `NtQuerySystemInformation`. Not exposed by
    // `windows-sys`, so it is declared here; present in `ntdll` since
    // Windows 7. For `SystemProcessorPerformanceInformation` the input buffer
    // is a single `USHORT` naming the processor group to report on.
    #[link(name = "ntdll")]
    extern "system" {
        fn NtQuerySystemInformationEx(
            system_information_class: SYSTEM_INFORMATION_CLASS,
            input_buffer: *const core::ffi::c_void,
            input_buffer_length: u32,
            system_information: *mut core::ffi::c_void,
            system_information_length: u32,
            return_length: *mut u32,
        ) -> NTSTATUS;
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

    /// Turns the returned byte count into a number of whole records.
    ///
    /// A reply that is not a whole number of records means the buffer was
    /// misinterpreted; truncating rather than checking would silently
    /// attribute one processor's times to another.
    fn record_count(returned_bytes: u32, capacity: usize) -> Result<usize, MetricError> {
        let record_size = core::mem::size_of::<SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION>();
        let returned = returned_bytes as usize;

        if !returned.is_multiple_of(record_size) {
            return Err(MetricError::new(
                MetricErrorCode::Parse,
                format!(
                    "processor performance reply of {returned} bytes is not a whole number of \
                     {record_size}-byte records"
                ),
            ));
        }

        Ok((returned / record_size).min(capacity))
    }

    /// Reads one processor group's per-processor times.
    ///
    /// # Safety
    ///
    /// The single `unsafe` call writes at most `capacity` records into a
    /// `Vec` that has been reserved for exactly that many and whose pointer is
    /// alive for the whole call. The group number is passed by pointer to a
    /// live local of the declared size. The status and the returned byte count
    /// are both checked before any element is read, and the vector's length is
    /// only grown to the number of records the call actually reported, so no
    /// uninitialised element is ever observed.
    fn read_group(group: u16, capacity: usize) -> Result<GroupTimes, MetricError> {
        let mut buffer: Vec<SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION> =
            Vec::with_capacity(capacity);
        let byte_capacity = capacity
            .checked_mul(core::mem::size_of::<SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION>())
            .and_then(|bytes| u32::try_from(bytes).ok())
            .ok_or_else(|| MetricError::internal("processor performance buffer size overflowed"))?;

        let mut returned_bytes: u32 = 0;
        let group_number = group;

        // SAFETY: see the function docs.
        let status = unsafe {
            NtQuerySystemInformationEx(
                SystemProcessorPerformanceInformation,
                core::ptr::from_ref(&group_number).cast(),
                u32::try_from(core::mem::size_of::<u16>()).expect("2 fits in u32"),
                buffer.as_mut_ptr().cast(),
                byte_capacity,
                &mut returned_bytes,
            )
        };

        if status < 0 {
            return Err(MetricError::new(
                MetricErrorCode::ProviderUnavailable,
                format!(
                    "NtQuerySystemInformationEx(SystemProcessorPerformanceInformation) failed \
                     for processor group {group} with status {status:#010x}"
                ),
            ));
        }

        let count = record_count(returned_bytes, capacity)?;

        // SAFETY: the call reported `returned_bytes` written, and `count` is
        // derived from it and clamped to the reserved capacity, so exactly
        // that many elements are initialised.
        unsafe { buffer.set_len(count) };

        Ok(GroupTimes::new(
            group,
            buffer.iter().filter_map(convert).collect(),
        ))
    }

    /// Reads per-processor times for every group in the machine.
    ///
    /// **One call per processor group, never one per processor**: a
    /// 128-processor machine costs two calls.
    ///
    /// A group that fails is skipped rather than failing the whole read, so a
    /// problem on one group still leaves the others measurable.
    pub fn read_all_groups(map: &ProcessorMap) -> Result<Vec<GroupTimes>, MetricError> {
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
            match read_group(group, map.processors_in_group(group) as usize) {
                Ok(reading) => readings.push(reading),
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

    /// Whether the per-processor counter source works on this machine.
    ///
    /// Probed once at startup so the catalog can declare per-processor usage
    /// unsupported up front on a system where the call is unavailable, rather
    /// than reporting N failures on every refresh.
    pub fn probe() -> Result<(), MetricError> {
        let mut returned_bytes: u32 = 0;
        let mut buffer = [SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION::default(); 1];
        let group_number: u16 = 0;

        // SAFETY: a one-element buffer with its exact byte size; a short reply
        // is expected and its contents are never read.
        let status = unsafe {
            NtQuerySystemInformationEx(
                SystemProcessorPerformanceInformation,
                core::ptr::from_ref(&group_number).cast(),
                u32::try_from(core::mem::size_of::<u16>()).expect("2 fits in u32"),
                buffer.as_mut_ptr().cast(),
                u32::try_from(core::mem::size_of_val(&buffer)).expect("size fits in u32"),
                &mut returned_bytes,
            )
        };

        // STATUS_INFO_LENGTH_MISMATCH is the expected answer to a
        // deliberately undersized buffer: the entry point exists and
        // understands the request, which is all this probe asks.
        const STATUS_INFO_LENGTH_MISMATCH: NTSTATUS = 0xC000_0004_u32 as NTSTATUS;

        if status >= 0 || status == STATUS_INFO_LENGTH_MISMATCH {
            return Ok(());
        }

        Err(MetricError::new(
            MetricErrorCode::Unsupported,
            format!(
                "per-processor CPU counters are unavailable on this system \
                 (NtQuerySystemInformationEx returned {status:#010x})"
            ),
        ))
    }

    /// Kept to prove the non-`Ex` entry point is linked and to document the
    /// fallback shape; the group-aware form is what PULSE calls.
    #[allow(dead_code)]
    fn single_group_fallback(
        buffer: &mut [SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION],
        returned_bytes: &mut u32,
    ) -> NTSTATUS {
        // SAFETY: the buffer's own length and size are passed; the caller
        // reads no element unless the status is success.
        unsafe {
            NtQuerySystemInformation(
                SystemProcessorPerformanceInformation,
                buffer.as_mut_ptr().cast(),
                u32::try_from(core::mem::size_of_val(buffer)).unwrap_or(0),
                returned_bytes,
            )
        }
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
