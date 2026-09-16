//! AMD telemetry on Fedora, from the `amdgpu` driver's sysfs attributes.
//!
//! The open-source `amdgpu` driver exposes its counters as plain files under
//! the card's PCI device directory, so no library, no vendor SDK and no
//! subprocess is involved — and no `rocm-smi` or `radeontop`.
//!
//! ```text
//! /sys/class/drm/cardN/device/
//! ├── gpu_busy_percent       engine utilisation, whole percent
//! ├── mem_info_vram_total    dedicated VRAM, bytes
//! ├── mem_info_vram_used     dedicated VRAM in use, bytes
//! ├── pp_dpm_sclk            core clock states, active marked with '*'
//! └── pp_dpm_mclk            memory clock states, active marked with '*'
//! ```
//!
//! **An absent file means that metric is unavailable, never that the GPU is
//! gone.** The attributes appear and disappear by driver version, power
//! management mode and ASIC generation: `gpu_busy_percent` is absent on some
//! older parts, and the `pp_dpm_*` files vanish entirely when the power
//! management method is not the one that publishes them. A card with only VRAM
//! readable is still a perfectly good GPU in PULSE.
//!
//! # The `pp_dpm_*` format
//!
//! These files list the available clock states, one per line, with the active
//! one marked:
//!
//! ```text
//! 0: 500Mhz
//! 1: 1200Mhz *
//! 2: 2100Mhz
//! ```
//!
//! The marker, the spacing and the capitalisation of `Mhz` all vary between
//! driver versions, so the parser below is written against several real
//! shapes rather than one assumed one.

use std::fs;
use std::path::Path;

use crate::metrics::wellknown::gpu::GpuMemoryReading;
use crate::metrics::wellknown::units::megahertz_to_hertz;

/// The driver name PULSE recognises.
pub const DRIVER: &str = "amdgpu";

const BUSY_PERCENT: &str = "gpu_busy_percent";
const VRAM_TOTAL: &str = "mem_info_vram_total";
const VRAM_USED: &str = "mem_info_vram_used";
const SCLK: &str = "pp_dpm_sclk";
const MCLK: &str = "pp_dpm_mclk";

/// Reads a sysfs attribute of the card's device directory.
fn attribute(device: &Path, name: &str) -> Option<String> {
    fs::read_to_string(device.join(name))
        .ok()
        .map(|content| content.trim().to_string())
}

/// Parses `gpu_busy_percent`, a whole percentage.
pub fn parse_busy_percent(content: &str) -> Option<u32> {
    content.trim().parse::<u32>().ok()
}

/// Parses one of the `mem_info_vram_*` files, already in bytes.
///
/// The kernel reports these in bytes, so unlike `/proc/meminfo` there is no
/// unit conversion — and no opportunity to be wrong by a factor of 1024.
pub fn parse_vram_bytes(content: &str) -> Option<u64> {
    content.trim().parse::<u64>().ok()
}

/// Extracts the active clock state from a `pp_dpm_*` file, in megahertz.
///
/// Returns `None` when no line is marked active — which happens while the
/// device is in a transitional power state — rather than guessing at the
/// highest or the first entry.
pub fn parse_active_clock_mhz(content: &str) -> Option<u32> {
    for line in content.lines() {
        let line = line.trim();
        if !line.ends_with('*') {
            continue;
        }

        // `1: 1200Mhz *` → the frequency token is after the index.
        let without_marker = line.trim_end_matches('*').trim();
        let value = without_marker
            .split_once(':')
            .map(|(_, rest)| rest)
            .unwrap_or(without_marker)
            .trim();

        // Strip whichever spelling of the unit this driver used.
        let digits: String = value
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect();

        if let Ok(megahertz) = digits.parse::<u32>() {
            if megahertz > 0 {
                return Some(megahertz);
            }
        }
    }

    None
}

/// Everything the `amdgpu` attributes could provide for one card.
///
/// Each field is independent: a missing `gpu_busy_percent` costs utilisation
/// and nothing else.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AmdgpuReading {
    pub usage_percent: Option<f64>,
    pub memory: Option<GpuMemoryReading>,
    pub core_clock_hz: Option<u64>,
    pub memory_clock_hz: Option<u64>,
}

/// Reads every available attribute of one card.
///
/// Takes the device directory as a parameter so the whole read is tested
/// against a fixture tree — including the shapes a live AMD card cannot be
/// asked to produce, such as a malformed file or `used` exceeding `total`.
pub fn read_device(device: &Path) -> AmdgpuReading {
    let usage_percent = attribute(device, BUSY_PERCENT)
        .and_then(|content| parse_busy_percent(&content))
        .and_then(crate::metrics::wellknown::gpu::utilization_percent);

    // Both files or neither: a total without a used figure cannot produce any
    // of the four memory metrics.
    let memory = match (
        attribute(device, VRAM_TOTAL).and_then(|c| parse_vram_bytes(&c)),
        attribute(device, VRAM_USED).and_then(|c| parse_vram_bytes(&c)),
    ) {
        (Some(total), Some(used)) => GpuMemoryReading::from_total_and_used(total, used).ok(),
        _ => None,
    };

    let clock = |name: &str| {
        attribute(device, name)
            .and_then(|content| parse_active_clock_mhz(&content))
            .and_then(|megahertz| megahertz_to_hertz(u64::from(megahertz)))
    };

    AmdgpuReading {
        usage_percent,
        memory,
        core_clock_hz: clock(SCLK),
        memory_clock_hz: clock(MCLK),
    }
}

#[cfg(test)]
pub mod fixtures {
    //! A fake `amdgpu` device directory.

    use std::fs;
    use std::path::PathBuf;

    pub struct AmdDevice {
        pub path: PathBuf,
    }

    impl AmdDevice {
        pub fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "pulse-amdgpu-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("fixture device");

            Self { path }
        }

        pub fn with(&self, name: &str, content: &str) -> &Self {
            fs::write(self.path.join(name), content).expect("attribute");
            self
        }
    }

    impl Drop for AmdDevice {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::AmdDevice;
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    // --- individual parsers ----------------------------------------------

    #[test]
    fn parses_the_busy_percentage() {
        assert_eq!(parse_busy_percent("0"), Some(0));
        assert_eq!(parse_busy_percent("47\n"), Some(47));
        assert_eq!(parse_busy_percent("100"), Some(100));
    }

    #[test]
    fn rejects_a_malformed_busy_percentage() {
        for content in ["", "abc", "-1", "4.7", "47%"] {
            assert_eq!(parse_busy_percent(content), None, "'{content}'");
        }
    }

    #[test]
    fn parses_vram_bytes_without_any_unit_conversion() {
        // The kernel reports bytes here, unlike /proc/meminfo's kB.
        assert_eq!(parse_vram_bytes("8589934592\n"), Some(8 * GIB));
        assert_eq!(parse_vram_bytes("0"), Some(0));
        assert_eq!(parse_vram_bytes("bad"), None);
    }

    #[test]
    fn finds_the_active_clock_state() {
        let content = "0: 500Mhz\n1: 1200Mhz *\n2: 2100Mhz\n";
        assert_eq!(parse_active_clock_mhz(content), Some(1200));
    }

    #[test]
    fn tolerates_the_spacing_and_capitalisation_variants_drivers_use() {
        // Real shapes seen across driver versions.
        let variants = [
            ("0: 500Mhz\n1: 1200Mhz *\n", 1200),
            ("0: 500MHz\n1: 1200MHz*\n", 1200),
            ("0: 500Mhz\n1: 1200Mhz   *\n", 1200),
            ("1: 2100Mhz *", 2100),
            ("0: 800Mhz *\n1: 1600Mhz\n", 800),
        ];

        for (content, expected) in variants {
            assert_eq!(
                parse_active_clock_mhz(content),
                Some(expected),
                "failed on {content:?}"
            );
        }
    }

    #[test]
    fn picks_the_marked_state_not_the_first_or_the_highest() {
        let content = "0: 500Mhz\n1: 1200Mhz\n2: 2100Mhz *\n3: 2500Mhz\n";
        assert_eq!(parse_active_clock_mhz(content), Some(2100));
    }

    #[test]
    fn an_unmarked_file_yields_nothing_rather_than_a_guess() {
        // Happens during a power-state transition. Guessing the first entry
        // would publish a frequency the GPU is not running at.
        assert_eq!(parse_active_clock_mhz("0: 500Mhz\n1: 1200Mhz\n"), None);
        assert_eq!(parse_active_clock_mhz(""), None);
    }

    #[test]
    fn a_malformed_clock_line_is_not_published() {
        assert_eq!(parse_active_clock_mhz("garbage *"), None);
        assert_eq!(parse_active_clock_mhz("0: Mhz *"), None);
        assert_eq!(
            parse_active_clock_mhz("0: 0Mhz *"),
            None,
            "0 is not a clock"
        );
    }

    #[test]
    fn a_malformed_line_does_not_hide_a_valid_one() {
        let content = "garbage *\n1: 1200Mhz *\n";
        assert_eq!(parse_active_clock_mhz(content), Some(1200));
    }

    // --- reading a device -------------------------------------------------

    #[test]
    fn reads_a_fully_featured_card() {
        let device = AmdDevice::new("full");
        device
            .with(BUSY_PERCENT, "47\n")
            .with(VRAM_TOTAL, &format!("{}\n", 8 * GIB))
            .with(VRAM_USED, &format!("{}\n", 2 * GIB))
            .with(SCLK, "0: 500Mhz\n1: 2100Mhz *\n")
            .with(MCLK, "0: 96Mhz\n1: 1000Mhz *\n");

        let reading = read_device(&device.path);

        assert_eq!(reading.usage_percent, Some(47.0));
        let memory = reading.memory.expect("readable");
        assert_eq!(memory.total(), 8 * GIB);
        assert_eq!(memory.used(), 2 * GIB);
        assert_eq!(memory.free(), 6 * GIB);
        assert_eq!(memory.usage_percent(), 25.0);
        // MHz in the file, hertz in the contract.
        assert_eq!(reading.core_clock_hz, Some(2_100_000_000));
        assert_eq!(reading.memory_clock_hz, Some(1_000_000_000));
    }

    #[test]
    fn a_missing_attribute_costs_only_its_own_metric() {
        let device = AmdDevice::new("partial");
        device
            .with(VRAM_TOTAL, &format!("{}\n", 8 * GIB))
            .with(VRAM_USED, &format!("{}\n", GIB));

        let reading = read_device(&device.path);

        assert_eq!(reading.usage_percent, None, "no gpu_busy_percent file");
        assert_eq!(reading.core_clock_hz, None, "no pp_dpm_sclk file");
        assert_eq!(reading.memory_clock_hz, None);
        // And the VRAM that is present still reads.
        assert!(reading.memory.is_some());
    }

    #[test]
    fn a_card_exposing_nothing_reads_as_nothing_rather_than_zeroes() {
        let device = AmdDevice::new("empty");

        assert_eq!(read_device(&device.path), AmdgpuReading::default());
    }

    #[test]
    fn a_missing_device_directory_is_not_a_panic() {
        let reading = read_device(Path::new("/nonexistent/pulse/amdgpu"));
        assert_eq!(reading, AmdgpuReading::default());
    }

    #[test]
    fn vram_used_above_total_is_refused_rather_than_published() {
        let device = AmdDevice::new("inconsistent");
        device
            .with(VRAM_TOTAL, &format!("{}\n", 8 * GIB))
            .with(VRAM_USED, &format!("{}\n", 9 * GIB));

        assert!(
            read_device(&device.path).memory.is_none(),
            "an impossible reading must not reach a widget"
        );
    }

    #[test]
    fn a_zero_vram_total_is_refused() {
        let device = AmdDevice::new("zerototal");
        device.with(VRAM_TOTAL, "0\n").with(VRAM_USED, "0\n");

        assert!(read_device(&device.path).memory.is_none());
    }

    #[test]
    fn a_total_without_a_used_figure_yields_no_memory_metrics() {
        let device = AmdDevice::new("halfmemory");
        device.with(VRAM_TOTAL, &format!("{}\n", 8 * GIB));

        assert!(read_device(&device.path).memory.is_none());
    }

    #[test]
    fn malformed_files_are_ignored_rather_than_parsed_into_nonsense() {
        let device = AmdDevice::new("malformed");
        device
            .with(BUSY_PERCENT, "not a number\n")
            .with(VRAM_TOTAL, "eight gigabytes\n")
            .with(VRAM_USED, "\n")
            .with(SCLK, "\u{0}\u{1}garbage\n");

        assert_eq!(read_device(&device.path), AmdgpuReading::default());
    }

    #[test]
    fn an_implausible_busy_percentage_is_not_published() {
        let device = AmdDevice::new("badbusy");
        device.with(BUSY_PERCENT, "4000000\n");

        assert_eq!(read_device(&device.path).usage_percent, None);
    }
}
