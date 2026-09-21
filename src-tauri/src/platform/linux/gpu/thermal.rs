//! Which GPU sensor is which, on Fedora.
//!
//! A graphics card that reports sensors carries them under its own PCI device
//! directory:
//!
//! ```text
//! /sys/class/drm/cardN/device/hwmon/hwmonM/
//! ├── name              amdgpu, nouveau, radeon…
//! ├── temp1_input       a temperature, in millidegrees Celsius
//! ├── temp1_label       "edge", "junction", "mem"… when the driver says
//! └── fan1_input        a fan speed, in RPM
//! ```
//!
//! That path is the association: these sensors belong to *this* card, with no
//! guessing from a driver name or a numbering coincidence. See
//! [`super::super::hwmon`] for the file format and the millidegree conversion.
//!
//! # Labels decide, not channel numbers
//!
//! `amdgpu` publishes up to three temperatures and labels each one:
//!
//! | Label | PULSE metric | What it measures |
//! |---|---|---|
//! | `edge` | `gpu.temperature.core` | The die's own sensor |
//! | `junction` | `gpu.temperature.hotspot` | The hottest point on the package |
//! | `mem` | `gpu.temperature.memory` | The video memory |
//!
//! **The mapping is by label, never by position.** Assuming `temp1` is the
//! edge, `temp2` the junction and `temp3` the memory happens to be right on
//! many cards and wrong on others — a card that publishes only `edge` and `mem`
//! would have its memory temperature published as a hotspot, a number that is
//! plausible, wrong, and impossible for the user to catch.
//!
//! A driver that labels nothing is a different case, and the only one where a
//! position is used: a device with exactly **one** unlabelled temperature
//! channel publishes it as the core temperature, because a single sensor on a
//! GPU is the GPU's temperature. Two unlabelled channels are published as
//! nothing at all — there is no honest way to say which is which.
//!
//! # One fan, or none
//!
//! PULSE's contract has a single `gpu.fan.speed` per GPU. A card exposing one
//! `fanN_input` publishes it; a card exposing several publishes none, with the
//! reason, rather than picking the first and presenting a partial answer as the
//! whole one.
//!
//! # Read-only
//!
//! Nothing here opens `pwmN`, `pwmN_enable` or any limit file, for reading or
//! for writing. PULSE observes the cooling system; it does not touch it.

use std::path::{Path, PathBuf};

use crate::metrics::model::Availability;
use crate::metrics::wellknown::gpu::GpuCapabilities;
use crate::platform::linux::hwmon::{self, HwmonChannel, HwmonDevice};

/// Labels a driver uses for the die's own temperature.
const CORE_LABELS: &[&str] = &["edge", "temp1", "gpu", "gpu core"];
/// Labels a driver uses for the package hotspot.
const HOTSPOT_LABELS: &[&str] = &["junction", "hotspot"];
/// Labels a driver uses for the video memory.
const MEMORY_LABELS: &[&str] = &["mem", "memory", "vram"];

/// The files one GPU's readings come from.
///
/// Paths are resolved **once**, at startup, and read at sample time. The set of
/// sensors a driver exposes does not change while the machine runs, even though
/// every value does, so a refresh re-reads four small files rather than walking
/// `/sys` again.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GpuThermalSources {
    pub temperature_core: Option<PathBuf>,
    pub temperature_hotspot: Option<PathBuf>,
    pub temperature_memory: Option<PathBuf>,
    pub fan: Option<PathBuf>,
    /// Why a metric has no source, when the reason is more specific than "this
    /// driver does not expose it" — several fans, or unlabelled channels that
    /// cannot be told apart.
    pub fan_reason: Option<String>,
}

impl GpuThermalSources {
    /// Whether any sensor at all was found.
    pub fn is_empty(&self) -> bool {
        self.temperature_core.is_none()
            && self.temperature_hotspot.is_none()
            && self.temperature_memory.is_none()
            && self.fan.is_none()
    }

    /// The availabilities these sources imply, folded into a device's
    /// capabilities.
    ///
    /// Each of the four is decided on its own: a card with an edge temperature
    /// and no junction sensor keeps the first and explains the second.
    pub fn apply(&self, mut capabilities: GpuCapabilities, driver: &str) -> GpuCapabilities {
        let missing = |what: &str| {
            Availability::unsupported(format!(
                "the {driver} driver on this device exposes no {what} sensor"
            ))
        };

        capabilities.temperature_core = match self.temperature_core {
            Some(_) => Availability::Available,
            None => missing("GPU temperature"),
        };
        capabilities.temperature_hotspot = match self.temperature_hotspot {
            Some(_) => Availability::Available,
            None => missing("hotspot temperature"),
        };
        capabilities.temperature_memory = match self.temperature_memory {
            Some(_) => Availability::Available,
            None => missing("memory temperature"),
        };
        capabilities.fan_speed = match (&self.fan, &self.fan_reason) {
            (Some(_), _) => Availability::Available,
            (None, Some(reason)) => Availability::unsupported(reason.clone()),
            (None, None) => missing("fan speed"),
        };

        capabilities
    }
}

/// Finds the sensors of the card at this device directory.
///
/// A card with several hwmon nodes — rare, but a driver may publish more than
/// one — contributes all of them, and the first node to provide a given sensor
/// wins. Devices are enumerated in a deterministic order, so that choice does
/// not vary between runs.
pub fn discover(device: &Path) -> GpuThermalSources {
    let devices = hwmon::devices_under(device);
    let mut sources = GpuThermalSources::default();

    for hwmon_device in &devices {
        merge(&mut sources, from_device(hwmon_device));
    }

    sources
}

/// Fills in whatever the accumulated sources are still missing.
fn merge(into: &mut GpuThermalSources, from: GpuThermalSources) {
    if into.temperature_core.is_none() {
        into.temperature_core = from.temperature_core;
    }
    if into.temperature_hotspot.is_none() {
        into.temperature_hotspot = from.temperature_hotspot;
    }
    if into.temperature_memory.is_none() {
        into.temperature_memory = from.temperature_memory;
    }
    if into.fan.is_none() {
        into.fan = from.fan;
        into.fan_reason = from.fan_reason;
    }
}

/// Maps one hwmon device's channels onto PULSE's four thermal metrics.
pub fn from_device(device: &HwmonDevice) -> GpuThermalSources {
    map_channels(&device.channels("temp"), &device.channels("fan"))
}

/// The mapping itself, over channels alone.
///
/// Separated from the filesystem so every shape — reordered labels, a missing
/// junction sensor, two unlabelled channels, three fans — is tested directly.
pub fn map_channels(temperatures: &[HwmonChannel], fans: &[HwmonChannel]) -> GpuThermalSources {
    let mut sources = GpuThermalSources::default();

    let labelled = |labels: &[&str]| -> Option<PathBuf> {
        temperatures
            .iter()
            .find(|channel| {
                channel
                    .label
                    .as_deref()
                    .map(|label| label.trim().to_ascii_lowercase())
                    .is_some_and(|label| labels.contains(&label.as_str()))
            })
            .map(|channel| channel.input.clone())
    };

    sources.temperature_core = labelled(CORE_LABELS);
    sources.temperature_hotspot = labelled(HOTSPOT_LABELS);
    sources.temperature_memory = labelled(MEMORY_LABELS);

    // The unlabelled case, and the only place a position is used: exactly one
    // channel, no label, and therefore nothing to confuse it with. This is the
    // `nouveau` shape.
    if sources.temperature_core.is_none() {
        let unlabelled: Vec<&HwmonChannel> = temperatures
            .iter()
            .filter(|channel| channel.label.is_none())
            .collect();

        if let [only] = unlabelled[..] {
            sources.temperature_core = Some(only.input.clone());
        }
    }

    match fans {
        [] => {}
        [only] => sources.fan = Some(only.input.clone()),
        several => {
            // One `gpu.fan.speed` per GPU: publishing fan 1 of three as the
            // adapter's speed would answer a different question, silently.
            sources.fan_reason = Some(format!(
                "this device reports {} fans independently, and PULSE publishes a single \
                 fan speed per GPU; reporting one of them as the adapter's speed would be \
                 misleading",
                several.len()
            ));
        }
    }

    sources
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::linux::hwmon::fixtures::HwmonTree;

    fn channel(index: u32, label: Option<&str>) -> HwmonChannel {
        HwmonChannel {
            index,
            label: label.map(str::to_string),
            input: PathBuf::from(format!("/fixture/temp{index}_input")),
        }
    }

    fn fan(index: u32) -> HwmonChannel {
        HwmonChannel {
            index,
            label: None,
            input: PathBuf::from(format!("/fixture/fan{index}_input")),
        }
    }

    #[test]
    fn maps_the_amdgpu_labels_onto_the_three_temperatures() {
        let sources = map_channels(
            &[
                channel(1, Some("edge")),
                channel(2, Some("junction")),
                channel(3, Some("mem")),
            ],
            &[fan(1)],
        );

        assert!(sources
            .temperature_core
            .expect("core")
            .ends_with("temp1_input"));
        assert!(sources
            .temperature_hotspot
            .expect("hotspot")
            .ends_with("temp2_input"));
        assert!(sources
            .temperature_memory
            .expect("memory")
            .ends_with("temp3_input"));
        assert!(sources.fan.expect("fan").ends_with("fan1_input"));
    }

    #[test]
    fn reordered_labels_follow_the_label_and_not_the_position() {
        // The bug this exists to prevent: a card whose channels are in a
        // different order having its memory temperature published as a hotspot.
        let sources = map_channels(
            &[
                channel(1, Some("mem")),
                channel(2, Some("edge")),
                channel(3, Some("junction")),
            ],
            &[],
        );

        assert!(sources
            .temperature_memory
            .expect("memory")
            .ends_with("temp1_input"));
        assert!(sources
            .temperature_core
            .expect("core")
            .ends_with("temp2_input"));
        assert!(sources
            .temperature_hotspot
            .expect("hotspot")
            .ends_with("temp3_input"));
    }

    #[test]
    fn labels_are_matched_without_regard_to_case_or_padding() {
        let sources = map_channels(
            &[channel(1, Some("  Junction ")), channel(2, Some("EDGE"))],
            &[],
        );

        assert!(sources.temperature_hotspot.is_some());
        assert!(sources.temperature_core.is_some());
    }

    #[test]
    fn a_card_with_only_an_edge_sensor_publishes_only_that() {
        // Absent is absent: no hotspot is invented from the edge reading.
        let sources = map_channels(&[channel(1, Some("edge"))], &[]);

        assert!(sources.temperature_core.is_some());
        assert_eq!(sources.temperature_hotspot, None);
        assert_eq!(sources.temperature_memory, None);
    }

    #[test]
    fn a_single_unlabelled_channel_is_the_gpu_temperature() {
        // The `nouveau` shape, when the sensor is there at all.
        let sources = map_channels(&[channel(1, None)], &[]);

        assert!(sources
            .temperature_core
            .expect("core")
            .ends_with("temp1_input"));
        assert_eq!(sources.temperature_hotspot, None);
    }

    #[test]
    fn two_unlabelled_channels_are_published_as_nothing() {
        // There is no honest way to say which is which, and guessing would
        // publish one sensor's reading under another's name.
        let sources = map_channels(&[channel(1, None), channel(2, None)], &[]);

        assert_eq!(sources.temperature_core, None);
        assert_eq!(sources.temperature_hotspot, None);
        assert!(sources.is_empty());
    }

    #[test]
    fn an_unrecognised_label_is_ignored_rather_than_guessed_at() {
        let sources = map_channels(&[channel(1, Some("vddgfx")), channel(2, Some("edge"))], &[]);

        assert!(sources
            .temperature_core
            .expect("core")
            .ends_with("temp2_input"));
        assert_eq!(sources.temperature_memory, None);
    }

    #[test]
    fn several_fans_leave_the_metric_unsupported_with_a_reason() {
        let sources = map_channels(&[], &[fan(1), fan(2), fan(3)]);

        assert_eq!(sources.fan, None);
        assert!(sources
            .fan_reason
            .expect("reason")
            .contains("3 fans independently"));
    }

    #[test]
    fn no_channels_at_all_is_an_empty_mapping_not_a_failure() {
        let sources = map_channels(&[], &[]);

        assert!(sources.is_empty());
        assert_eq!(sources.fan_reason, None);
    }

    // --- capabilities -----------------------------------------------------

    #[test]
    fn each_sensor_decides_its_own_availability() {
        let sources = map_channels(&[channel(1, Some("edge"))], &[fan(1)]);
        let capabilities = sources.apply(
            GpuCapabilities::none_available(&Availability::unsupported("no telemetry backend")),
            "amdgpu",
        );

        assert!(capabilities.temperature_core.is_available());
        assert!(capabilities.fan_speed.is_available());
        assert!(!capabilities.temperature_hotspot.is_available());
        assert!(!capabilities.temperature_memory.is_available());
        // And the performance half is untouched by any of this.
        assert!(!capabilities.usage_core.is_available());
    }

    #[test]
    fn a_thermal_only_card_is_not_a_card_without_telemetry() {
        // The state the GPU card's notice exists for: nothing to say about
        // performance, a real temperature to show.
        let sources = map_channels(&[channel(1, None)], &[]);
        let capabilities = sources.apply(
            GpuCapabilities::none_available(&Availability::unsupported("no vendor library")),
            "nouveau",
        );

        assert!(!capabilities.has_performance());
        assert!(capabilities.has_thermals());
    }

    #[test]
    fn the_reason_names_the_driver_the_user_is_actually_running() {
        let capabilities =
            GpuThermalSources::default().apply(GpuCapabilities::all_available(), "nouveau");

        let Availability::Unsupported { reason } = &capabilities.temperature_core else {
            panic!("expected an unsupported temperature");
        };
        assert!(reason.contains("nouveau"));
    }

    #[test]
    fn several_fans_reach_the_capability_as_their_own_reason() {
        let sources = map_channels(&[], &[fan(1), fan(2)]);
        let capabilities = sources.apply(GpuCapabilities::all_available(), "amdgpu");

        let Availability::Unsupported { reason } = &capabilities.fan_speed else {
            panic!("expected an unsupported fan speed");
        };
        assert!(reason.contains("single fan speed per GPU"));
    }

    // --- against a fixture tree -------------------------------------------

    #[test]
    fn discovers_the_sensors_under_a_card_device_directory() {
        let tree = HwmonTree::new("gpu-thermal");
        let card = tree.root.join("0000:03:00.0");
        let hwmon_root = card.join(crate::platform::linux::hwmon::DEVICE_HWMON_SUBDIR);
        std::fs::create_dir_all(&hwmon_root).expect("fixture");

        let nested = HwmonTree {
            root: hwmon_root.clone(),
        };
        let device = nested.device(4, "amdgpu");
        device
            .input("temp", 1, "52000")
            .label("temp", 1, "edge")
            .input("temp", 2, "61000")
            .label("temp", 2, "junction")
            .input("fan", 1, "1200");
        std::mem::forget(nested);

        let sources = discover(&card);

        assert!(sources.temperature_core.is_some());
        assert!(sources.temperature_hotspot.is_some());
        assert_eq!(sources.temperature_memory, None);
        assert!(sources.fan.is_some());
    }

    #[test]
    fn a_card_with_no_hwmon_node_yields_no_sources() {
        // The reference machine: an NVIDIA card on `nouveau`, correctly
        // detected, with no sensor node at all. Not a failure.
        let tree = HwmonTree::new("gpu-no-hwmon");
        let card = tree.root.join("0000:01:00.0");
        std::fs::create_dir_all(&card).expect("fixture");

        assert!(discover(&card).is_empty());
    }
}
