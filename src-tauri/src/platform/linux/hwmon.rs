//! Reading sensors on Fedora, through `hwmon`.
//!
//! `hwmon` is the kernel's one common interface for temperatures, fans,
//! voltages and currents, whatever driver is behind them. Every sensor PULSE
//! reads on Linux — an Intel package temperature, an AMD die temperature, a GPU
//! and its fan — arrives through the files described here, which is why this
//! module knows nothing about CPUs or GPUs and the two modules that do know
//! share it.
//!
//! ```text
//! /sys/class/hwmon/hwmonN/
//! ├── name                  the driver: coretemp, k10temp, amdgpu, nouveau…
//! ├── device -> ../../…     the hardware this hwmon belongs to
//! ├── temp1_input           a temperature, in millidegrees Celsius
//! ├── temp1_label           what that channel measures, when the driver says
//! ├── temp2_input
//! └── fan1_input            a fan speed, in revolutions per minute
//! ```
//!
//! # `hwmonN` is not an identity
//!
//! The number is assigned in probe order. It changes when a module loads
//! earlier, when a driver is reloaded, and between boots — so `hwmon6` being
//! `coretemp` today says nothing about tomorrow. **Nothing here is ever
//! persisted, and no mapping is keyed on it.** What identifies a sensor is the
//! driver `name`, the hardware its `device` symlink resolves to, and the
//! channel's `tempN_label`.
//!
//! # Millidegrees in, Celsius out
//!
//! `tempN_input` is in **millidegrees Celsius**: `42000` means 42.0 °C. The
//! conversion happens here, once, at the platform edge — PULSE's contract
//! carries Celsius and nothing above this layer has any reason to learn that
//! millidegrees exist. `fanN_input` is already in RPM and needs none.
//!
//! # Read-only, without exception
//!
//! `hwmon` also exposes `pwmN`, `pwmN_enable` and assorted limit files, and
//! writing to them changes how the machine cools itself. **PULSE never opens
//! any of them for writing, and never opens the control files at all.** This
//! phase observes; it does not control. See `docs/metrics/thermals.md`.
//!
//! Nothing here needs root: `hwmon` inputs are world-readable on a stock
//! Fedora system.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::metrics::model::{Availability, MetricError, MetricErrorCode};

/// Where the kernel exposes hardware monitoring devices.
pub const HWMON_ROOT: &str = "/sys/class/hwmon";

/// The subdirectory a device's own `hwmon` nodes live under.
///
/// A PCI device that reports sensors carries them at
/// `<device>/hwmon/hwmonN`, which is how a GPU's temperature is tied to that
/// GPU rather than to whatever `hwmonN` happens to be numbered nearby.
pub const DEVICE_HWMON_SUBDIR: &str = "hwmon";

/// Millidegrees Celsius per degree.
const MILLIDEGREES_PER_DEGREE: f64 = 1000.0;

/// The lowest temperature a working sensor can report.
///
/// Absolute zero. Not a plausibility filter — a driver is entitled to report a
/// sensor reading PULSE finds surprising, and silently clamping it would hide a
/// real problem — but a physically impossible value is a failed read rather
/// than a measurement, and is refused as one.
const ABSOLUTE_ZERO_C: f64 = -273.15;

/// One hardware monitoring device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HwmonDevice {
    /// The `hwmonN` directory. Held to read channels from, **never stored**.
    pub path: PathBuf,
    /// The driver's name: `coretemp`, `k10temp`, `amdgpu`, `nouveau`…
    pub name: String,
    /// What the `device` symlink resolves to, when there is one.
    ///
    /// This is what ties a hwmon to a piece of hardware: a GPU's sensors
    /// resolve to its PCI device, and `coretemp`'s to a platform device.
    pub device: Option<PathBuf>,
}

/// One measurement channel of a hwmon device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HwmonChannel {
    /// The channel number, e.g. `1` for `temp1_input`.
    pub index: u32,
    /// The driver's own description of what this channel measures — `Package id
    /// 0`, `Tdie`, `junction`, `edge`. Absent on drivers that label nothing,
    /// which is common for a single-sensor device.
    pub label: Option<String>,
    /// The `*_input` file to read the value from.
    pub input: PathBuf,
}

impl HwmonDevice {
    /// Reads a hwmon directory, or `None` when it has no `name`.
    ///
    /// A directory without a name is not a hwmon device PULSE can identify, and
    /// identifying it is the whole point — the number in the path cannot.
    pub fn read(path: &Path) -> Option<Self> {
        let name = fs::read_to_string(path.join("name"))
            .ok()
            .map(|content| content.trim().to_string())
            .filter(|name| !name.is_empty())?;

        Some(Self {
            path: path.to_path_buf(),
            name,
            // `canonicalize` rather than `read_link`: the symlink is relative,
            // and the resolved path is what can be compared against a PCI
            // device path.
            device: fs::canonicalize(path.join("device")).ok(),
        })
    }

    /// The channels of one family, `temp` or `fan`, sorted by index.
    ///
    /// Sorted so that a caller falling back to "the first channel" gets the
    /// lowest-numbered one deterministically rather than whatever the directory
    /// iteration produced.
    pub fn channels(&self, family: &str) -> Vec<HwmonChannel> {
        let Ok(entries) = fs::read_dir(&self.path) else {
            return Vec::new();
        };

        let mut channels: Vec<HwmonChannel> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_str()?.to_string();
                let index = channel_index(&name, family)?;

                Some(HwmonChannel {
                    index,
                    label: self.label(family, index),
                    input: self.path.join(&name),
                })
            })
            .collect();

        channels.sort_by_key(|channel| channel.index);
        channels
    }

    /// One channel's label, when the driver publishes one.
    fn label(&self, family: &str, index: u32) -> Option<String> {
        fs::read_to_string(self.path.join(format!("{family}{index}_label")))
            .ok()
            .map(|content| content.trim().to_string())
            .filter(|label| !label.is_empty())
    }
}

/// Extracts `N` from `<family>N_input`.
///
/// Returns `None` for every other file, which is what keeps `temp1_crit`,
/// `temp1_max` and the `pwm*` control files out of the channel list — reading a
/// limit as if it were a measurement is the single most common way a monitoring
/// tool reports a laptop as running at 100 °C.
fn channel_index(file_name: &str, family: &str) -> Option<u32> {
    file_name
        .strip_prefix(family)?
        .strip_suffix("_input")?
        .parse::<u32>()
        .ok()
}

/// Enumerates the hwmon devices under a root.
///
/// Takes the root as a parameter so the whole traversal is tested against a
/// fixture tree: the live `/sys` cannot be made to contain a second `coretemp`,
/// a renumbered device or a missing symlink on demand — and PULSE's tests never
/// write to `/sys`.
///
/// Sorted by name and then by path, so the enumeration order does not depend on
/// directory iteration and two devices of the same driver keep a stable
/// relative order.
pub fn discover_in(root: &Path) -> Vec<HwmonDevice> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut devices: Vec<HwmonDevice> = entries
        .flatten()
        .filter_map(|entry| HwmonDevice::read(&entry.path()))
        .collect();

    devices.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.path.cmp(&right.path))
    });
    devices
}

/// Enumerates the hwmon devices of the running system.
pub fn discover() -> Vec<HwmonDevice> {
    discover_in(Path::new(HWMON_ROOT))
}

/// The hwmon devices belonging to one piece of hardware.
///
/// `<device>/hwmon/hwmonN`. This is the association that makes a GPU
/// temperature *that GPU's* temperature: the sensors live under the card's own
/// PCI device directory, so nothing has to guess from a driver name or a
/// numbering coincidence.
pub fn devices_under(device: &Path) -> Vec<HwmonDevice> {
    discover_in(&device.join(DEVICE_HWMON_SUBDIR))
}

/// Reads a `*_input` file as an integer.
fn read_input(path: &Path) -> Result<i64, MetricError> {
    let content = fs::read_to_string(path).map_err(|error| {
        let code = match error.kind() {
            // The node vanished: a driver unbound, a GPU reset, a device
            // removed. Not the same as never having existed.
            io::ErrorKind::NotFound => MetricErrorCode::NotDetected,
            io::ErrorKind::PermissionDenied => MetricErrorCode::PermissionDenied,
            _ => MetricErrorCode::Io,
        };

        MetricError::new(code, format!("could not read {}: {error}", path.display()))
    })?;

    content.trim().parse::<i64>().map_err(|error| {
        MetricError::new(
            MetricErrorCode::Parse,
            format!(
                "{} does not contain an integer reading: {error}",
                path.display()
            ),
        )
    })
}

/// Reads a `tempN_input` and converts it to degrees Celsius.
///
/// The file is in millidegrees, so `42000` becomes `42.0`. A negative reading
/// is kept — sensors on a cold machine legitimately report below zero — while a
/// physically impossible one is refused, because it is a failed read and not a
/// measurement.
pub fn read_temperature_celsius(input: &Path) -> Result<f64, MetricError> {
    let millidegrees = read_input(input)?;
    let celsius = millidegrees as f64 / MILLIDEGREES_PER_DEGREE;

    if !celsius.is_finite() || celsius < ABSOLUTE_ZERO_C {
        return Err(MetricError::new(
            MetricErrorCode::Parse,
            format!(
                "{} reported {millidegrees} m°C, which is not a temperature",
                input.display()
            ),
        ));
    }

    Ok(celsius)
}

/// Reads a `fanN_input`, in revolutions per minute.
///
/// **Zero is kept.** A fan that is stopped — a GPU under its zero-RPM threshold,
/// a quiet desktop — genuinely turns at 0 RPM, and that is a measurement the
/// user wants to see. It is the *absence* of a reading that must show as `—`,
/// and the two reach the interface as different things: a value of `0` and no
/// value at all.
///
/// A negative reading is refused: a fan cannot turn backwards, so the file was
/// not a fan speed.
pub fn read_fan_rpm(input: &Path) -> Result<f64, MetricError> {
    let rpm = read_input(input)?;

    if rpm < 0 {
        return Err(MetricError::new(
            MetricErrorCode::Parse,
            format!(
                "{} reported {rpm} RPM, which is not a fan speed",
                input.display()
            ),
        ));
    }

    Ok(rpm as f64)
}

/// Turns a sensor read failure into the availability that describes it.
///
/// A sensor node that is not there is `unsupported` — this driver on this
/// hardware does not offer the reading — rather than `notDetected`, which would
/// suggest the *device* is missing. A node that disappeared after being read
/// once is a different story, and arrives as an I/O or not-found error from
/// [`read_temperature_celsius`] at sample time.
pub fn availability_for_sensor(error: &MetricError) -> Availability {
    match error.code {
        // The file existed at discovery and does not now: a driver reload, a
        // suspend, a device removal. It may well come back.
        MetricErrorCode::NotDetected => {
            Availability::temporarily_unavailable(error.message.clone())
        }
        MetricErrorCode::PermissionDenied => Availability::permission_denied(error.message.clone()),
        MetricErrorCode::Io => Availability::temporarily_unavailable(error.message.clone()),
        _ => Availability::provider_error(error.clone()),
    }
}

#[cfg(test)]
pub mod fixtures {
    //! Fixture hwmon trees.
    //!
    //! Every test in this module and in the two that build on it runs against a
    //! directory built here. **Nothing writes to `/sys`**, and nothing depends
    //! on the sensors of the machine running the tests — which is what lets a
    //! dual-socket `coretemp`, a renumbered hwmon and a GPU with no sensor at
    //! all be covered from one laptop.

    use std::fs;
    #[cfg(unix)]
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A temporary hwmon root, removed when it goes out of scope.
    pub struct HwmonTree {
        pub root: PathBuf,
    }

    impl HwmonTree {
        pub fn new(name: &str) -> Self {
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "pulse-hwmon-{name}-{}-{unique}",
                std::process::id()
            ));

            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("fixture root");

            Self { root }
        }

        /// Adds a `hwmonN` directory with a driver name.
        pub fn device(&self, index: u32, name: &str) -> HwmonFixture {
            let path = self.root.join(format!("hwmon{index}"));
            fs::create_dir_all(&path).expect("fixture device");
            fs::write(path.join("name"), format!("{name}\n")).expect("fixture name");

            HwmonFixture { path }
        }

        /// Adds a `hwmonN` directory with no `name` file at all.
        pub fn nameless(&self, index: u32) -> PathBuf {
            let path = self.root.join(format!("hwmon{index}"));
            fs::create_dir_all(&path).expect("fixture device");
            path
        }
    }

    impl Drop for HwmonTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// One fixture hwmon device.
    pub struct HwmonFixture {
        pub path: PathBuf,
    }

    impl HwmonFixture {
        /// Writes `<family><index>_input`.
        pub fn input(&self, family: &str, index: u32, value: &str) -> &Self {
            fs::write(
                self.path.join(format!("{family}{index}_input")),
                format!("{value}\n"),
            )
            .expect("fixture input");
            self
        }

        /// Writes `<family><index>_label`.
        pub fn label(&self, family: &str, index: u32, value: &str) -> &Self {
            fs::write(
                self.path.join(format!("{family}{index}_label")),
                format!("{value}\n"),
            )
            .expect("fixture label");
            self
        }

        /// Writes any other attribute — a limit file, a control file — so tests
        /// can assert PULSE ignores it.
        pub fn attribute(&self, name: &str, value: &str) -> &Self {
            fs::write(self.path.join(name), format!("{value}\n")).expect("fixture attribute");
            self
        }

        /// Points this device's `device` symlink at a directory.
        ///
        /// Unix only — `hwmon` is a Linux interface, and the Windows
        /// cross-check harness compiles this module for a target that has no
        /// `symlink`. The association it tests is exercised on Fedora, where it
        /// is the only place it matters.
        #[cfg(unix)]
        pub fn attached_to(&self, target: &Path) -> &Self {
            fs::create_dir_all(target).expect("fixture target");
            let link = self.path.join("device");
            let _ = fs::remove_file(&link);
            std::os::unix::fs::symlink(target, link).expect("fixture symlink");
            self
        }

        /// The path of one of this device's inputs.
        pub fn input_path(&self, family: &str, index: u32) -> PathBuf {
            self.path.join(format!("{family}{index}_input"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::HwmonTree;
    use super::*;

    #[test]
    fn discovers_devices_by_their_driver_name() {
        let tree = HwmonTree::new("discover");
        tree.device(0, "acpitz");
        tree.device(6, "coretemp");

        let devices = discover_in(&tree.root);

        let names: Vec<&str> = devices.iter().map(|device| device.name.as_str()).collect();
        assert_eq!(names, ["acpitz", "coretemp"]);
    }

    #[test]
    fn the_hwmon_number_is_never_the_identity() {
        // The same machine, after a driver loaded in a different order. The
        // driver PULSE is looking for must still be found.
        let first = HwmonTree::new("renumber-a");
        first.device(6, "coretemp").input("temp", 1, "93000");

        let second = HwmonTree::new("renumber-b");
        second.device(0, "coretemp").input("temp", 1, "93000");

        let by_name = |devices: Vec<HwmonDevice>| {
            devices
                .into_iter()
                .find(|device| device.name == "coretemp")
                .expect("coretemp is present whatever it is numbered")
        };

        let a = by_name(discover_in(&first.root));
        let b = by_name(discover_in(&second.root));

        assert_ne!(a.path, b.path, "the fixture really did renumber it");
        assert_eq!(
            read_temperature_celsius(&a.channels("temp")[0].input).expect("read"),
            read_temperature_celsius(&b.channels("temp")[0].input).expect("read"),
        );
    }

    #[test]
    fn a_directory_without_a_name_is_not_a_device() {
        let tree = HwmonTree::new("nameless");
        tree.nameless(0);
        tree.device(1, "coretemp");

        let devices = discover_in(&tree.root);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "coretemp");
    }

    #[test]
    fn a_missing_root_is_an_empty_list_rather_than_a_failure() {
        assert!(discover_in(Path::new("/nonexistent/hwmon/root")).is_empty());
    }

    // --- channels ---------------------------------------------------------

    #[test]
    fn lists_channels_with_their_labels_in_numeric_order() {
        let tree = HwmonTree::new("channels");
        let device = tree.device(0, "coretemp");
        device
            .input("temp", 1, "93000")
            .label("temp", 1, "Package id 0")
            .input("temp", 10, "91000")
            .label("temp", 10, "Core 8")
            .input("temp", 2, "73000")
            .label("temp", 2, "Core 0");

        let channels = super::HwmonDevice::read(&device.path)
            .expect("device")
            .channels("temp");

        // Numeric, not lexicographic: temp10 must not sort between temp1 and
        // temp2.
        let indices: Vec<u32> = channels.iter().map(|channel| channel.index).collect();
        assert_eq!(indices, [1, 2, 10]);
        assert_eq!(channels[0].label.as_deref(), Some("Package id 0"));
        assert_eq!(channels[2].label.as_deref(), Some("Core 8"));
    }

    #[test]
    fn an_unlabelled_channel_is_listed_without_a_label() {
        // The `nouveau` shape: one temperature, no label file.
        let tree = HwmonTree::new("unlabelled");
        let device = tree.device(0, "nouveau");
        device.input("temp", 1, "46000");

        let channels = super::HwmonDevice::read(&device.path)
            .expect("device")
            .channels("temp");

        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].label, None);
    }

    #[test]
    fn limit_and_control_files_are_never_channels() {
        // Reading `temp1_crit` as a measurement is how a monitoring tool
        // reports an idle laptop at 100 °C. And `pwm1` is a control PULSE must
        // not even look at.
        let tree = HwmonTree::new("limits");
        let device = tree.device(0, "coretemp");
        device
            .input("temp", 1, "45000")
            .attribute("temp1_crit", "100000")
            .attribute("temp1_max", "90000")
            .attribute("temp1_crit_alarm", "0")
            .attribute("pwm1", "128")
            .attribute("pwm1_enable", "2");

        let channels = super::HwmonDevice::read(&device.path)
            .expect("device")
            .channels("temp");

        assert_eq!(channels.len(), 1);
        assert!(channels[0].input.ends_with("temp1_input"));
    }

    #[test]
    fn temperature_and_fan_channels_do_not_bleed_into_each_other() {
        let tree = HwmonTree::new("families");
        let device = tree.device(0, "amdgpu");
        device.input("temp", 1, "52000").input("fan", 1, "1200");

        let device = super::HwmonDevice::read(&device.path).expect("device");

        assert_eq!(device.channels("temp").len(), 1);
        assert_eq!(device.channels("fan").len(), 1);
    }

    // --- values -----------------------------------------------------------

    #[test]
    fn converts_millidegrees_to_celsius() {
        let tree = HwmonTree::new("millidegrees");
        let device = tree.device(0, "coretemp");
        device
            .input("temp", 1, "42000")
            .input("temp", 2, "93500")
            .input("temp", 3, "0");

        let read = |index| read_temperature_celsius(&device.input_path("temp", index));

        assert_eq!(read(1).expect("read"), 42.0);
        assert_eq!(read(2).expect("read"), 93.5);
        // 0 m°C is 0 °C, which is a temperature like any other.
        assert_eq!(read(3).expect("read"), 0.0);
    }

    #[test]
    fn a_negative_temperature_is_a_reading_not_an_error() {
        // A machine in a cold room, or a sensor that reads below zero at idle.
        let tree = HwmonTree::new("negative");
        let device = tree.device(0, "coretemp");
        device.input("temp", 1, "-5000");

        assert_eq!(
            read_temperature_celsius(&device.input_path("temp", 1)).expect("read"),
            -5.0
        );
    }

    #[test]
    fn a_physically_impossible_temperature_is_refused() {
        // Below absolute zero is a failed read, not a cold GPU.
        let tree = HwmonTree::new("impossible");
        let device = tree.device(0, "coretemp");
        device.input("temp", 1, "-400000");

        let error = read_temperature_celsius(&device.input_path("temp", 1)).expect_err("refused");

        assert_eq!(error.code, MetricErrorCode::Parse);
    }

    #[test]
    fn an_unparseable_reading_becomes_a_structured_error() {
        let tree = HwmonTree::new("garbage");
        let device = tree.device(0, "coretemp");
        device.input("temp", 1, "not a number");

        let error = read_temperature_celsius(&device.input_path("temp", 1)).expect_err("refused");

        assert_eq!(error.code, MetricErrorCode::Parse);
        assert!(!error.message.is_empty());
    }

    #[test]
    fn a_vanished_sensor_is_temporarily_unavailable_rather_than_a_crash() {
        // A driver reload, a GPU reset, a suspend. The node goes away and comes
        // back; nothing here may panic, and nothing may report the hardware as
        // absent for good.
        let tree = HwmonTree::new("vanished");
        let device = tree.device(0, "nouveau");
        let path = device.input_path("temp", 1);

        let error = read_temperature_celsius(&path).expect_err("no such file");

        assert_eq!(error.code, MetricErrorCode::NotDetected);
        assert_eq!(
            availability_for_sensor(&error).status_str(),
            "temporarilyUnavailable"
        );
    }

    #[test]
    fn a_stopped_fan_reports_zero_rpm_and_a_missing_one_reports_nothing() {
        // The distinction the whole contract rests on: `0 RPM` is a
        // measurement — a GPU below its zero-RPM threshold — and an absent
        // sensor is not a fan at rest.
        let tree = HwmonTree::new("fan");
        let device = tree.device(0, "amdgpu");
        device.input("fan", 1, "0").input("fan", 2, "2187");

        assert_eq!(
            read_fan_rpm(&device.input_path("fan", 1)).expect("read"),
            0.0
        );
        assert_eq!(
            read_fan_rpm(&device.input_path("fan", 2)).expect("read"),
            2187.0
        );
        assert!(read_fan_rpm(&device.input_path("fan", 3)).is_err());
    }

    #[test]
    fn a_negative_fan_speed_is_refused() {
        let tree = HwmonTree::new("fan-negative");
        let device = tree.device(0, "amdgpu");
        device.input("fan", 1, "-1");

        assert!(read_fan_rpm(&device.input_path("fan", 1)).is_err());
    }

    // --- device association ----------------------------------------------

    #[cfg(unix)]
    #[test]
    fn a_device_symlink_ties_a_hwmon_to_its_hardware() {
        let tree = HwmonTree::new("attached");
        let hardware = tree.root.join("pci0000:00").join("0000:01:00.0");
        let device = tree.device(0, "nouveau");
        device.attached_to(&hardware);

        let read = super::HwmonDevice::read(&device.path).expect("device");

        assert_eq!(
            read.device.expect("resolved"),
            fs::canonicalize(&hardware).expect("canonical")
        );
    }

    #[test]
    fn a_hwmon_without_a_device_symlink_is_still_usable() {
        // `acpitz` and the virtual thermal zones have none, and a driver-name
        // lookup must still work.
        let tree = HwmonTree::new("detached");
        let device = tree.device(0, "acpitz");
        device.input("temp", 1, "97000");

        let read = super::HwmonDevice::read(&device.path).expect("device");

        assert_eq!(read.device, None);
        assert_eq!(read.channels("temp").len(), 1);
    }

    #[test]
    fn a_hardware_device_exposes_its_own_hwmon_nodes() {
        // How a GPU's sensors are tied to that GPU: they live under the card's
        // own device directory, so nothing has to guess from numbering.
        let tree = HwmonTree::new("under-device");
        let card = tree.root.join("0000:01:00.0");
        let hwmon_root = card.join(DEVICE_HWMON_SUBDIR);
        fs::create_dir_all(&hwmon_root).expect("fixture");

        let nested = HwmonTree {
            root: hwmon_root.clone(),
        };
        nested.device(3, "amdgpu").input("temp", 1, "52000");
        // Leaked on purpose: the outer tree owns this directory.
        std::mem::forget(nested);

        let devices = devices_under(&card);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "amdgpu");
    }

    #[test]
    fn a_device_with_no_hwmon_directory_yields_nothing() {
        // The reference machine's own case: an NVIDIA card on `nouveau` with no
        // hwmon node at all. Not a failure — the sensors simply are not there.
        let tree = HwmonTree::new("no-hwmon");
        let card = tree.root.join("0000:01:00.0");
        fs::create_dir_all(&card).expect("fixture");

        assert!(devices_under(&card).is_empty());
    }
}
