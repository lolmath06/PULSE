//! CPU frequency: the canonical unit and what the number actually means.
//!
//! # The unit
//!
//! PULSE publishes frequency in **hertz**, always. Linux's CPUFreq interface
//! reports kilohertz, Windows' power API reports megahertz, and both are
//! converted here — in the shared layer, once — so that no provider invents
//! its own scale and no frontend ever learns that kHz exists.
//!
//! ```text
//! /sys/.../scaling_cur_freq   3200000 kHz  ─┐
//!                                            ├─► 3_200_000_000 Hz
//! CallNtPowerInformation      3200 MHz     ─┘
//! ```
//!
//! # What `cpu.frequency.current` means
//!
//! > The frequency the operating system interface PULSE reads currently
//! > reports for this logical processor.
//!
//! It is deliberately **not** claimed to be a perfect instantaneous
//! measurement of the silicon. Modern CPUs make that claim indefensible:
//!
//! - frequency scaling moves the clock continuously between samples;
//! - Turbo/boost states are brief and opportunistic;
//! - energy policies cap the clock independently of load;
//! - hybrid CPUs run P-cores and E-cores at unrelated frequencies;
//! - under virtualisation the guest may see the host's rated clock only.
//!
//! On Linux with `intel_pstate` or `amd-pstate`, `scaling_cur_freq` is read
//! from the hardware at the moment of the read and is about as close to the
//! truth as an unprivileged process can get. On Windows, `CurrentMhz` is the
//! kernel's most recent accounting figure. Both are honest answers to "what
//! does the OS say", which is what the metric promises.
//!
//! # What `cpu.frequency.max` means
//!
//! > The maximum frequency the platform reports for this logical processor.
//!
//! A *scaling* or *policy* limit is not the same thing as the hardware
//! maximum, and PULSE never lets one stand in for the other silently — see
//! [`MaxFrequencySource`].

use serde::Serialize;

/// Hertz per kilohertz.
const HZ_PER_KHZ: u64 = 1_000;
/// Hertz per megahertz.
const HZ_PER_MHZ: u64 = 1_000_000;

/// Converts a CPUFreq kilohertz reading to hertz.
///
/// Returns `None` for a zero reading or an arithmetic overflow.
///
/// **Zero is not a frequency.** Both platforms use `0` to mean "no figure
/// available" — an idle-state artefact on Linux, an unpopulated field under
/// some hypervisors on Windows — and publishing `0 Hz` would render as
/// `0 GHz`, which the user would read as a claim rather than as an absence.
/// PULSE reports the metric unavailable instead.
pub const fn kilohertz_to_hertz(kilohertz: u64) -> Option<u64> {
    if kilohertz == 0 {
        return None;
    }

    kilohertz.checked_mul(HZ_PER_KHZ)
}

/// Converts a megahertz reading to hertz.
///
/// Same contract as [`kilohertz_to_hertz`]: zero and overflow both yield
/// `None`.
pub const fn megahertz_to_hertz(megahertz: u64) -> Option<u64> {
    if megahertz == 0 {
        return None;
    }

    megahertz.checked_mul(HZ_PER_MHZ)
}

/// Where a `cpu.frequency.max` value came from.
///
/// The two are genuinely different quantities and the difference is visible to
/// the user: a laptop in a power-saving profile can report a scaling maximum
/// of 1.8 GHz on a chip whose hardware maximum is 5.6 GHz. Presenting the
/// former as "maximum frequency" would make PULSE look wrong to anyone who
/// knows their own hardware.
///
/// PULSE therefore publishes the metric only from a genuine hardware figure,
/// and records which one it used so the description can say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MaxFrequencySource {
    /// The platform's rated hardware maximum — Linux `cpuinfo_max_freq`, or
    /// the Windows power API's `MaxMhz`.
    ///
    /// Note that even this is a *rated* figure: on Intel it is typically the
    /// maximum turbo frequency, on Windows frequently the base frequency
    /// instead. It is what the platform advertises, which is the most PULSE
    /// can honestly claim.
    Hardware,
    /// A governor or policy ceiling — Linux `scaling_max_freq`.
    ///
    /// Reflects the *current power policy*, not the chip. PULSE does not
    /// publish `cpu.frequency.max` from this; the variant exists so the
    /// distinction is nameable and testable rather than implicit.
    ScalingPolicy,
}

impl MaxFrequencySource {
    /// Whether a value from this source may be published as
    /// `cpu.frequency.max`.
    pub const fn is_publishable_as_hardware_max(self) -> bool {
        matches!(self, MaxFrequencySource::Hardware)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_kilohertz_the_way_cpufreq_reports_it() {
        // Real readings from an Intel laptop.
        assert_eq!(kilohertz_to_hertz(3_200_000), Some(3_200_000_000));
        assert_eq!(kilohertz_to_hertz(5_600_000), Some(5_600_000_000));
        assert_eq!(kilohertz_to_hertz(400_000), Some(400_000_000));
        assert_eq!(kilohertz_to_hertz(1), Some(1_000));
    }

    #[test]
    fn converts_megahertz_the_way_windows_reports_it() {
        assert_eq!(megahertz_to_hertz(3_200), Some(3_200_000_000));
        assert_eq!(megahertz_to_hertz(800), Some(800_000_000));
        assert_eq!(megahertz_to_hertz(5_400), Some(5_400_000_000));
        assert_eq!(megahertz_to_hertz(1), Some(1_000_000));
    }

    #[test]
    fn zero_is_an_absent_reading_not_a_frequency_of_zero() {
        // Publishing this would render as "0 GHz", which reads as a claim.
        assert_eq!(kilohertz_to_hertz(0), None);
        assert_eq!(megahertz_to_hertz(0), None);
    }

    #[test]
    fn overflow_is_refused_rather_than_wrapped() {
        assert_eq!(kilohertz_to_hertz(u64::MAX), None);
        assert_eq!(megahertz_to_hertz(u64::MAX), None);
        assert_eq!(kilohertz_to_hertz(u64::MAX / 999), None);
        assert_eq!(megahertz_to_hertz(u64::MAX / 999_999), None);
    }

    #[test]
    fn the_largest_convertible_readings_still_work() {
        let max_khz = u64::MAX / HZ_PER_KHZ;
        assert_eq!(kilohertz_to_hertz(max_khz), Some(max_khz * HZ_PER_KHZ));

        let max_mhz = u64::MAX / HZ_PER_MHZ;
        assert_eq!(megahertz_to_hertz(max_mhz), Some(max_mhz * HZ_PER_MHZ));
    }

    #[test]
    fn every_converted_value_fits_an_f64_without_losing_precision() {
        // Metric values cross the IPC boundary as f64. A CPU frequency in Hz
        // is far below 2^53, so the conversion is exact — worth pinning,
        // because it is the reason Hz is a safe canonical unit here.
        for khz in [400_000_u64, 3_200_000, 5_600_000, 9_999_999] {
            let hz = kilohertz_to_hertz(khz).expect("convertible");
            assert_eq!(hz as f64 as u64, hz);
            assert!((hz as f64) < 2f64.powi(53));
        }
    }

    #[test]
    fn a_scaling_limit_is_never_published_as_a_hardware_maximum() {
        assert!(MaxFrequencySource::Hardware.is_publishable_as_hardware_max());
        assert!(!MaxFrequencySource::ScalingPolicy.is_publishable_as_hardware_max());
    }
}
