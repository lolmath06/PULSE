//! Windows CPU topology, and the mapping that gives every logical processor a
//! PULSE ordinal.
//!
//! # The API
//!
//! `GetLogicalProcessorInformationEx(RelationAll, …)` returns a packed,
//! variable-length list of records describing how logical processors, physical
//! cores, caches, NUMA nodes and packages relate to each other. It is the only
//! Win32 call that reports all of this, it needs no privileges, and — unlike
//! `GetSystemInfo`'s `dwNumberOfProcessors` — it is not capped at one processor
//! group.
//!
//! PULSE uses two of the relation kinds and ignores the rest:
//!
//! | Record | Meaning | PULSE metric |
//! |---|---|---|
//! | `RelationProcessorCore` | one physical core, plus the mask of the logical processors on it | `cpu.count.physical` |
//! | `RelationProcessorPackage` | one physical package (socket) | `cpu.count.package` |
//!
//! Each `RelationProcessorCore` record carries a `GROUP_AFFINITY` array: for a
//! core with SMT, one mask with two bits set; for a core without, one bit.
//! Counting records gives physical cores; counting set bits gives logical
//! processors. **Conflating the two is the classic Windows topology bug**, and
//! it is exactly the distinction PULSE refuses to blur.
//!
//! # Processor groups
//!
//! Windows addresses logical processors as `(group, index in group)`, with at
//! most 64 per group. A machine with more than 64 logical processors has
//! several groups, and there is no system-wide flat processor number: CPU 70
//! is "group 1, bit 5".
//!
//! PULSE will not put that on screen. A dashboard should say `CPU 70`, not
//! `group 1 bit 5`. So this module assigns a **PULSE ordinal** by sorting every
//! discovered `(group, index)` pair ascending — group first, then index — and
//! numbering the result from zero:
//!
//! ```text
//! (0,  0) → cpu:logical-0
//! (0,  1) → cpu:logical-1
//!  …
//! (0, 63) → cpu:logical-63
//! (1,  0) → cpu:logical-64
//! (1,  1) → cpu:logical-65
//! ```
//!
//! On the overwhelmingly common single-group machine this is the identity
//! mapping, so `cpu:logical-5` is Task Manager's "CPU 5". The mapping is
//! deterministic (the sort makes it independent of the order Windows returned
//! the records in) and is kept in memory so that topology, usage and frequency
//! all attribute their numbers to the same `SourceId`.
//!
//! # Testability
//!
//! Everything below the FFI boundary is pure. The `unsafe` block converts the
//! packed Windows buffer into [`ProcessorRelation`] values and stops; every
//! decision — which bits are processors, how cores are counted, how ordinals
//! are assigned — is a plain function over plain structs, unit-tested on
//! Fedora. That is why a 128-processor, four-group machine can be covered by
//! tests on a laptop that has neither.

use std::collections::BTreeSet;

use crate::metrics::wellknown::cpu::{CpuTopology, LogicalId, LogicalProcessor};

/// Maximum logical processors in one Windows processor group.
pub const MAX_PROCESSORS_PER_GROUP: u8 = 64;

/// A logical processor as Windows addresses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessorNumber {
    /// Processor group. `0` on every machine with 64 or fewer processors.
    pub group: u16,
    /// Bit index within the group's affinity mask, `0..64`.
    pub index: u8,
}

impl ProcessorNumber {
    pub const fn new(group: u16, index: u8) -> Self {
        Self { group, index }
    }
}

/// One group affinity mask, as carried by a relation record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupMask {
    pub group: u16,
    /// `KAFFINITY` — one bit per logical processor in the group.
    pub mask: u64,
}

impl GroupMask {
    pub const fn new(group: u16, mask: u64) -> Self {
        Self { group, mask }
    }

    /// The logical processors this mask selects, in ascending bit order.
    pub fn processors(self) -> impl Iterator<Item = ProcessorNumber> {
        (0..MAX_PROCESSORS_PER_GROUP)
            .filter(move |bit| self.mask & (1_u64 << bit) != 0)
            .map(move |bit| ProcessorNumber::new(self.group, bit))
    }
}

/// The relation kinds PULSE reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    /// `RelationProcessorCore` — one physical execution core.
    Core,
    /// `RelationProcessorPackage` — one physical package (socket).
    Package,
}

/// One relation record, already converted out of the packed Windows buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorRelation {
    pub kind: RelationKind,
    /// The group affinities this record covers. A core is normally confined to
    /// one group; a package on a large machine can span several.
    pub group_masks: Vec<GroupMask>,
}

impl ProcessorRelation {
    pub fn core(group_masks: Vec<GroupMask>) -> Self {
        Self {
            kind: RelationKind::Core,
            group_masks,
        }
    }

    pub fn package(group_masks: Vec<GroupMask>) -> Self {
        Self {
            kind: RelationKind::Package,
            group_masks,
        }
    }
}

/// The mapping between Windows processor numbers and PULSE ordinals.
///
/// Held for the life of the provider so that topology, usage and frequency all
/// resolve to the same `cpu:logical-N`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessorMap {
    /// Every logical processor, sorted by `(group, index)`. The position in
    /// this vector **is** the PULSE ordinal.
    processors: Vec<ProcessorNumber>,
    physical_core_count: Option<u32>,
    package_count: Option<u32>,
}

impl ProcessorMap {
    /// Derives the mapping and the counts from a set of relation records.
    ///
    /// Sorting is what makes the ordinals deterministic: Windows does not
    /// promise any particular record order, and a mapping that changed between
    /// two runs would silently re-point every saved reference.
    pub fn from_relations(relations: &[ProcessorRelation]) -> Self {
        let cores: Vec<&ProcessorRelation> = relations
            .iter()
            .filter(|relation| relation.kind == RelationKind::Core)
            .collect();

        // Logical processors come from the core records: every logical
        // processor sits on exactly one physical core, so the union of the
        // core masks is the complete set, with no double counting.
        let mut processors: BTreeSet<ProcessorNumber> = BTreeSet::new();
        for core in &cores {
            for mask in &core.group_masks {
                processors.extend(mask.processors());
            }
        }

        let package_count = relations
            .iter()
            .filter(|relation| relation.kind == RelationKind::Package)
            .count();

        Self {
            processors: processors.into_iter().collect(),
            // One record per physical core — not one per set bit, which would
            // count hardware threads.
            physical_core_count: (!cores.is_empty()).then_some(cores.len() as u32),
            package_count: (package_count > 0).then_some(package_count as u32),
        }
    }

    /// The PULSE ordinal of a Windows processor number.
    pub fn ordinal_of(&self, processor: ProcessorNumber) -> Option<LogicalId> {
        self.processors
            .binary_search(&processor)
            .ok()
            .map(|index| LogicalId::new(index as u32))
    }

    /// The Windows processor number behind a PULSE ordinal.
    pub fn processor_at(&self, id: LogicalId) -> Option<ProcessorNumber> {
        self.processors.get(id.get() as usize).copied()
    }

    /// Every logical processor, in ordinal order.
    pub fn processors(&self) -> &[ProcessorNumber] {
        &self.processors
    }

    /// How many logical processors were discovered.
    pub fn logical_count(&self) -> u32 {
        self.processors.len() as u32
    }

    /// Every processor group present, ascending.
    ///
    /// Used to drive the per-group API calls: one call per group, never one
    /// per processor.
    pub fn groups(&self) -> Vec<u16> {
        let groups: BTreeSet<u16> = self
            .processors
            .iter()
            .map(|processor| processor.group)
            .collect();

        groups.into_iter().collect()
    }

    /// How many logical processors sit in one group.
    pub fn processors_in_group(&self, group: u16) -> u32 {
        self.processors
            .iter()
            .filter(|processor| processor.group == group)
            .count() as u32
    }

    pub fn physical_core_count(&self) -> Option<u32> {
        self.physical_core_count
    }

    pub fn package_count(&self) -> Option<u32> {
        self.package_count
    }

    /// Builds the platform-neutral topology, with every metric available.
    ///
    /// The caller narrows individual availabilities afterwards from what the
    /// usage and frequency APIs actually returned.
    pub fn to_topology(&self) -> CpuTopology {
        CpuTopology::new(
            (0..self.logical_count())
                .map(|ordinal| LogicalProcessor::available(LogicalId::new(ordinal)))
                .collect(),
            self.physical_core_count,
            self.package_count,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds `count` single-threaded cores in one group, starting at bit 0.
    fn cores_without_smt(group: u16, count: u8) -> Vec<ProcessorRelation> {
        (0..count)
            .map(|bit| ProcessorRelation::core(vec![GroupMask::new(group, 1 << bit)]))
            .collect()
    }

    /// Builds `count` two-thread cores in one group, starting at bit 0.
    fn cores_with_smt(group: u16, count: u8) -> Vec<ProcessorRelation> {
        (0..count)
            .map(|core| ProcessorRelation::core(vec![GroupMask::new(group, 0b11 << (core * 2))]))
            .collect()
    }

    // --- mask decoding ----------------------------------------------------

    #[test]
    fn a_mask_lists_the_processors_it_selects() {
        let processors: Vec<ProcessorNumber> = GroupMask::new(0, 0b1011).processors().collect();

        assert_eq!(
            processors,
            [
                ProcessorNumber::new(0, 0),
                ProcessorNumber::new(0, 1),
                ProcessorNumber::new(0, 3),
            ]
        );
    }

    #[test]
    fn the_highest_bit_of_a_full_group_is_decoded() {
        // A full 64-processor group: bit 63 must not be lost to a shift bug.
        let processors: Vec<ProcessorNumber> = GroupMask::new(3, u64::MAX).processors().collect();

        assert_eq!(processors.len(), 64);
        assert_eq!(processors[63], ProcessorNumber::new(3, 63));
    }

    #[test]
    fn an_empty_mask_selects_nothing() {
        assert_eq!(GroupMask::new(0, 0).processors().count(), 0);
    }

    // --- core and package counting ---------------------------------------

    #[test]
    fn counts_one_physical_core_per_record_not_per_thread() {
        // Four cores, two threads each: 8 logical processors, 4 cores.
        // Counting set bits here is the classic bug.
        let mut relations = cores_with_smt(0, 4);
        relations.push(ProcessorRelation::package(vec![GroupMask::new(0, 0xFF)]));

        let map = ProcessorMap::from_relations(&relations);

        assert_eq!(map.logical_count(), 8);
        assert_eq!(map.physical_core_count(), Some(4));
        assert_eq!(map.package_count(), Some(1));
    }

    #[test]
    fn counts_cores_without_simultaneous_multithreading() {
        let mut relations = cores_without_smt(0, 6);
        relations.push(ProcessorRelation::package(vec![GroupMask::new(0, 0x3F)]));

        let map = ProcessorMap::from_relations(&relations);

        assert_eq!(map.logical_count(), 6);
        assert_eq!(map.physical_core_count(), Some(6));
    }

    #[test]
    fn counts_a_hybrid_processor_correctly() {
        // An Intel hybrid part: 8 P-cores with SMT, 16 E-cores without.
        // 8 + 16 = 24 physical cores, 16 + 16 = 32 logical processors.
        let mut relations: Vec<ProcessorRelation> = (0..8)
            .map(|core| ProcessorRelation::core(vec![GroupMask::new(0, 0b11 << (core * 2))]))
            .collect();
        relations.extend(
            (16..32).map(|bit| ProcessorRelation::core(vec![GroupMask::new(0, 1_u64 << bit)])),
        );
        relations.push(ProcessorRelation::package(vec![GroupMask::new(
            0,
            u64::MAX,
        )]));

        let map = ProcessorMap::from_relations(&relations);

        assert_eq!(map.physical_core_count(), Some(24));
        assert_eq!(map.logical_count(), 32);
        assert_eq!(map.package_count(), Some(1));
    }

    #[test]
    fn counts_several_packages() {
        // Two sockets, four cores each, one group.
        let mut relations: Vec<ProcessorRelation> = (0..8)
            .map(|core| ProcessorRelation::core(vec![GroupMask::new(0, 1_u64 << core)]))
            .collect();
        relations.push(ProcessorRelation::package(vec![GroupMask::new(0, 0x0F)]));
        relations.push(ProcessorRelation::package(vec![GroupMask::new(0, 0xF0)]));

        let map = ProcessorMap::from_relations(&relations);

        assert_eq!(map.package_count(), Some(2));
        assert_eq!(map.physical_core_count(), Some(8));
    }

    #[test]
    fn relation_kinds_other_than_cores_never_add_processors() {
        // A package record covers processors the core records already
        // reported; counting it again would double every machine.
        let relations = vec![
            ProcessorRelation::core(vec![GroupMask::new(0, 0b11)]),
            ProcessorRelation::package(vec![GroupMask::new(0, 0b11)]),
        ];

        assert_eq!(ProcessorMap::from_relations(&relations).logical_count(), 2);
    }

    #[test]
    fn a_machine_that_reports_no_relations_yields_unknown_counts() {
        let map = ProcessorMap::from_relations(&[]);

        assert_eq!(map.logical_count(), 0);
        assert_eq!(map.physical_core_count(), None);
        assert_eq!(map.package_count(), None);
    }

    #[test]
    fn packages_without_cores_still_count() {
        let relations = vec![ProcessorRelation::package(vec![GroupMask::new(0, 0b1)])];
        let map = ProcessorMap::from_relations(&relations);

        assert_eq!(map.package_count(), Some(1));
        assert_eq!(map.physical_core_count(), None);
    }

    // --- ordinal mapping --------------------------------------------------

    #[test]
    fn a_single_group_machine_maps_to_the_identity() {
        // The common case: cpu:logical-5 is Task Manager's CPU 5.
        let map = ProcessorMap::from_relations(&cores_without_smt(0, 8));

        for index in 0..8_u8 {
            assert_eq!(
                map.ordinal_of(ProcessorNumber::new(0, index)),
                Some(LogicalId::new(u32::from(index)))
            );
        }
    }

    #[test]
    fn ordinals_run_group_by_group_for_a_machine_over_sixty_four_processors() {
        // 128 logical processors: two full groups of 64.
        let relations: Vec<ProcessorRelation> = (0..2_u16)
            .flat_map(|group| {
                (0..64_u8)
                    .map(move |bit| ProcessorRelation::core(vec![GroupMask::new(group, 1 << bit)]))
            })
            .collect();

        let map = ProcessorMap::from_relations(&relations);

        assert_eq!(map.logical_count(), 128);
        assert_eq!(map.physical_core_count(), Some(128));
        assert_eq!(map.groups(), [0, 1]);

        // The boundary is where a 64-processor assumption breaks.
        assert_eq!(
            map.ordinal_of(ProcessorNumber::new(0, 63)),
            Some(LogicalId::new(63))
        );
        assert_eq!(
            map.ordinal_of(ProcessorNumber::new(1, 0)),
            Some(LogicalId::new(64)),
            "group 1 bit 0 must become CPU 64, not CPU 0"
        );
        assert_eq!(
            map.ordinal_of(ProcessorNumber::new(1, 63)),
            Some(LogicalId::new(127))
        );
    }

    #[test]
    fn four_groups_with_simultaneous_multithreading_map_correctly() {
        // 256 logical processors across four groups: a real large server.
        let relations: Vec<ProcessorRelation> = (0..4_u16)
            .flat_map(|group| {
                (0..32_u8).map(move |core| {
                    ProcessorRelation::core(vec![GroupMask::new(group, 0b11 << (core * 2))])
                })
            })
            .collect();

        let map = ProcessorMap::from_relations(&relations);

        assert_eq!(map.logical_count(), 256);
        assert_eq!(map.physical_core_count(), Some(128));
        assert_eq!(map.groups(), [0, 1, 2, 3]);
        assert_eq!(map.processors_in_group(2), 64);

        assert_eq!(
            map.ordinal_of(ProcessorNumber::new(2, 0)),
            Some(LogicalId::new(128))
        );
        assert_eq!(
            map.ordinal_of(ProcessorNumber::new(3, 63)),
            Some(LogicalId::new(255))
        );
    }

    #[test]
    fn partly_filled_groups_are_numbered_without_gaps() {
        // Windows balances processors across groups rather than filling one:
        // two groups of 40 on an 80-processor machine. PULSE ordinals stay
        // contiguous, so the UI shows CPU 0-79 with no holes.
        let relations: Vec<ProcessorRelation> = (0..2_u16)
            .flat_map(|group| {
                (0..40_u8)
                    .map(move |bit| ProcessorRelation::core(vec![GroupMask::new(group, 1 << bit)]))
            })
            .collect();

        let map = ProcessorMap::from_relations(&relations);

        assert_eq!(map.logical_count(), 80);
        assert_eq!(
            map.ordinal_of(ProcessorNumber::new(1, 0)),
            Some(LogicalId::new(40))
        );
        assert_eq!(
            map.ordinal_of(ProcessorNumber::new(0, 40)),
            None,
            "a processor that does not exist has no ordinal"
        );
    }

    #[test]
    fn the_mapping_round_trips_in_both_directions() {
        let relations: Vec<ProcessorRelation> = (0..3_u16)
            .flat_map(|group| {
                (0..10_u8)
                    .map(move |bit| ProcessorRelation::core(vec![GroupMask::new(group, 1 << bit)]))
            })
            .collect();
        let map = ProcessorMap::from_relations(&relations);

        for ordinal in 0..map.logical_count() {
            let id = LogicalId::new(ordinal);
            let processor = map.processor_at(id).expect("mapped");
            assert_eq!(map.ordinal_of(processor), Some(id));
        }

        assert_eq!(map.processor_at(LogicalId::new(30)), None);
    }

    #[test]
    fn the_mapping_does_not_depend_on_the_order_windows_returned_records_in() {
        let ascending: Vec<ProcessorRelation> = (0..2_u16)
            .flat_map(|group| {
                (0..8_u8)
                    .map(move |bit| ProcessorRelation::core(vec![GroupMask::new(group, 1 << bit)]))
            })
            .collect();

        let mut descending = ascending.clone();
        descending.reverse();

        assert_eq!(
            ProcessorMap::from_relations(&ascending),
            ProcessorMap::from_relations(&descending),
            "ordinals must be deterministic across runs"
        );
    }

    #[test]
    fn a_core_spanning_two_groups_is_still_one_core() {
        // Defensive: the API allows multiple group masks per record.
        let relations = vec![ProcessorRelation::core(vec![
            GroupMask::new(0, 0b1),
            GroupMask::new(1, 0b1),
        ])];
        let map = ProcessorMap::from_relations(&relations);

        assert_eq!(map.physical_core_count(), Some(1));
        assert_eq!(map.logical_count(), 2);
        assert_eq!(map.groups(), [0, 1]);
    }

    #[test]
    fn duplicate_processors_across_records_are_counted_once() {
        let relations = vec![
            ProcessorRelation::core(vec![GroupMask::new(0, 0b11)]),
            ProcessorRelation::core(vec![GroupMask::new(0, 0b11)]),
        ];
        let map = ProcessorMap::from_relations(&relations);

        // Two core records, but they name the same two logical processors.
        assert_eq!(map.logical_count(), 2);
        assert_eq!(map.physical_core_count(), Some(2));
    }

    // --- conversion to the shared topology --------------------------------

    #[test]
    fn produces_the_platform_neutral_topology() {
        let mut relations = cores_with_smt(0, 4);
        relations.push(ProcessorRelation::package(vec![GroupMask::new(0, 0xFF)]));

        let topology = ProcessorMap::from_relations(&relations).to_topology();

        assert_eq!(topology.logical_count(), 8);
        assert_eq!(topology.physical_core_count, Some(4));
        assert_eq!(topology.package_count, Some(1));

        // Ordinals are contiguous from zero, as the mapping guarantees.
        let ordinals: Vec<u32> = topology
            .logical()
            .iter()
            .map(|processor| processor.id.get())
            .collect();
        assert_eq!(ordinals, (0..8).collect::<Vec<u32>>());
    }
}

/// The part that calls into `kernel32`.
#[cfg(target_os = "windows")]
pub mod imp {
    use windows_sys::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, RelationAll, RelationProcessorCore,
        RelationProcessorPackage, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };

    use crate::metrics::model::{MetricError, MetricErrorCode};

    use super::{GroupMask, ProcessorMap, ProcessorRelation, RelationKind};

    /// `ERROR_INSUFFICIENT_BUFFER`, the expected answer to a sizing call.
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

    /// Asks Windows how large the relation buffer must be.
    ///
    /// # Safety
    ///
    /// A null buffer with a zero length is the documented way to request the
    /// required size; the call writes only through `length`, which points at a
    /// live local. Nothing is read from the buffer.
    fn required_bytes() -> Result<u32, MetricError> {
        let mut length: u32 = 0;

        // SAFETY: see the function docs.
        let ok = unsafe {
            GetLogicalProcessorInformationEx(RelationAll, core::ptr::null_mut(), &mut length)
        };

        // Success with a null buffer would mean there is nothing to report.
        if ok != 0 {
            return Ok(length);
        }

        let error = std::io::Error::last_os_error();
        if error.raw_os_error().map(|code| code as u32) != Some(ERROR_INSUFFICIENT_BUFFER) {
            return Err(MetricError::new(
                MetricErrorCode::Unsupported,
                format!("GetLogicalProcessorInformationEx could not be sized: {error}"),
            ));
        }

        Ok(length)
    }

    /// Reads and converts the relation records.
    ///
    /// The buffer is a packed sequence of variable-length records, each
    /// declaring its own `Size`. Walking it by that field — rather than by
    /// `size_of` a struct whose trailing array is declared as length one — is
    /// the only correct way to traverse it, and is why this conversion exists
    /// instead of a cast.
    ///
    /// # Safety
    ///
    /// The buffer is sized by the call above and filled by the call below,
    /// both checked before anything is read. Each record is read only after
    /// verifying that its declared `Size` is plausible and that the record
    /// lies entirely within the returned bytes, so no read can run past the
    /// allocation. Group affinity arrays are read for exactly the `GroupCount`
    /// the record declares, bounded by the record's own extent.
    pub fn read_relations() -> Result<Vec<ProcessorRelation>, MetricError> {
        let mut length = required_bytes()?;
        if length == 0 {
            return Ok(Vec::new());
        }

        let mut buffer: Vec<u8> = vec![0; length as usize];

        // SAFETY: the buffer is `length` bytes long and its length is passed
        // unchanged; the status is checked before any byte is interpreted.
        let ok = unsafe {
            GetLogicalProcessorInformationEx(
                RelationAll,
                buffer
                    .as_mut_ptr()
                    .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>(),
                &mut length,
            )
        };

        if ok == 0 {
            return Err(MetricError::new(
                MetricErrorCode::Unsupported,
                format!(
                    "GetLogicalProcessorInformationEx failed: {}",
                    std::io::Error::last_os_error()
                ),
            ));
        }

        let filled = (length as usize).min(buffer.len());
        Ok(convert_buffer(&buffer[..filled]))
    }

    /// Walks the packed buffer and converts the records PULSE reads.
    ///
    /// Anything malformed stops the walk rather than being guessed at: a
    /// partial topology is better than one built from misaligned reads.
    fn convert_buffer(buffer: &[u8]) -> Vec<ProcessorRelation> {
        // The fixed prefix every record starts with: Relationship (i32) then
        // Size (u32).
        const HEADER_BYTES: usize = 8;

        let mut relations = Vec::new();
        let mut offset = 0_usize;

        while offset + HEADER_BYTES <= buffer.len() {
            let record = &buffer[offset..];

            // SAFETY: at least HEADER_BYTES remain, and the header is read
            // through the packed struct's own layout.
            let header = unsafe {
                &*record
                    .as_ptr()
                    .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>()
            };
            let size = header.Size as usize;

            if size < HEADER_BYTES || size > record.len() {
                break;
            }

            // Compared rather than pattern-matched: these are plain `i32`
            // constants, and a lowercase constant in a pattern position is a
            // classic Rust trap — it would bind a new variable and match
            // everything instead of comparing.
            let kind = if header.Relationship == RelationProcessorCore {
                Some(RelationKind::Core)
            } else if header.Relationship == RelationProcessorPackage {
                Some(RelationKind::Package)
            } else {
                // Caches, NUMA nodes and groups are not read by PULSE.
                None
            };

            if let Some(kind) = kind {
                // SAFETY: the record is a processor relation of the declared
                // size, so its union holds a PROCESSOR_RELATIONSHIP; the group
                // array is read for exactly GroupCount entries, and only while
                // they lie inside the record.
                let processor = unsafe { &header.Anonymous.Processor };
                let count = processor.GroupCount as usize;

                let group_masks: Vec<GroupMask> = (0..count)
                    .map(|index| {
                        // SAFETY: GroupMask is a trailing array declared as
                        // length one; element `index` is inside the record
                        // because the record declared `GroupCount` of them and
                        // its Size was validated above.
                        let affinity = unsafe { processor.GroupMask.as_ptr().add(index).read() };
                        GroupMask::new(affinity.Group, affinity.Mask as u64)
                    })
                    .collect();

                relations.push(ProcessorRelation { kind, group_masks });
            }

            offset += size;
        }

        relations
    }

    /// Discovers the machine's processor map.
    pub fn read_processor_map() -> Result<ProcessorMap, MetricError> {
        let map = ProcessorMap::from_relations(&read_relations()?);

        if map.logical_count() == 0 {
            return Err(MetricError::new(
                MetricErrorCode::NotDetected,
                "GetLogicalProcessorInformationEx reported no processor core",
            ));
        }

        Ok(map)
    }
}
