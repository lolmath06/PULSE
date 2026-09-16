//! Canonical unit conversions shared by every metric family.
//!
//! PULSE publishes hertz and bytes on the wire; the interfaces it reads use
//! whatever they please. Kilohertz from Linux CPUFreq, megahertz from the
//! Windows power API, megahertz again from NVML and AMD's `pp_dpm_*` files —
//! all of it is converted **here**, once, so no provider invents its own scale
//! and no frontend ever learns that MHz exists.

/// Hertz per kilohertz.
const HZ_PER_KHZ: u64 = 1_000;
/// Hertz per megahertz.
const HZ_PER_MHZ: u64 = 1_000_000;

/// Converts a kilohertz reading to hertz.
///
/// Returns `None` for a zero reading or an arithmetic overflow.
///
/// **Zero is not a frequency.** Every interface PULSE reads uses `0` to mean
/// "no figure available" — an idle-state artefact on Linux CPUFreq, an
/// unpopulated field under some hypervisors, a powered-down GPU clock domain.
/// Publishing `0 Hz` would render as `0 GHz`, which a user reads as a claim
/// that the part has stopped rather than as an absence. PULSE reports the
/// metric unavailable instead.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_kilohertz_the_way_cpufreq_reports_it() {
        assert_eq!(kilohertz_to_hertz(3_200_000), Some(3_200_000_000));
        assert_eq!(kilohertz_to_hertz(5_600_000), Some(5_600_000_000));
        assert_eq!(kilohertz_to_hertz(1), Some(1_000));
    }

    #[test]
    fn converts_megahertz_the_way_nvml_and_windows_report_it() {
        assert_eq!(megahertz_to_hertz(3_200), Some(3_200_000_000));
        assert_eq!(megahertz_to_hertz(2_100), Some(2_100_000_000));
        // A GDDR6 memory clock as NVML reports it.
        assert_eq!(megahertz_to_hertz(8_001), Some(8_001_000_000));
    }

    #[test]
    fn zero_is_an_absent_reading_not_a_frequency_of_zero() {
        assert_eq!(kilohertz_to_hertz(0), None);
        assert_eq!(megahertz_to_hertz(0), None);
    }

    #[test]
    fn overflow_is_refused_rather_than_wrapped() {
        assert_eq!(kilohertz_to_hertz(u64::MAX), None);
        assert_eq!(megahertz_to_hertz(u64::MAX), None);
    }

    #[test]
    fn every_converted_value_fits_an_f64_without_losing_precision() {
        // Metric values cross the IPC boundary as f64; a clock in hertz is far
        // below 2^53, which is what makes hertz a safe canonical unit.
        for mhz in [400_u64, 2_100, 8_001, 12_000] {
            let hz = megahertz_to_hertz(mhz).expect("convertible");
            assert_eq!(hz as f64 as u64, hz);
            assert!((hz as f64) < 2f64.powi(53));
        }
    }
}
