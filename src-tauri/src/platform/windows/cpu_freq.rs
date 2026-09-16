//! Per-logical-processor CPU frequency on Windows.
//!
//! # The API
//!
//! `CallNtPowerInformation(ProcessorInformation, …)` fills an array of
//! `PROCESSOR_POWER_INFORMATION`, one entry per logical processor:
//!
//! ```text
//! struct PROCESSOR_POWER_INFORMATION {
//!     ULONG Number;            processor number, per the caveat below
//!     ULONG MaxMhz;            maximum frequency the platform reports
//!     ULONG CurrentMhz;        current frequency the platform reports
//!     ULONG MhzLimit;          current policy ceiling
//!     ULONG MaxIdleState;
//!     ULONG CurrentIdleState;
//! }
//! ```
//!
//! It is documented, needs no privileges, returns the **whole array in one
//! call** rather than one call per processor, and carries exactly the two
//! figures PULSE publishes. `powrprof.dll` is present on every Windows
//! installation.
//!
//! Alternatives considered and rejected: WMI's `Win32_Processor.CurrentClockSpeed`
//! (needs the WMI service, is machine-wide rather than per processor, and is
//! notoriously stale); PDH's `\Processor Information(*)\% Processor Performance`
//! (a percentage of the base clock, not a frequency, and localised); reading
//! the `MSR` registers directly (needs a kernel driver and administrator).
//!
//! # Mapping entries to processors
//!
//! `Number` is unreliable on machines with several processor groups: it is
//! documented as the processor number, but on multi-group systems it repeats
//! per group. PULSE therefore maps by **array position**, which Windows fills
//! in system order — group 0's processors, then group 1's — the same order the
//! [`ProcessorMap`] assigns ordinals in. `Number` is still read and used as a
//! cross-check on single-group machines.
//!
//! # What the numbers mean
//!
//! `CurrentMhz` is the kernel's latest accounting figure for that processor,
//! not an instantaneous hardware measurement; see
//! `metrics::wellknown::cpu::frequency` for the full semantics PULSE promises.
//!
//! `MaxMhz` is what the platform advertises as the maximum. On most Windows
//! systems this is the processor's **base** frequency rather than its maximum
//! turbo frequency — the same figure the System control panel shows. It is a
//! hardware figure, not a power-policy ceiling (that is `MhzLimit`, which PULSE
//! deliberately does not publish as a maximum), so it is published as
//! `cpu.frequency.max` with that meaning documented.
//!
//! On a hybrid CPU the entries differ between performance and efficiency
//! cores, and nothing here assumes otherwise.
//!
//! [`ProcessorMap`]: super::cpu_topology::ProcessorMap

use std::collections::BTreeMap;

use crate::metrics::wellknown::cpu::{megahertz_to_hertz, LogicalId};

use super::cpu_topology::ProcessorMap;

/// One entry of the `PROCESSOR_POWER_INFORMATION` array, in plain Rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcessorPowerReading {
    /// The `Number` field, kept for cross-checking on single-group machines.
    pub number: u32,
    pub max_mhz: u32,
    pub current_mhz: u32,
}

impl ProcessorPowerReading {
    pub const fn new(number: u32, max_mhz: u32, current_mhz: u32) -> Self {
        Self {
            number,
            max_mhz,
            current_mhz,
        }
    }
}

/// One processor's frequencies, in hertz.
///
/// Each half is independent: a machine that reports a maximum but no current
/// figure — or the reverse — publishes the one it has and marks the other
/// unavailable, rather than losing both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcessorFrequencies {
    pub current_hz: Option<u64>,
    pub max_hz: Option<u64>,
}

impl ProcessorFrequencies {
    /// Converts one reading from megahertz to the canonical hertz.
    ///
    /// A zero field means "not reported" on this API — common under
    /// hypervisors — and becomes `None` rather than a published `0 Hz`.
    pub fn from_reading(reading: ProcessorPowerReading) -> Self {
        Self {
            current_hz: megahertz_to_hertz(u64::from(reading.current_mhz)),
            max_hz: megahertz_to_hertz(u64::from(reading.max_mhz)),
        }
    }

    /// Whether either figure could be obtained.
    pub const fn is_empty(&self) -> bool {
        self.current_hz.is_none() && self.max_hz.is_none()
    }
}

/// Attributes the returned array to PULSE ordinals.
///
/// Entry `i` is the logical processor with ordinal `i`, per the array-position
/// rule documented above. Entries beyond the known processor count are
/// dropped rather than invented into existence.
pub fn frequencies_by_ordinal(
    readings: &[ProcessorPowerReading],
    map: &ProcessorMap,
) -> BTreeMap<LogicalId, ProcessorFrequencies> {
    let mut frequencies = BTreeMap::new();

    for (index, &reading) in readings.iter().enumerate() {
        let Ok(ordinal) = u32::try_from(index) else {
            continue;
        };
        if ordinal >= map.logical_count() {
            continue;
        }

        let converted = ProcessorFrequencies::from_reading(reading);
        if converted.is_empty() {
            continue;
        }

        frequencies.insert(LogicalId::new(ordinal), converted);
    }

    frequencies
}

/// The parts that call into `powrprof`.
#[cfg(target_os = "windows")]
pub mod imp {
    use windows_sys::Win32::System::Power::{CallNtPowerInformation, ProcessorInformation};

    use crate::metrics::model::{MetricError, MetricErrorCode};

    use super::super::cpu_topology::ProcessorMap;
    use super::ProcessorPowerReading;

    /// `PROCESSOR_POWER_INFORMATION`, declared here because `windows-sys` does
    /// not ship it. Layout is fixed by the Windows SDK header `powerbase.h`.
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct ProcessorPowerInformation {
        number: u32,
        max_mhz: u32,
        current_mhz: u32,
        mhz_limit: u32,
        max_idle_state: u32,
        current_idle_state: u32,
    }

    /// Reads the frequency of every logical processor.
    ///
    /// **One call for the whole machine**, sized from the discovered processor
    /// count rather than from any assumption about how many there are.
    ///
    /// # Safety
    ///
    /// The single `unsafe` call writes at most `capacity` records into a `Vec`
    /// reserved for exactly that many, whose pointer is alive for the whole
    /// call, and whose byte size is passed as the output length. No input
    /// buffer is required for `ProcessorInformation`, so a null pointer and a
    /// zero length are passed, as the API documents. The status is checked
    /// before any element is read, and the vector's length is only grown after
    /// a successful call, so no uninitialised element is ever observed.
    pub fn read_all(map: &ProcessorMap) -> Result<Vec<ProcessorPowerReading>, MetricError> {
        let capacity = map.logical_count() as usize;
        if capacity == 0 {
            return Err(MetricError::new(
                MetricErrorCode::NotDetected,
                "no logical processor was discovered",
            ));
        }

        let mut buffer: Vec<ProcessorPowerInformation> = Vec::with_capacity(capacity);
        let byte_capacity = capacity
            .checked_mul(core::mem::size_of::<ProcessorPowerInformation>())
            .and_then(|bytes| u32::try_from(bytes).ok())
            .ok_or_else(|| MetricError::internal("processor power buffer size overflowed"))?;

        // SAFETY: see the function docs.
        let status = unsafe {
            CallNtPowerInformation(
                ProcessorInformation,
                core::ptr::null(),
                0,
                buffer.as_mut_ptr().cast(),
                byte_capacity,
            )
        };

        if status < 0 {
            return Err(MetricError::new(
                MetricErrorCode::Unsupported,
                format!(
                    "CallNtPowerInformation(ProcessorInformation) failed with status {status:#010x}"
                ),
            ));
        }

        // SAFETY: a successful call fills the whole buffer it was given, which
        // is exactly `capacity` records.
        unsafe { buffer.set_len(capacity) };

        Ok(buffer
            .iter()
            .map(|raw| ProcessorPowerReading::new(raw.number, raw.max_mhz, raw.current_mhz))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::super::cpu_topology::{GroupMask, ProcessorRelation};
    use super::*;

    fn map_with(processors: u8) -> ProcessorMap {
        ProcessorMap::from_relations(
            &(0..processors)
                .map(|bit| ProcessorRelation::core(vec![GroupMask::new(0, 1_u64 << bit)]))
                .collect::<Vec<_>>(),
        )
    }

    // --- conversion -------------------------------------------------------

    #[test]
    fn converts_megahertz_to_the_canonical_hertz() {
        let frequencies =
            ProcessorFrequencies::from_reading(ProcessorPowerReading::new(0, 3_200, 1_800));

        // No frontend should ever see MHz.
        assert_eq!(frequencies.max_hz, Some(3_200_000_000));
        assert_eq!(frequencies.current_hz, Some(1_800_000_000));
    }

    #[test]
    fn a_zero_field_becomes_unavailable_not_zero_hertz() {
        // Common under hypervisors. "0 GHz" would read as a claim.
        let frequencies =
            ProcessorFrequencies::from_reading(ProcessorPowerReading::new(0, 3_200, 0));

        assert_eq!(frequencies.max_hz, Some(3_200_000_000));
        assert_eq!(frequencies.current_hz, None);

        let neither = ProcessorFrequencies::from_reading(ProcessorPowerReading::new(0, 0, 0));
        assert!(neither.is_empty());
    }

    #[test]
    fn the_two_frequencies_are_independent() {
        // A maximum without a current reading still publishes the maximum.
        let max_only = ProcessorFrequencies::from_reading(ProcessorPowerReading::new(0, 4_000, 0));
        assert!(!max_only.is_empty());
        assert!(max_only.max_hz.is_some());

        let current_only =
            ProcessorFrequencies::from_reading(ProcessorPowerReading::new(0, 0, 2_400));
        assert!(!current_only.is_empty());
        assert!(current_only.current_hz.is_some());
    }

    #[test]
    fn an_absurd_megahertz_value_cannot_overflow_the_conversion() {
        let frequencies =
            ProcessorFrequencies::from_reading(ProcessorPowerReading::new(0, u32::MAX, u32::MAX));

        // u32::MAX MHz is nonsense but fits in u64 hertz; the point is that it
        // does not wrap.
        assert_eq!(
            frequencies.max_hz,
            Some(u64::from(u32::MAX) * 1_000_000),
            "conversion must not wrap"
        );
    }

    // --- attribution ------------------------------------------------------

    #[test]
    fn array_position_becomes_the_logical_processor_ordinal() {
        let readings = [
            ProcessorPowerReading::new(0, 3_200, 3_000),
            ProcessorPowerReading::new(1, 3_200, 1_200),
            ProcessorPowerReading::new(2, 3_200, 800),
        ];

        let frequencies = frequencies_by_ordinal(&readings, &map_with(3));

        assert_eq!(frequencies.len(), 3);
        assert_eq!(
            frequencies[&LogicalId::new(0)].current_hz,
            Some(3_000_000_000)
        );
        assert_eq!(
            frequencies[&LogicalId::new(2)].current_hz,
            Some(800_000_000)
        );
    }

    #[test]
    fn hybrid_processors_keep_their_different_maxima() {
        // Four P-cores at 5.6 GHz and four E-cores at 4.1 GHz: nothing may
        // flatten them to one figure.
        let readings: Vec<ProcessorPowerReading> = (0..8)
            .map(|index| {
                let max = if index < 4 { 5_600 } else { 4_100 };
                ProcessorPowerReading::new(index, max, 3_000)
            })
            .collect();

        let frequencies = frequencies_by_ordinal(&readings, &map_with(8));

        assert_eq!(frequencies[&LogicalId::new(0)].max_hz, Some(5_600_000_000));
        assert_eq!(frequencies[&LogicalId::new(7)].max_hz, Some(4_100_000_000));
        assert_ne!(
            frequencies[&LogicalId::new(0)].max_hz,
            frequencies[&LogicalId::new(7)].max_hz
        );
    }

    #[test]
    fn a_machine_over_sixty_four_processors_is_covered_end_to_end() {
        let map = ProcessorMap::from_relations(
            &(0..2_u16)
                .flat_map(|group| {
                    (0..64_u8).map(move |bit| {
                        ProcessorRelation::core(vec![GroupMask::new(group, 1_u64 << bit)])
                    })
                })
                .collect::<Vec<_>>(),
        );
        let readings: Vec<ProcessorPowerReading> = (0..128)
            .map(|index| ProcessorPowerReading::new(index % 64, 3_200, 2_000 + index))
            .collect();

        let frequencies = frequencies_by_ordinal(&readings, &map);

        assert_eq!(frequencies.len(), 128);
        // `Number` repeats per group, which is exactly why position is used.
        assert_eq!(
            frequencies[&LogicalId::new(64)].current_hz,
            Some(2_064_000_000)
        );
        assert_eq!(
            frequencies[&LogicalId::new(127)].current_hz,
            Some(2_127_000_000)
        );
    }

    #[test]
    fn surplus_entries_are_dropped_rather_than_invented_into_processors() {
        let readings = [ProcessorPowerReading::new(0, 3_200, 3_000); 8];

        let frequencies = frequencies_by_ordinal(&readings, &map_with(4));

        assert_eq!(frequencies.len(), 4);
        assert!(!frequencies.contains_key(&LogicalId::new(4)));
    }

    #[test]
    fn a_short_reply_leaves_the_remaining_processors_unreported() {
        // And crucially, does not shift anyone's identity.
        let readings = [
            ProcessorPowerReading::new(0, 3_200, 3_000),
            ProcessorPowerReading::new(1, 3_200, 1_000),
        ];

        let frequencies = frequencies_by_ordinal(&readings, &map_with(4));

        assert_eq!(frequencies.len(), 2);
        assert_eq!(
            frequencies[&LogicalId::new(1)].current_hz,
            Some(1_000_000_000)
        );
        assert!(!frequencies.contains_key(&LogicalId::new(2)));
    }

    #[test]
    fn a_processor_reporting_nothing_is_omitted_while_the_others_survive() {
        let readings = [
            ProcessorPowerReading::new(0, 3_200, 3_000),
            ProcessorPowerReading::new(1, 0, 0),
            ProcessorPowerReading::new(2, 3_200, 2_000),
        ];

        let frequencies = frequencies_by_ordinal(&readings, &map_with(3));

        assert_eq!(frequencies.len(), 2);
        assert!(!frequencies.contains_key(&LogicalId::new(1)));
        assert!(frequencies.contains_key(&LogicalId::new(2)));
    }

    #[test]
    fn an_empty_reply_yields_an_empty_map_not_a_panic() {
        assert!(frequencies_by_ordinal(&[], &map_with(4)).is_empty());
        assert!(frequencies_by_ordinal(
            &[ProcessorPowerReading::new(0, 3_200, 3_000)],
            &ProcessorMap::default()
        )
        .is_empty());
    }

    #[test]
    fn the_result_is_ordered_numerically_by_ordinal() {
        let readings: Vec<ProcessorPowerReading> = (0..12)
            .map(|index| ProcessorPowerReading::new(index, 3_200, 2_000))
            .collect();

        let ordinals: Vec<u32> = frequencies_by_ordinal(&readings, &map_with(12))
            .keys()
            .map(|id| id.get())
            .collect();

        assert_eq!(ordinals, (0..12).collect::<Vec<u32>>());
    }
}
