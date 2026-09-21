//! CPU package temperature on Fedora, and the labels that must not be confused.
//!
//! A CPU's thermal sensors arrive through `hwmon` like every other sensor (see
//! [`super::hwmon`]), but *which channel means what* is driver-specific, and
//! getting it wrong produces a number that is plausible, wrong and impossible
//! for a user to catch.
//!
//! # Intel — `coretemp`
//!
//! ```text
//! temp1_label  Package id 0     ← the package sensor
//! temp2_label  Core 0
//! temp6_label  Core 4
//! temp10_label Core 8
//! ```
//!
//! `Package id N` maps to `cpu:package-N`, using **the kernel's own package
//! numbering** — the same `physical_package_id` that `cpu.count.package` is
//! derived from, so the two can never come to describe different things.
//!
//! `Core N` is deliberately never used. Averaging the cores would produce a
//! figure PULSE invented, and one that reads *lower* than the truth exactly when
//! it matters: a single core boosting hard is what throttles a machine, and a
//! mean hides it.
//!
//! # Intel — `peci_cputemp`
//!
//! A server-oriented driver that reaches the CPU over PECI and labels more
//! channels, most of which are **not** the current temperature:
//!
//! | Label | What it is | Publishable as the CPU temperature |
//! |---|---|---|
//! | `Die` | the die's current temperature | **yes** |
//! | `DTS` | a reading relative to the throttle point | no |
//! | `Tcontrol` | the fan-control target | no |
//! | `Tthrottle` | where throttling begins | no |
//! | `Tjmax` | the maximum junction temperature | no |
//!
//! `Tjmax` is a constant of the part — typically 100 °C. Publishing it as the
//! current temperature would tell every user their idle laptop is at 100 °C,
//! which is the single most common way a monitoring tool gets this wrong.
//!
//! # AMD — `k10temp`
//!
//! ```text
//! Tctl     a control value, offset on many parts so a fan curve behaves
//! Tdie     the die's actual temperature
//! Tccd1…   per-chiplet sensors
//! ```
//!
//! **`Tdie` only.** On many Ryzen and Threadripper parts `Tctl` carries a
//! deliberate offset above the real die temperature so that stock coolers spin
//! up sooner; the kernel documents the two as different things. Publishing
//! `Tctl` as the CPU temperature would overstate it by a fixed amount, which no
//! user could detect and every comparison would inherit. A part exposing only
//! `Tctl` therefore publishes **nothing**, with the reason — `unsupported` is a
//! better answer than a number with the wrong meaning.
//!
//! `Tccd*` are chiplet sensors, not package sensors, and are not used either.
//!
//! # Which package a device belongs to
//!
//! `coretemp` says so in the label, and a dual-socket machine gets it right for
//! free. `k10temp` and `peci_cputemp` do not label a package index, so PULSE
//! attributes their reading to package 0 **only when the machine has exactly one
//! package and exactly one such device**. With two sockets there is no way to
//! tell which device is which without inventing a mapping out of probe order —
//! and a socket's temperature attributed to the other socket is exactly the kind
//! of quiet wrongness this module exists to prevent.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::metrics::model::Availability;
use crate::metrics::wellknown::cpu::{CpuPackage, PackageId};

use super::hwmon::{HwmonChannel, HwmonDevice};

/// The Intel driver that labels a package sensor directly.
pub const CORETEMP: &str = "coretemp";
/// The AMD driver.
pub const K10TEMP: &str = "k10temp";
/// The Intel PECI driver used on server platforms.
pub const PECI_CPUTEMP: &str = "peci_cputemp";

/// The `coretemp` label prefix identifying a package channel.
const PACKAGE_LABEL_PREFIX: &str = "package id ";
/// The `k10temp` label carrying the real die temperature.
const K10TEMP_DIE_LABEL: &str = "tdie";
/// The `peci_cputemp` label carrying the die's current temperature.
const PECI_DIE_LABEL: &str = "die";

/// Where each package's temperature is read from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CpuPackageSensors {
    /// Package index to `tempN_input` path. Resolved once at startup; the file
    /// is re-read at sample time.
    sensors: BTreeMap<u32, PathBuf>,
    /// Why no sensor was mapped, when the reason is more specific than "this
    /// machine exposes none" — several CPU hwmon devices with no package label
    /// between them, or a driver exposing only a control value.
    reason: Option<String>,
}

impl CpuPackageSensors {
    /// The sensor file of one package, if there is one.
    pub fn sensor(&self, id: PackageId) -> Option<&PathBuf> {
        self.sensors.get(&id.get())
    }

    /// Whether nothing at all was mapped.
    pub fn is_empty(&self) -> bool {
        self.sensors.is_empty()
    }

    /// Describes one package for the catalog.
    ///
    /// A package with no sensor keeps its definition and explains itself, so a
    /// dashboard built on a machine that has one still resolves on a machine
    /// that does not.
    pub fn describe(&self, id: PackageId) -> CpuPackage {
        match self.sensor(id) {
            Some(_) => CpuPackage::available(id),
            None => CpuPackage::unavailable(
                id,
                Availability::unsupported(self.reason.clone().unwrap_or_else(|| {
                    "no CPU thermal driver on this machine reports a package temperature"
                        .to_string()
                })),
            ),
        }
    }
}

/// Maps the CPU thermal sensors of a set of hwmon devices.
///
/// `package_count` is the topology's own answer, and is used only to decide
/// whether an unlabelled driver's single reading can safely be attributed to
/// package 0.
pub fn map_sensors(devices: &[HwmonDevice], package_count: Option<u32>) -> CpuPackageSensors {
    let mut sensors: BTreeMap<u32, PathBuf> = BTreeMap::new();

    // Intel `coretemp`, which labels the package index itself. Several devices
    // on a multi-socket machine each label their own package, so this is
    // correct for one socket and for eight.
    for device in devices.iter().filter(|device| device.name == CORETEMP) {
        for channel in device.channels("temp") {
            if let Some(index) = package_index(&channel) {
                sensors.entry(index).or_insert(channel.input);
            }
        }
    }

    if !sensors.is_empty() {
        return CpuPackageSensors {
            sensors,
            reason: None,
        };
    }

    // The unlabelled drivers. One device, one package, one reading — or
    // nothing.
    let unlabelled: Vec<&HwmonDevice> = devices
        .iter()
        .filter(|device| device.name == K10TEMP || device.name == PECI_CPUTEMP)
        .collect();

    let [device] = unlabelled[..] else {
        if unlabelled.len() > 1 {
            return CpuPackageSensors {
                sensors,
                reason: Some(format!(
                    "this machine exposes {} CPU thermal devices that do not label a package, \
                     and PULSE will not guess which socket each one measures",
                    unlabelled.len()
                )),
            };
        }

        return CpuPackageSensors::default();
    };

    if matches!(package_count, Some(count) if count > 1) {
        return CpuPackageSensors {
            sensors,
            reason: Some(format!(
                "the {} driver does not label which package its sensor belongs to, and this \
                 machine has more than one package",
                device.name
            )),
        };
    }

    let label = match device.name.as_str() {
        K10TEMP => K10TEMP_DIE_LABEL,
        _ => PECI_DIE_LABEL,
    };

    match find_labelled(device, label) {
        Some(input) => {
            sensors.insert(0, input);
            CpuPackageSensors {
                sensors,
                reason: None,
            }
        }
        None => CpuPackageSensors {
            sensors,
            reason: Some(missing_die_reason(&device.name)),
        },
    }
}

/// Maps the CPU thermal sensors of the running system.
pub fn discover(package_count: Option<u32>) -> CpuPackageSensors {
    map_sensors(&super::hwmon::discover(), package_count)
}

/// `Package id 3` → `3`.
///
/// Matched case-insensitively, and only on the package prefix: `Core 3` yields
/// `None`, which is the whole point.
fn package_index(channel: &HwmonChannel) -> Option<u32> {
    channel
        .label
        .as_deref()?
        .trim()
        .to_ascii_lowercase()
        .strip_prefix(PACKAGE_LABEL_PREFIX)?
        .trim()
        .parse::<u32>()
        .ok()
}

/// The input of the channel carrying this exact label.
fn find_labelled(device: &HwmonDevice, label: &str) -> Option<PathBuf> {
    device
        .channels("temp")
        .into_iter()
        .find(|channel| {
            channel
                .label
                .as_deref()
                .is_some_and(|found| found.trim().eq_ignore_ascii_case(label))
        })
        .map(|channel| channel.input)
}

/// Why a driver that is present still reports no usable package temperature.
fn missing_die_reason(driver: &str) -> String {
    match driver {
        K10TEMP => format!(
            "the {K10TEMP} driver on this processor reports no Tdie channel; its Tctl value is a \
             control figure carrying a deliberate offset on many parts, and publishing it as the \
             CPU temperature would overstate it"
        ),
        other => format!(
            "the {other} driver on this processor reports no die temperature channel; its other \
             channels are thermal limits and control targets rather than the current temperature"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::linux::hwmon::fixtures::HwmonTree;
    use crate::platform::linux::hwmon::{self, read_temperature_celsius};

    /// The reference machine's `coretemp`, abbreviated: one package channel
    /// among many core channels.
    fn intel_tree(tree: &HwmonTree, index: u32, package: u32) {
        let device = tree.device(index, CORETEMP);
        device
            .input("temp", 1, "93000")
            .label("temp", 1, &format!("Package id {package}"))
            .input("temp", 2, "73000")
            .label("temp", 2, "Core 0")
            .input("temp", 6, "72000")
            .label("temp", 6, "Core 4")
            .input("temp", 10, "91000")
            .label("temp", 10, "Core 8")
            .attribute("temp1_crit", "100000")
            .attribute("temp1_max", "100000");
    }

    #[test]
    fn maps_the_intel_package_channel_and_ignores_the_cores() {
        let tree = HwmonTree::new("coretemp");
        intel_tree(&tree, 6, 0);

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(1));

        let input = sensors.sensor(PackageId::new(0)).expect("mapped");
        assert!(input.ends_with("temp1_input"));
        assert_eq!(read_temperature_celsius(input).expect("read"), 93.0);
        assert_eq!(sensors.sensor(PackageId::new(1)), None);
    }

    #[test]
    fn a_core_channel_is_never_published_as_the_package() {
        // `Core 0` at 73 °C sits right next to `Package id 0` at 93 °C. Reading
        // the wrong one understates the machine by twenty degrees.
        let tree = HwmonTree::new("coretemp-cores");
        intel_tree(&tree, 0, 0);

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(1));
        let input = sensors.sensor(PackageId::new(0)).expect("mapped");

        assert_eq!(read_temperature_celsius(input).expect("read"), 93.0);
    }

    #[test]
    fn a_thermal_limit_is_never_published_as_a_temperature() {
        // `temp1_crit` is 100 °C on this fixture and is not an `_input` file at
        // all, so it can never be selected.
        let tree = HwmonTree::new("coretemp-limits");
        intel_tree(&tree, 0, 0);

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(1));
        let input = sensors.sensor(PackageId::new(0)).expect("mapped");

        assert!(input.ends_with("temp1_input"));
        assert_ne!(read_temperature_celsius(input).expect("read"), 100.0);
    }

    #[test]
    fn two_sockets_map_to_two_packages_by_their_labels() {
        let tree = HwmonTree::new("coretemp-dual");
        intel_tree(&tree, 0, 0);
        let second = tree.device(1, CORETEMP);
        second
            .input("temp", 1, "51000")
            .label("temp", 1, "Package id 1");

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(2));

        assert_eq!(
            read_temperature_celsius(sensors.sensor(PackageId::new(0)).expect("package 0"))
                .expect("read"),
            93.0
        );
        assert_eq!(
            read_temperature_celsius(sensors.sensor(PackageId::new(1)).expect("package 1"))
                .expect("read"),
            51.0
        );
    }

    #[test]
    fn the_hwmon_numbering_does_not_decide_the_package_numbering() {
        // The same machine after a probe-order change: package 1's sensors are
        // now hwmon0. The labels still decide.
        let tree = HwmonTree::new("coretemp-renumbered");
        let first = tree.device(0, CORETEMP);
        first
            .input("temp", 1, "51000")
            .label("temp", 1, "Package id 1");
        intel_tree(&tree, 1, 0);

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(2));

        assert_eq!(
            read_temperature_celsius(sensors.sensor(PackageId::new(0)).expect("package 0"))
                .expect("read"),
            93.0,
            "package 0 must follow its label, not the hwmon index"
        );
    }

    #[test]
    fn labels_are_matched_without_regard_to_case_or_padding() {
        let tree = HwmonTree::new("coretemp-case");
        tree.device(0, CORETEMP)
            .input("temp", 1, "60000")
            .label("temp", 1, "  PACKAGE ID 0 ");

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(1));

        assert!(sensors.sensor(PackageId::new(0)).is_some());
    }

    // --- AMD --------------------------------------------------------------

    #[test]
    fn maps_the_amd_die_temperature() {
        let tree = HwmonTree::new("k10temp");
        tree.device(0, K10TEMP)
            .input("temp", 1, "45000")
            .label("temp", 1, "Tctl")
            .input("temp", 2, "38000")
            .label("temp", 2, "Tdie")
            .input("temp", 3, "37000")
            .label("temp", 3, "Tccd1");

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(1));
        let input = sensors.sensor(PackageId::new(0)).expect("mapped");

        // Tdie (38 °C), not Tctl (45 °C) and not the chiplet sensor.
        assert_eq!(read_temperature_celsius(input).expect("read"), 38.0);
    }

    #[test]
    fn an_amd_part_exposing_only_tctl_publishes_nothing() {
        // Tctl carries a deliberate offset on many parts. Publishing it would
        // overstate the CPU by a fixed amount nobody could detect.
        let tree = HwmonTree::new("k10temp-tctl");
        tree.device(0, K10TEMP)
            .input("temp", 1, "45000")
            .label("temp", 1, "Tctl");

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(1));

        assert!(sensors.is_empty());
        let package = sensors.describe(PackageId::new(0));
        let Availability::Unsupported { reason } = &package.temperature else {
            panic!("expected an unsupported package temperature");
        };
        assert!(reason.contains("Tdie"));
        assert!(reason.contains("offset"));
    }

    #[test]
    fn a_chiplet_sensor_is_not_a_package_sensor() {
        let tree = HwmonTree::new("k10temp-ccd");
        tree.device(0, K10TEMP)
            .input("temp", 1, "37000")
            .label("temp", 1, "Tccd1")
            .input("temp", 2, "38000")
            .label("temp", 2, "Tccd2");

        assert!(map_sensors(&hwmon::discover_in(&tree.root), Some(1)).is_empty());
    }

    // --- Intel PECI -------------------------------------------------------

    #[test]
    fn maps_the_peci_die_channel_and_none_of_the_limits() {
        let tree = HwmonTree::new("peci");
        tree.device(0, PECI_CPUTEMP)
            .input("temp", 1, "100000")
            .label("temp", 1, "Tjmax")
            .input("temp", 2, "95000")
            .label("temp", 2, "Tthrottle")
            .input("temp", 3, "90000")
            .label("temp", 3, "Tcontrol")
            .input("temp", 4, "48000")
            .label("temp", 4, "Die")
            .input("temp", 5, "-52000")
            .label("temp", 5, "DTS");

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(1));
        let input = sensors.sensor(PackageId::new(0)).expect("mapped");

        // The die (48 °C) — not Tjmax, which would report every idle machine
        // at 100 °C, and not the DTS offset.
        assert_eq!(read_temperature_celsius(input).expect("read"), 48.0);
    }

    #[test]
    fn a_peci_device_without_a_die_channel_publishes_nothing() {
        let tree = HwmonTree::new("peci-limits-only");
        tree.device(0, PECI_CPUTEMP)
            .input("temp", 1, "100000")
            .label("temp", 1, "Tjmax")
            .input("temp", 2, "90000")
            .label("temp", 2, "Tcontrol");

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(1));

        assert!(sensors.is_empty());
        let Availability::Unsupported { reason } = &sensors.describe(PackageId::new(0)).temperature
        else {
            panic!("expected an unsupported package temperature");
        };
        assert!(reason.contains("limits"));
    }

    #[test]
    fn an_unlabelled_driver_is_not_attributed_to_a_socket_on_a_multi_socket_machine() {
        // One k10temp device, two packages: which socket is it? Unanswerable,
        // and a wrong answer would be invisible.
        let tree = HwmonTree::new("k10temp-dual");
        tree.device(0, K10TEMP)
            .input("temp", 1, "38000")
            .label("temp", 1, "Tdie");

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(2));

        assert!(sensors.is_empty());
        assert!(sensors
            .describe(PackageId::new(0))
            .temperature
            .status_str()
            .eq("unsupported"));
    }

    #[test]
    fn several_unlabelled_devices_are_never_mapped_by_probe_order() {
        let tree = HwmonTree::new("k10temp-many");
        for index in 0..2 {
            tree.device(index, K10TEMP)
                .input("temp", 1, "38000")
                .label("temp", 1, "Tdie");
        }

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(2));

        assert!(sensors.is_empty());
        let Availability::Unsupported { reason } = &sensors.describe(PackageId::new(0)).temperature
        else {
            panic!("expected an unsupported package temperature");
        };
        assert!(reason.contains("will not guess"));
    }

    // --- neighbours -------------------------------------------------------

    #[test]
    fn an_unrelated_sensor_is_never_mistaken_for_the_cpu() {
        // The reference machine also has `acpitz`, `nvme`, `spd5118` and a
        // wireless card's thermal zone. None of them is the CPU, and `acpitz`
        // in particular reads a platform zone that can sit far from the die.
        let tree = HwmonTree::new("neighbours");
        tree.device(1, "acpitz").input("temp", 1, "97000");
        tree.device(3, "nvme")
            .input("temp", 1, "61850")
            .label("temp", 1, "Composite");
        tree.device(4, "spd5118").input("temp", 1, "68750");
        tree.device(7, "iwlwifi_1").input("temp", 1, "59000");

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(1));

        assert!(sensors.is_empty());
    }

    #[test]
    fn a_machine_with_no_thermal_driver_explains_itself() {
        let sensors = map_sensors(&[], Some(1));

        assert!(sensors.is_empty());
        let package = sensors.describe(PackageId::new(0));
        assert_eq!(package.id, PackageId::new(0));
        assert!(!package.temperature.is_available());
        let Availability::Unsupported { reason } = &package.temperature else {
            panic!("expected an unsupported package temperature");
        };
        assert!(reason.contains("no CPU thermal driver"));
    }

    #[test]
    fn a_mapped_package_is_described_as_available() {
        let tree = HwmonTree::new("describe");
        intel_tree(&tree, 0, 0);

        let sensors = map_sensors(&hwmon::discover_in(&tree.root), Some(1));

        assert!(sensors
            .describe(PackageId::new(0))
            .temperature
            .is_available());
        assert!(!sensors
            .describe(PackageId::new(1))
            .temperature
            .is_available());
    }
}
