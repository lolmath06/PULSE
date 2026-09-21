//! CPU topology: the vocabulary PULSE uses to talk about processors.
//!
//! Three words that everyday speech conflates and PULSE never may:
//!
//! | Term | Meaning | PULSE metric |
//! |---|---|---|
//! | **Processor package** | One physical chip in one socket | `cpu.count.package` |
//! | **Physical core** | One execution core inside a package | `cpu.count.physical` |
//! | **Logical processor** | One hardware thread the scheduler can run on | `cpu.count.logical` |
//!
//! A user saying "CPU usage per core" almost always means *per logical
//! processor*: with SMT (Intel Hyper-Threading, AMD SMT) one physical core
//! exposes two logical processors, and `/proc/stat`, Windows and every
//! scheduler report usage per logical processor, never per core. PULSE
//! therefore publishes `cpu.usage.logical` and says exactly what it measures.
//!
//! On a hybrid CPU the distinction sharpens further: an Intel P-core carries
//! two logical processors while an E-core carries one, so
//! `logical / physical` is not a constant and must never be assumed.

use std::fmt;

use crate::metrics::model::{Availability, SourceId};

/// Prefix of a logical processor's source instance: `cpu:logical-0`.
pub const LOGICAL_INSTANCE_PREFIX: &str = "logical-";

/// Prefix of a processor package's source instance: `cpu:package-0`.
pub const PACKAGE_INSTANCE_PREFIX: &str = "package-";

/// The ordinal of one logical processor, as PULSE numbers them.
///
/// **This is a slot number, not a hardware serial.** It identifies "the
/// logical processor the OS calls number N on this machine", which is stable
/// for as long as the machine's CPU configuration is, and means nothing at all
/// on a different machine. See `docs/metrics/identifiers.md`.
///
/// On Linux it is the kernel's own CPU number (`cpu7` → `LogicalId(7)`), so it
/// lines up with `taskset`, `htop` and everything else the user might compare
/// against. On Windows it is a PULSE-assigned ordinal derived from
/// `(processor group, index in group)` — see
/// `platform::windows::cpu_topology`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LogicalId(u32);

impl LogicalId {
    pub const fn new(ordinal: u32) -> Self {
        Self(ordinal)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// The canonical source identifier, e.g. `cpu:logical-7`.
    pub fn source_id(self) -> SourceId {
        SourceId::new(format!("cpu:{LOGICAL_INSTANCE_PREFIX}{}", self.0))
            .expect("a logical processor source built from an integer is always valid")
    }

    /// The user-facing label, e.g. `CPU 7`.
    ///
    /// Presentation only. `CPU 7` is what every other tool on both platforms
    /// calls it, so PULSE uses the same words rather than inventing
    /// "Logical processor 7" for a list of thirty-two rows.
    pub fn label(self) -> String {
        format!("CPU {}", self.0)
    }

    /// Recovers an ordinal from a `cpu:logical-N` source identifier.
    ///
    /// Returns `None` for any other source, including `cpu:system`.
    pub fn from_source(source: &SourceId) -> Option<Self> {
        if source.kind() != "cpu" {
            return None;
        }

        source
            .instance()
            .strip_prefix(LOGICAL_INSTANCE_PREFIX)
            .and_then(|ordinal| ordinal.parse::<u32>().ok())
            .map(Self::new)
    }
}

impl fmt::Display for LogicalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The index of one processor package, as the platform numbers them.
///
/// **The same numbering as `cpu.count.package`, and deliberately not a second
/// one.** On Linux it is the kernel's `physical_package_id`, which is what
/// `coretemp` also labels its channels with (`Package id 0`), so the thermal
/// reading and the topology count cannot come to describe different things.
/// Inventing a thermal-only package numbering — "the first hwmon device is
/// package 0" — is how a dual-socket machine ends up attributing one socket's
/// temperature to the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageId(u32);

impl PackageId {
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// The canonical source identifier, e.g. `cpu:package-0`.
    pub fn source_id(self) -> SourceId {
        SourceId::new(format!("cpu:{PACKAGE_INSTANCE_PREFIX}{}", self.0))
            .expect("a package source built from an integer is always valid")
    }

    /// The user-facing label, e.g. `Package 0`.
    pub fn label(self) -> String {
        format!("Package {}", self.0)
    }

    /// Recovers an index from a `cpu:package-N` source identifier.
    ///
    /// Returns `None` for any other source, including `cpu:system` and
    /// `cpu:logical-0`.
    pub fn from_source(source: &SourceId) -> Option<Self> {
        if source.kind() != "cpu" {
            return None;
        }

        source
            .instance()
            .strip_prefix(PACKAGE_INSTANCE_PREFIX)
            .and_then(|index| index.parse::<u32>().ok())
            .map(Self::new)
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One processor package, and what can be measured on it.
///
/// Separate from the *count* of packages because the two answer different
/// questions: a machine can know it has two sockets and be able to read a
/// temperature from only one of them, and publishing a package with no sensor
/// as absent-from-the-catalog would make a dashboard silently lose a widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuPackage {
    pub id: PackageId,
    /// Whether `cpu.temperature.package` can be sampled here.
    pub temperature: Availability,
}

impl CpuPackage {
    /// A package whose temperature is readable.
    pub fn available(id: PackageId) -> Self {
        Self {
            id,
            temperature: Availability::Available,
        }
    }

    /// A package whose temperature is not readable, and why.
    pub fn unavailable(id: PackageId, temperature: Availability) -> Self {
        Self { id, temperature }
    }
}

/// One logical processor as a platform discovered it.
///
/// Availability is carried **per metric** rather than per processor: a machine
/// where `cpu17` has no `cpufreq` directory must still publish `cpu17`'s
/// usage. Collapsing the three into one flag is exactly the failure mode
/// PULSE's availability contract exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalProcessor {
    pub id: LogicalId,
    /// Whether `cpu.usage.logical` can be sampled here.
    pub usage: Availability,
    /// Whether `cpu.frequency.current` can be sampled here.
    pub frequency_current: Availability,
    /// Whether `cpu.frequency.max` can be sampled here.
    pub frequency_max: Availability,
}

impl LogicalProcessor {
    /// A logical processor where everything is readable.
    pub fn available(id: LogicalId) -> Self {
        Self {
            id,
            usage: Availability::Available,
            frequency_current: Availability::Available,
            frequency_max: Availability::Available,
        }
    }

    /// Replaces both frequency availabilities, e.g. on a machine with no
    /// `cpufreq` driver at all.
    pub fn with_frequencies(mut self, availability: Availability) -> Self {
        self.frequency_current = availability.clone();
        self.frequency_max = availability;
        self
    }

    pub fn with_frequency_current(mut self, availability: Availability) -> Self {
        self.frequency_current = availability;
        self
    }

    pub fn with_frequency_max(mut self, availability: Availability) -> Self {
        self.frequency_max = availability;
        self
    }
}

/// What a platform discovered about this machine's CPU configuration.
///
/// The counts are `Option` on purpose. A kernel that exposes no topology
/// information, or a hypervisor that hides it, leaves PULSE genuinely unable
/// to say how many physical cores exist — and `cpu.count.physical` is then
/// published as unavailable with a reason, never as a guess derived by
/// dividing the logical count by two.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CpuTopology {
    /// Every logical processor, **sorted by ordinal** and without duplicates.
    ///
    /// Ordinals may have gaps: a machine can legitimately present
    /// `0-3,8-11` when CPUs are offlined or hot-unplugged.
    logical: Vec<LogicalProcessor>,
    /// Distinct `(package, core)` pairs, when the platform could count them.
    pub physical_core_count: Option<u32>,
    /// Distinct processor packages, when the platform could count them.
    pub package_count: Option<u32>,
    /// The packages PULSE can name individually, **sorted by index** and
    /// without duplicates.
    ///
    /// Empty on a platform that exposes no package topology, and empty is not
    /// the same as `package_count == Some(0)`: a machine can know it has one
    /// socket without PULSE being able to address it as a measurement source.
    packages: Vec<CpuPackage>,
}

impl CpuTopology {
    /// Builds a topology, sorting the logical processors and dropping any
    /// duplicate ordinal.
    ///
    /// Sorting here rather than trusting the caller is what guarantees
    /// `docs/metrics/identifiers.md`'s promise that the catalog order is
    /// numeric — `logical-2` before `logical-10`, never lexicographic.
    pub fn new(
        mut logical: Vec<LogicalProcessor>,
        physical_core_count: Option<u32>,
        package_count: Option<u32>,
    ) -> Self {
        logical.sort_by_key(|processor| processor.id);
        logical.dedup_by_key(|processor| processor.id);

        Self {
            logical,
            physical_core_count,
            package_count,
            packages: Vec::new(),
        }
    }

    /// Attaches the packages this machine can be measured per-socket on.
    ///
    /// Kept out of [`CpuTopology::new`] so that adding a per-package
    /// measurement never changes the signature every platform already calls,
    /// and so a platform with no package-level sensor simply does not call it.
    ///
    /// Sorted and deduplicated here rather than trusted from the caller: a
    /// duplicate index would reach the engine as a colliding metric reference
    /// and get the whole provider rejected.
    pub fn with_packages(mut self, mut packages: Vec<CpuPackage>) -> Self {
        packages.sort_by_key(|package| package.id);
        packages.dedup_by_key(|package| package.id);

        self.packages = packages;
        self
    }

    /// The packages that can be addressed individually, in ascending order.
    pub fn packages(&self) -> &[CpuPackage] {
        &self.packages
    }

    /// The logical processors, in ascending ordinal order.
    pub fn logical(&self) -> &[LogicalProcessor] {
        &self.logical
    }

    /// How many logical processors were discovered.
    ///
    /// Always known — it is the length of a list PULSE enumerated itself,
    /// unlike the core and package counts, which depend on the platform
    /// exposing topology information.
    pub fn logical_count(&self) -> u32 {
        self.logical.len() as u32
    }

    /// Whether this ordinal is present.
    pub fn contains(&self, id: LogicalId) -> bool {
        self.logical
            .binary_search_by_key(&id, |processor| processor.id)
            .is_ok()
    }

    /// Looks up one logical processor.
    pub fn get(&self, id: LogicalId) -> Option<&LogicalProcessor> {
        self.logical
            .binary_search_by_key(&id, |processor| processor.id)
            .ok()
            .map(|index| &self.logical[index])
    }

    /// Whether any logical processor was discovered at all.
    pub fn is_empty(&self) -> bool {
        self.logical.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_logical_id_builds_the_canonical_source() {
        assert_eq!(LogicalId::new(0).source_id().as_str(), "cpu:logical-0");
        assert_eq!(LogicalId::new(7).source_id().as_str(), "cpu:logical-7");
        assert_eq!(LogicalId::new(127).source_id().as_str(), "cpu:logical-127");
    }

    #[test]
    fn the_source_round_trips_through_its_ordinal() {
        for ordinal in [0, 1, 9, 10, 31, 255, 4095] {
            let id = LogicalId::new(ordinal);
            assert_eq!(LogicalId::from_source(&id.source_id()), Some(id));
        }
    }

    #[test]
    fn only_logical_processor_sources_yield_an_ordinal() {
        let rejected = [
            "cpu:system",
            "memory:system",
            "gpu:logical-0",
            "cpu:logical-",
            "cpu:logical-abc",
            "cpu:logical-1x",
            "cpu:0",
        ];

        for source in rejected {
            let source = SourceId::new(source).expect("valid shape");
            assert_eq!(
                LogicalId::from_source(&source),
                None,
                "'{source}' must not parse as a logical processor"
            );
        }
    }

    #[test]
    fn labels_read_the_way_every_other_tool_names_them() {
        assert_eq!(LogicalId::new(0).label(), "CPU 0");
        assert_eq!(LogicalId::new(31).label(), "CPU 31");
    }

    #[test]
    fn topology_sorts_numerically_not_lexicographically() {
        // The ordering bug this guards against would list logical-1,
        // logical-10, logical-11, logical-2 in the UI.
        let topology = CpuTopology::new(
            [10, 2, 1, 11, 0]
                .map(|ordinal| LogicalProcessor::available(LogicalId::new(ordinal)))
                .to_vec(),
            None,
            None,
        );

        let ordinals: Vec<u32> = topology
            .logical()
            .iter()
            .map(|processor| processor.id.get())
            .collect();

        assert_eq!(ordinals, [0, 1, 2, 10, 11]);
    }

    #[test]
    fn topology_tolerates_non_contiguous_ordinals() {
        // `0-3,8-11` is a legitimate online CPU list.
        let ordinals = [0, 1, 2, 3, 8, 9, 10, 11];
        let topology = CpuTopology::new(
            ordinals
                .map(|ordinal| LogicalProcessor::available(LogicalId::new(ordinal)))
                .to_vec(),
            Some(4),
            Some(1),
        );

        assert_eq!(topology.logical_count(), 8);
        assert!(topology.contains(LogicalId::new(8)));
        assert!(!topology.contains(LogicalId::new(4)));
        assert!(topology.get(LogicalId::new(11)).is_some());
        assert!(topology.get(LogicalId::new(12)).is_none());
    }

    #[test]
    fn a_duplicate_ordinal_is_kept_once() {
        let topology = CpuTopology::new(
            [0, 1, 1, 0]
                .map(|ordinal| LogicalProcessor::available(LogicalId::new(ordinal)))
                .to_vec(),
            None,
            None,
        );

        // A duplicate would otherwise reach the engine as a colliding metric
        // reference and reject the whole provider.
        assert_eq!(topology.logical_count(), 2);
    }

    #[test]
    fn unknown_counts_stay_unknown_rather_than_being_guessed() {
        let topology = CpuTopology::new(
            vec![LogicalProcessor::available(LogicalId::new(0))],
            None,
            None,
        );

        assert_eq!(topology.logical_count(), 1);
        assert_eq!(topology.physical_core_count, None);
        assert_eq!(topology.package_count, None);
    }

    #[test]
    fn frequency_availability_is_per_metric_not_per_processor() {
        let processor = LogicalProcessor::available(LogicalId::new(17))
            .with_frequency_max(Availability::unsupported("no cpuinfo_max_freq"));

        // Usage and current frequency survive a missing maximum.
        assert!(processor.usage.is_available());
        assert!(processor.frequency_current.is_available());
        assert!(!processor.frequency_max.is_available());
    }

    #[test]
    fn an_empty_topology_is_representable() {
        let topology = CpuTopology::default();

        assert!(topology.is_empty());
        assert_eq!(topology.logical_count(), 0);
        assert!(!topology.contains(LogicalId::new(0)));
    }
}
