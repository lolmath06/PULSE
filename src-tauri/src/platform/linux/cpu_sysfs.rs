//! Linux CPU topology and frequency, read from `/sys/devices/system/cpu/`.
//!
//! Everything here reads files with `std::fs` and nothing else. **No
//! subprocess, ever** — not `lscpu`, not `cat`, not `grep`, not `cpupower`.
//! Spawning a process per metric would be slower, would depend on tools that
//! may not be installed, would parse output that changes with locale and
//! version, and would give PULSE a shell injection surface it has no reason to
//! have.
//!
//! None of it needs root: every file used here is world-readable on a stock
//! Fedora system.
//!
//! # The files
//!
//! ```text
//! /sys/devices/system/cpu/
//! ├── online                                 which CPUs exist right now
//! └── cpuN/
//!     ├── topology/
//!     │   ├── core_id                        core index within the package
//!     │   └── physical_package_id            which socket this core is in
//!     └── cpufreq/
//!         ├── scaling_cur_freq               current clock, kHz
//!         └── cpuinfo_max_freq               hardware maximum, kHz
//! ```
//!
//! **Nothing here assumes a file exists.** `cpufreq/` is absent under many
//! hypervisors and on kernels without a frequency driver; `topology/` is
//! absent on some architectures and virtual machines. Each is handled
//! independently so that one missing directory costs exactly one metric.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::metrics::model::{Availability, MetricError, MetricErrorCode};
use crate::metrics::wellknown::cpu::{kilohertz_to_hertz, LogicalId};

use super::cpu_list::parse_cpu_list;

/// Root of the kernel's CPU information.
const CPU_ROOT: &str = "/sys/devices/system/cpu";

/// Reads a sysfs file, mapping the failure to a classified error.
///
/// The distinction matters: a missing file means the kernel does not expose
/// the capability here, while a permission error means it does and PULSE
/// cannot reach it. Collapsing them would tell a user to try `sudo` for
/// something that would not work either way.
fn read_sysfs(path: PathBuf) -> Result<String, MetricError> {
    fs::read_to_string(&path).map_err(|error| {
        let code = match error.kind() {
            io::ErrorKind::NotFound => MetricErrorCode::NotDetected,
            io::ErrorKind::PermissionDenied => MetricErrorCode::PermissionDenied,
            _ => MetricErrorCode::Io,
        };

        MetricError::new(code, format!("could not read {}: {error}", path.display()))
    })
}

/// Parses a sysfs file containing a single unsigned integer.
pub fn parse_sysfs_u64(content: &str) -> Result<u64, MetricError> {
    content.trim().parse::<u64>().map_err(|error| {
        MetricError::new(
            MetricErrorCode::Parse,
            format!("'{}' is not an integer: {error}", content.trim()),
        )
    })
}

fn cpu_path(id: LogicalId, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{CPU_ROOT}/cpu{}/{suffix}", id.get()))
}

// --- logical processor discovery -----------------------------------------

/// Lists the logical processors the kernel currently reports as online.
///
/// Reads `/sys/devices/system/cpu/online`, which is the kernel's own answer
/// and therefore the one that matches `/proc/stat`'s `cpuN` lines.
pub fn read_online_cpus() -> Result<Vec<LogicalId>, MetricError> {
    parse_cpu_list(&read_sysfs(PathBuf::from(format!("{CPU_ROOT}/online")))?)
}

// --- topology -------------------------------------------------------------

/// One logical processor's position in the CPU hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcessorLocation {
    /// `physical_package_id` — which socket.
    pub package_id: u32,
    /// `core_id` — which core **within that package**.
    pub core_id: u32,
}

/// Counts distinct physical cores and packages from per-processor locations.
///
/// # Why the pair, and not `core_id` alone
///
/// `core_id` is only unique *within a package*. A dual-socket machine has a
/// `core_id` 0 in package 0 and another in package 1, and counting bare
/// `core_id` values would merge them — reporting a 2×32-core server as having
/// 32 cores. Counting distinct `(package_id, core_id)` pairs is what makes the
/// figure correct on one socket and on eight.
///
/// Returns `(physical_cores, packages)`. Both are `None` when no location
/// could be read at all, so the metrics are published as not-detected rather
/// than as a fabricated count.
pub fn count_cores_and_packages(locations: &[ProcessorLocation]) -> (Option<u32>, Option<u32>) {
    if locations.is_empty() {
        return (None, None);
    }

    let cores: BTreeSet<ProcessorLocation> = locations.iter().copied().collect();
    let packages: BTreeSet<u32> = locations.iter().map(|l| l.package_id).collect();

    (Some(cores.len() as u32), Some(packages.len() as u32))
}

/// Reads one logical processor's `topology/` entries.
///
/// Both files must be present: a `core_id` without a `physical_package_id`
/// cannot be counted correctly, and guessing package 0 would silently merge
/// cores on a multi-socket machine.
pub fn read_processor_location(id: LogicalId) -> Result<ProcessorLocation, MetricError> {
    let package_id = parse_sysfs_u64(&read_sysfs(cpu_path(id, "topology/physical_package_id"))?)?;
    let core_id = parse_sysfs_u64(&read_sysfs(cpu_path(id, "topology/core_id"))?)?;

    Ok(ProcessorLocation {
        package_id: package_id as u32,
        core_id: core_id as u32,
    })
}

/// Reads the topology of every given logical processor.
///
/// Processors whose topology files are missing are skipped rather than
/// failing the whole read: a partially-described machine still yields a
/// correct count of what *is* described, and a machine that describes nothing
/// yields `(None, None)`.
pub fn read_topology_counts(processors: &[LogicalId]) -> (Option<u32>, Option<u32>) {
    count_cores_and_packages(&read_processor_locations(processors))
}

/// Reads every given processor's location, skipping the ones that have none.
pub fn read_processor_locations(processors: &[LogicalId]) -> Vec<ProcessorLocation> {
    processors
        .iter()
        .filter_map(|&id| read_processor_location(id).ok())
        .collect()
}

/// The distinct package indices these locations name, in ascending order.
///
/// **The kernel's own `physical_package_id`**, which is what `coretemp` labels
/// its channels with and what `cpu.count.package` is counted from. PULSE never
/// invents a second numbering for thermals: a machine's package 1 is package 1
/// everywhere, or the two would eventually describe different sockets.
pub fn package_ids(locations: &[ProcessorLocation]) -> Vec<u32> {
    let ids: BTreeSet<u32> = locations
        .iter()
        .map(|location| location.package_id)
        .collect();

    ids.into_iter().collect()
}

// --- frequency ------------------------------------------------------------

/// `cpufreq` attribute holding the current clock, in kHz.
///
/// Chosen over `cpuinfo_cur_freq`, which is the same figure but is
/// root-readable only on many drivers — PULSE must work unprivileged.
///
/// With `intel_pstate` or `amd-pstate`, reading this file asks the hardware
/// for its current operating point at that moment, which is the most accurate
/// answer available without elevation. With the older `acpi-cpufreq` governor
/// it is the frequency the governor last requested. Both are "what the OS
/// reports", which is exactly what `cpu.frequency.current` promises.
const SCALING_CUR_FREQ: &str = "cpufreq/scaling_cur_freq";

/// `cpufreq` attribute holding the hardware maximum clock, in kHz.
///
/// Deliberately **not** `scaling_max_freq`, which is the current policy
/// ceiling: a laptop in a power-saving profile reports a scaling maximum far
/// below what the chip can do, and publishing that as "maximum frequency"
/// would be wrong in a way the user can see. When this file is absent, PULSE
/// reports the metric unsupported rather than substituting the policy value.
/// See `metrics::wellknown::cpu::frequency::MaxFrequencySource`.
const CPUINFO_MAX_FREQ: &str = "cpufreq/cpuinfo_max_freq";

/// Reads a `cpufreq` attribute and converts it to hertz.
///
/// A zero reading, an overflow, or unparseable content all yield an error
/// rather than a published `0 Hz`.
fn read_frequency_hertz(id: LogicalId, attribute: &str) -> Result<u64, MetricError> {
    let kilohertz = parse_sysfs_u64(&read_sysfs(cpu_path(id, attribute))?)?;

    kilohertz_to_hertz(kilohertz).ok_or_else(|| {
        MetricError::new(
            MetricErrorCode::Parse,
            format!(
                "cpu{} reported an unusable frequency of {kilohertz} kHz",
                id.get()
            ),
        )
    })
}

/// Reads the current clock of one logical processor, in hertz.
pub fn read_current_frequency(id: LogicalId) -> Result<u64, MetricError> {
    read_frequency_hertz(id, SCALING_CUR_FREQ)
}

/// Reads the hardware maximum clock of one logical processor, in hertz.
pub fn read_max_frequency(id: LogicalId) -> Result<u64, MetricError> {
    read_frequency_hertz(id, CPUINFO_MAX_FREQ)
}

/// Turns a frequency read failure into the availability that describes it.
///
/// A missing `cpufreq` directory is `unsupported` — the kernel has no
/// frequency driver for this processor and never will during this boot — as
/// opposed to `notDetected`, which would suggest absent hardware.
pub fn frequency_availability(error: &MetricError) -> Availability {
    match error.code {
        MetricErrorCode::NotDetected => Availability::unsupported(format!(
            "no cpufreq interface for this logical processor ({})",
            error.message
        )),
        MetricErrorCode::PermissionDenied => Availability::permission_denied(error.message.clone()),
        MetricErrorCode::Io => Availability::temporarily_unavailable(error.message.clone()),
        _ => Availability::provider_error(error.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location(package_id: u32, core_id: u32) -> ProcessorLocation {
        ProcessorLocation {
            package_id,
            core_id,
        }
    }

    #[test]
    fn parses_the_integers_sysfs_writes() {
        assert_eq!(parse_sysfs_u64("0\n").expect("valid"), 0);
        assert_eq!(parse_sysfs_u64("5600000\n").expect("valid"), 5_600_000);
        assert_eq!(parse_sysfs_u64("  42  ").expect("valid"), 42);
    }

    #[test]
    fn rejects_non_integer_sysfs_content() {
        for content in ["", "abc", "1.5", "-1", "0x10", "1 2"] {
            let error = parse_sysfs_u64(content)
                .err()
                .unwrap_or_else(|| panic!("'{content}' must be rejected"));
            assert_eq!(error.code, MetricErrorCode::Parse);
        }
    }

    // --- physical core counting ------------------------------------------

    #[test]
    fn counts_one_physical_core_per_distinct_pair() {
        // Four logical processors, no SMT: four cores.
        let locations = [
            location(0, 0),
            location(0, 1),
            location(0, 2),
            location(0, 3),
        ];

        assert_eq!(count_cores_and_packages(&locations), (Some(4), Some(1)));
    }

    #[test]
    fn smt_siblings_collapse_into_one_physical_core() {
        // Eight logical processors on four cores — the Hyper-Threading case.
        // Counting logical processors here would double the core count.
        let locations = [
            location(0, 0),
            location(0, 0),
            location(0, 1),
            location(0, 1),
            location(0, 2),
            location(0, 2),
            location(0, 3),
            location(0, 3),
        ];

        assert_eq!(count_cores_and_packages(&locations), (Some(4), Some(1)));
    }

    #[test]
    fn identical_core_ids_in_different_packages_are_never_merged() {
        // The bug this guards against: a dual-socket machine where each socket
        // numbers its cores 0..3 would otherwise be reported as having four
        // cores instead of eight.
        let locations = [
            location(0, 0),
            location(0, 1),
            location(0, 2),
            location(0, 3),
            location(1, 0),
            location(1, 1),
            location(1, 2),
            location(1, 3),
        ];

        assert_eq!(count_cores_and_packages(&locations), (Some(8), Some(2)));
    }

    #[test]
    fn counts_several_packages_with_smt() {
        // Two sockets, four cores each, two threads per core: 16 logical.
        let locations: Vec<ProcessorLocation> = (0..2)
            .flat_map(|package| (0..4).flat_map(move |core| [location(package, core); 2]))
            .collect();

        assert_eq!(locations.len(), 16);
        assert_eq!(count_cores_and_packages(&locations), (Some(8), Some(2)));
    }

    #[test]
    fn handles_a_hybrid_layout_with_sparse_core_ids() {
        // A real Intel hybrid CPU numbers P-core ids 0,4,8… and E-core ids
        // consecutively. The counting must not care about the numbering.
        let mut locations: Vec<ProcessorLocation> = Vec::new();
        for core in [0, 4, 8, 12] {
            locations.push(location(0, core));
            locations.push(location(0, core)); // SMT sibling
        }
        for core in 16..24 {
            locations.push(location(0, core)); // E-cores, one thread each
        }

        // 4 P-cores (8 threads) + 8 E-cores = 12 physical cores, 16 logical.
        assert_eq!(locations.len(), 16);
        assert_eq!(count_cores_and_packages(&locations), (Some(12), Some(1)));
    }

    #[test]
    fn a_machine_that_exposes_no_topology_reports_nothing_rather_than_guessing() {
        assert_eq!(count_cores_and_packages(&[]), (None, None));
    }

    #[test]
    fn a_single_processor_machine_is_counted_correctly() {
        assert_eq!(
            count_cores_and_packages(&[location(0, 0)]),
            (Some(1), Some(1))
        );
    }

    #[test]
    fn non_contiguous_package_ids_are_counted_by_identity_not_by_maximum() {
        // Some firmware numbers packages 0 and 2. Taking max+1 would say three.
        let locations = [location(0, 0), location(2, 0)];

        assert_eq!(count_cores_and_packages(&locations), (Some(2), Some(2)));
    }

    // --- frequency failure classification ---------------------------------

    #[test]
    fn a_missing_cpufreq_directory_is_unsupported_not_undetected_hardware() {
        // "Your CPU was not detected" would be alarming and wrong; the CPU is
        // fine, the kernel simply exposes no frequency interface for it.
        let availability = frequency_availability(&MetricError::new(
            MetricErrorCode::NotDetected,
            "could not read /sys/.../cpufreq/scaling_cur_freq: No such file",
        ));

        assert_eq!(availability.status_str(), "unsupported");
    }

    #[test]
    fn frequency_failures_keep_their_distinct_meanings() {
        let cases = [
            (MetricErrorCode::NotDetected, "unsupported"),
            (MetricErrorCode::PermissionDenied, "permissionDenied"),
            (MetricErrorCode::Io, "temporarilyUnavailable"),
            (MetricErrorCode::Parse, "providerError"),
        ];

        for (code, expected) in cases {
            let availability = frequency_availability(&MetricError::new(code, "detail"));
            assert_eq!(availability.status_str(), expected, "for {code:?}");
            assert!(!availability.is_available());
        }
    }

    // --- host tests: invariants only, never specific values ---------------

    #[cfg(target_os = "linux")]
    #[test]
    fn this_host_reports_at_least_one_online_cpu() {
        let online = read_online_cpus().expect("/sys/devices/system/cpu/online must be readable");

        assert!(!online.is_empty());
        // The list is sorted, which the catalog ordering depends on.
        let mut sorted = online.clone();
        sorted.sort();
        assert_eq!(online, sorted);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn this_hosts_core_count_never_exceeds_its_logical_count() {
        let online = read_online_cpus().expect("readable");
        let (cores, packages) = read_topology_counts(&online);

        if let Some(cores) = cores {
            assert!(cores >= 1);
            assert!(
                cores <= online.len() as u32,
                "a machine cannot have more physical cores than logical processors"
            );
        }
        if let Some(packages) = packages {
            assert!(packages >= 1);
            assert!(packages <= cores.unwrap_or(u32::MAX));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn any_frequency_this_host_reports_is_plausible() {
        let online = read_online_cpus().expect("readable");

        for id in online {
            // Absent is a perfectly good answer; a nonsensical number is not.
            if let Ok(hertz) = read_current_frequency(id) {
                assert!(
                    (100_000_000..=20_000_000_000).contains(&hertz),
                    "cpu{id} reported an implausible current frequency of {hertz} Hz"
                );
            }
            if let Ok(hertz) = read_max_frequency(id) {
                assert!(
                    (100_000_000..=20_000_000_000).contains(&hertz),
                    "cpu{id} reported an implausible maximum frequency of {hertz} Hz"
                );
            }
        }
    }
}

#[cfg(test)]
mod package_tests {
    use super::*;

    fn location(package_id: u32, core_id: u32) -> ProcessorLocation {
        ProcessorLocation {
            package_id,
            core_id,
        }
    }

    #[test]
    fn lists_each_package_once_in_ascending_order() {
        let locations = [
            location(1, 0),
            location(0, 0),
            location(1, 1),
            location(0, 1),
            location(0, 0),
        ];

        assert_eq!(package_ids(&locations), [0, 1]);
    }

    #[test]
    fn the_package_ids_are_the_kernel_numbering_not_a_dense_range() {
        // A machine that numbers its sockets 0 and 2 keeps those numbers: a
        // thermal reading labelled "Package id 2" must match the source
        // `cpu:package-2`, not a renumbered `cpu:package-1`.
        let locations = [location(0, 0), location(2, 0)];

        assert_eq!(package_ids(&locations), [0, 2]);
    }

    #[test]
    fn no_locations_means_no_packages_rather_than_one_assumed_package() {
        assert!(package_ids(&[]).is_empty());
    }

    #[test]
    fn the_package_list_and_the_package_count_agree() {
        let locations = [location(0, 0), location(0, 1), location(1, 0)];
        let (_, count) = count_cores_and_packages(&locations);

        assert_eq!(count, Some(package_ids(&locations).len() as u32));
    }
}
