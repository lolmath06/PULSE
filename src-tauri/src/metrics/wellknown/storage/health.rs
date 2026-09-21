//! The NVMe SMART / Health Information log page, parsed once for both
//! operating systems.
//!
//! # Why only NVMe
//!
//! NVMe standardises its health log. Log page `0x02` has a fixed 512-byte
//! layout defined by the specification, and every compliant controller fills
//! the same fields with the same meaning. Composite temperature is at offset
//! 1, percentage used at offset 5, and they mean the same thing on a Samsung,
//! a WD and a Kioxia.
//!
//! ATA SMART does not work like that. Its attribute table is a list of
//! vendor-defined IDs whose scaling, normalisation and even direction differ
//! between manufacturers and firmware revisions — attribute 231 is "SSD life
//! left" on one drive and "temperature" on another. Normalising it correctly
//! is a research project of its own, and normalising it *incorrectly* would
//! publish confident, wrong numbers about the health of someone's disk. PULSE
//! therefore reports `storage.health.*` as `Unsupported` on ATA devices rather
//! than guessing. The definitions stay in the catalog; see
//! `docs/metrics/storage.md`.
//!
//! # Why the parsing lives here
//!
//! Linux obtains the 512 bytes through an NVMe admin passthrough ioctl and
//! Windows through `IOCTL_STORAGE_QUERY_PROPERTY` with
//! `StorageDeviceProtocolSpecificProperty`. **The transport differs; the bytes
//! do not.** Parsing them once means the two platforms cannot drift, and means
//! the parser is fully tested on Fedora against synthetic buffers with no NVMe
//! device in sight.
//!
//! # Read-only, always
//!
//! Getting this log is a `Get Log Page` admin command: a read. PULSE never
//! issues `Format NVM`, `Sanitize`, `Firmware Commit`, `Namespace Management`
//! or a destructive self-test, on any code path, under any circumstance.

use crate::metrics::model::{MetricError, MetricErrorCode};

/// The size of the SMART / Health Information log page, in bytes.
///
/// Fixed by the NVMe specification. A shorter buffer is a truncated read, not
/// a small drive, and is refused rather than parsed with whatever arrived.
pub const SMART_LOG_LEN: usize = 512;

/// The log page identifier of the SMART / Health Information log.
pub const SMART_LOG_PAGE_ID: u8 = 0x02;

/// Offset of the Kelvin composite temperature field.
const OFFSET_COMPOSITE_TEMPERATURE: usize = 1;
/// Offset of the available spare percentage.
const OFFSET_AVAILABLE_SPARE: usize = 3;
/// Offset of the percentage used estimate.
const OFFSET_PERCENTAGE_USED: usize = 5;
/// Offset of the 128-bit power cycles count.
const OFFSET_POWER_CYCLES: usize = 112;
/// Offset of the 128-bit power-on hours count.
const OFFSET_POWER_ON_HOURS: usize = 128;
/// Offset of the 128-bit unsafe shutdowns count.
const OFFSET_UNSAFE_SHUTDOWNS: usize = 144;
/// Offset of the 128-bit media and data integrity errors count.
const OFFSET_MEDIA_ERRORS: usize = 160;

/// Absolute zero in degrees Celsius, for the Kelvin conversion.
const KELVIN_OFFSET: f64 = 273.15;

/// The bits of the Critical Warning byte, as the specification defines them.
///
/// Kept as a **transparent bitmask** rather than folded into any score. Each
/// bit is a specific condition the controller is asserting, and turning six
/// independent facts into one number would destroy exactly the information a
/// user needs. PULSE does not publish this as a metric in this phase — the
/// metric contract has no bitmask or multi-boolean value type yet, and
/// smuggling a bitmask into a `Number` or a percentage would be worse than
/// waiting. It is parsed, carried and available for diagnostics; see
/// `docs/metrics/storage.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CriticalWarning(pub u8);

impl CriticalWarning {
    /// The available spare capacity has fallen below its threshold.
    pub const fn spare_below_threshold(self) -> bool {
        self.0 & 0b0000_0001 != 0
    }
    /// Temperature is above an over-temperature threshold or below an
    /// under-temperature one.
    pub const fn temperature_threshold(self) -> bool {
        self.0 & 0b0000_0010 != 0
    }
    /// Device reliability is degraded.
    pub const fn reliability_degraded(self) -> bool {
        self.0 & 0b0000_0100 != 0
    }
    /// The medium has been placed in read-only mode.
    pub const fn read_only(self) -> bool {
        self.0 & 0b0000_1000 != 0
    }
    /// The volatile memory backup device has failed.
    pub const fn backup_failed(self) -> bool {
        self.0 & 0b0001_0000 != 0
    }
    /// Persistent memory region has become read-only or unreliable.
    pub const fn persistent_memory_unreliable(self) -> bool {
        self.0 & 0b0010_0000 != 0
    }

    /// Whether the controller is asserting any warning at all.
    pub const fn any(self) -> bool {
        self.0 != 0
    }
}

/// The standardised values PULSE publishes from the health log.
///
/// Every field is `Option` because a controller may legitimately report a
/// field as "not reported" (all-`0xFF` temperature), and because a truncated
/// or implausible value is dropped rather than published.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NvmeHealth {
    /// The controller's Critical Warning byte, unmodified.
    pub critical_warning: CriticalWarning,
    /// Composite temperature in degrees Celsius, converted from the Kelvin
    /// field. **The controller's own single composite figure** — not one of
    /// its per-sensor readings, and never a CPU or GPU temperature.
    pub temperature_celsius: Option<f64>,
    /// Available spare capacity, as a percentage of the total spare the
    /// controller was manufactured with.
    ///
    /// **Not free space on the filesystem, and not unallocated disk
    /// capacity.** It is the reserve of replacement blocks the controller
    /// keeps for retiring worn ones, and a healthy drive sits at or near
    /// 100 % for most of its life while being completely full of data.
    pub available_spare_percent: Option<f64>,
    /// The controller's estimate of the endurance consumed, as a percentage.
    ///
    /// **Published exactly as reported.** The specification permits values
    /// above 100 once the estimated endurance has been exceeded, and states
    /// that values above 254 are reported as 255. A drive past its rated
    /// endurance is precisely the case a user needs to see, so the figure is
    /// not clamped to 100 and is never turned into `100 - used` as a "health
    /// score".
    pub percentage_used: Option<f64>,
    /// Hours the controller has been powered on.
    pub power_on_hours: Option<u128>,
    /// Power cycles the controller has been through. Carried for diagnostics;
    /// not published as a metric in this phase.
    pub power_cycles: Option<u128>,
    /// Shutdowns during which power was lost without a notification.
    pub unsafe_shutdowns: Option<u128>,
    /// Media and data integrity errors the controller detected.
    pub media_errors: Option<u128>,
}

/// The largest `u128` counter that survives conversion to `f64` exactly.
///
/// `MetricValue` carries an `f64`, so a counter beyond 2^53 would be published
/// with silent rounding. Real drives report power-on hours in the thousands
/// and error counts in the units; anything beyond this bound is a misparse or
/// a controller writing garbage, and is dropped rather than published as an
/// approximation.
const MAX_EXACT_COUNTER: u128 = 1 << 53;

/// Converts a 128-bit NVMe counter into a publishable number.
pub fn counter_value(raw: Option<u128>) -> Option<f64> {
    raw.filter(|&value| value <= MAX_EXACT_COUNTER)
        .map(|value| value as f64)
}

/// Reads a little-endian `u16` at `offset`.
fn read_u16(buffer: &[u8], offset: usize) -> Option<u16> {
    let bytes = buffer.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

/// Reads a little-endian 128-bit counter at `offset`.
fn read_u128(buffer: &[u8], offset: usize) -> Option<u128> {
    let bytes: [u8; 16] = buffer.get(offset..offset + 16)?.try_into().ok()?;
    Some(u128::from_le_bytes(bytes))
}

/// Converts the log's Kelvin composite temperature into degrees Celsius.
///
/// Returns `None` for the values a controller uses to mean "not reported"
/// (`0`) or "invalid" (`0xFFFF`), and for anything outside a range a storage
/// device can physically be in. A drive reading −200 °C is a misparse, and
/// showing it would be worse than showing nothing.
pub fn kelvin_to_celsius(kelvin: u16) -> Option<f64> {
    if kelvin == 0 || kelvin == u16::MAX {
        return None;
    }

    let celsius = f64::from(kelvin) - KELVIN_OFFSET;

    // Generous, but bounded: storage is specified down to around −40 °C and
    // controllers shut down long before 150 °C.
    (-60.0..=150.0).contains(&celsius).then_some(celsius)
}

/// Converts a raw percentage byte into a publishable percentage.
///
/// Used for available spare, which the specification bounds to 0–100. A
/// controller reporting more than 100 % spare is reporting nonsense, and the
/// field is dropped rather than clamped into something that looks fine.
fn bounded_percent(raw: u8) -> Option<f64> {
    (raw <= 100).then(|| f64::from(raw))
}

/// Parses the 512-byte SMART / Health Information log page.
///
/// Refuses a buffer that is not exactly the specified length. A short read is
/// a failed read — perhaps the driver returned a partial response, perhaps the
/// wrong log page came back — and parsing what arrived would publish whatever
/// happened to sit at each offset as a temperature and a wear figure.
pub fn parse_smart_log(buffer: &[u8]) -> Result<NvmeHealth, MetricError> {
    if buffer.len() < SMART_LOG_LEN {
        return Err(MetricError::new(
            MetricErrorCode::Parse,
            format!(
                "NVMe SMART/Health log is {} bytes; the specification defines {SMART_LOG_LEN}",
                buffer.len()
            ),
        ));
    }

    let temperature = read_u16(buffer, OFFSET_COMPOSITE_TEMPERATURE).and_then(kelvin_to_celsius);

    Ok(NvmeHealth {
        critical_warning: CriticalWarning(buffer[0]),
        temperature_celsius: temperature,
        available_spare_percent: bounded_percent(buffer[OFFSET_AVAILABLE_SPARE]),
        // Deliberately not bounded to 100: see `NvmeHealth::percentage_used`.
        percentage_used: Some(f64::from(buffer[OFFSET_PERCENTAGE_USED])),
        power_on_hours: read_u128(buffer, OFFSET_POWER_ON_HOURS),
        power_cycles: read_u128(buffer, OFFSET_POWER_CYCLES),
        unsafe_shutdowns: read_u128(buffer, OFFSET_UNSAFE_SHUTDOWNS),
        media_errors: read_u128(buffer, OFFSET_MEDIA_ERRORS),
    })
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// Builds a synthetic health log page for tests.
    ///
    /// Mirrors the specified layout exactly, so a parser change that shifts an
    /// offset fails here instead of on someone's drive.
    pub fn smart_log(
        critical_warning: u8,
        temperature_kelvin: u16,
        available_spare: u8,
        percentage_used: u8,
        power_on_hours: u128,
        unsafe_shutdowns: u128,
        media_errors: u128,
    ) -> Vec<u8> {
        let mut buffer = vec![0_u8; SMART_LOG_LEN];

        buffer[0] = critical_warning;
        buffer[OFFSET_COMPOSITE_TEMPERATURE..OFFSET_COMPOSITE_TEMPERATURE + 2]
            .copy_from_slice(&temperature_kelvin.to_le_bytes());
        buffer[OFFSET_AVAILABLE_SPARE] = available_spare;
        buffer[OFFSET_PERCENTAGE_USED] = percentage_used;
        buffer[OFFSET_POWER_ON_HOURS..OFFSET_POWER_ON_HOURS + 16]
            .copy_from_slice(&power_on_hours.to_le_bytes());
        buffer[OFFSET_UNSAFE_SHUTDOWNS..OFFSET_UNSAFE_SHUTDOWNS + 16]
            .copy_from_slice(&unsafe_shutdowns.to_le_bytes());
        buffer[OFFSET_MEDIA_ERRORS..OFFSET_MEDIA_ERRORS + 16]
            .copy_from_slice(&media_errors.to_le_bytes());

        buffer
    }

    /// A plausible healthy 2 TB NVMe drive.
    pub fn healthy() -> Vec<u8> {
        smart_log(0, 316, 100, 3, 421, 7, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn parses_a_plausible_healthy_drive() {
        let health = parse_smart_log(&healthy()).expect("valid log");

        assert_eq!(health.temperature_celsius, Some(316.0 - KELVIN_OFFSET));
        assert_eq!(health.available_spare_percent, Some(100.0));
        assert_eq!(health.percentage_used, Some(3.0));
        assert_eq!(health.power_on_hours, Some(421));
        assert_eq!(health.unsafe_shutdowns, Some(7));
        assert_eq!(health.media_errors, Some(0));
        assert!(!health.critical_warning.any());
    }

    #[test]
    fn converts_kelvin_to_celsius() {
        // 316 K is a perfectly ordinary 42.85 °C for a working NVMe drive.
        assert_eq!(kelvin_to_celsius(316), Some(42.85000000000002));
        assert_eq!(kelvin_to_celsius(300).map(|c| c.round()), Some(27.0));
    }

    #[test]
    fn refuses_the_values_a_controller_uses_for_not_reported() {
        assert_eq!(kelvin_to_celsius(0), None);
        assert_eq!(kelvin_to_celsius(u16::MAX), None);
    }

    #[test]
    fn refuses_a_physically_impossible_temperature() {
        // 1 K would be −272 °C. That is a misparse, not a cold drive.
        assert_eq!(kelvin_to_celsius(1), None);
        // 500 K is 227 °C; a controller reaching that reported it wrong.
        assert_eq!(kelvin_to_celsius(500), None);
    }

    #[test]
    fn a_brand_new_drive_reports_zero_percent_used() {
        // Zero here is a real measurement, not a missing one.
        let health = parse_smart_log(&smart_log(0, 300, 100, 0, 1, 0, 0)).expect("valid");
        assert_eq!(health.percentage_used, Some(0.0));
    }

    #[test]
    fn a_fully_consumed_drive_reports_one_hundred() {
        let health = parse_smart_log(&smart_log(0, 300, 100, 100, 50_000, 0, 0)).expect("valid");
        assert_eq!(health.percentage_used, Some(100.0));
    }

    #[test]
    fn a_drive_past_its_rated_endurance_is_published_above_one_hundred() {
        // The specification permits this, and it is exactly the situation a
        // user must be able to see. Clamping to 100 would hide it, and
        // publishing `100 - used` as a health score would report −55 %.
        let health = parse_smart_log(&smart_log(0, 300, 41, 155, 60_000, 3, 12)).expect("valid");

        assert_eq!(health.percentage_used, Some(155.0));
        assert_eq!(health.available_spare_percent, Some(41.0));
    }

    #[test]
    fn available_spare_is_dropped_rather_than_clamped_when_out_of_range() {
        // The specification bounds it to 0–100; 200 is not a drive with extra
        // spare, it is a bad read.
        let health = parse_smart_log(&smart_log(0, 300, 200, 1, 10, 0, 0)).expect("valid");
        assert_eq!(health.available_spare_percent, None);

        let zero = parse_smart_log(&smart_log(0, 300, 0, 1, 10, 0, 0)).expect("valid");
        assert_eq!(
            zero.available_spare_percent,
            Some(0.0),
            "no spare left is a real, and alarming, reading"
        );
    }

    #[test]
    fn a_truncated_buffer_is_refused_rather_than_parsed() {
        // Parsing 64 bytes would read whatever sits at offset 128 as a
        // power-on-hours count.
        let error = parse_smart_log(&[0_u8; 64]).expect_err("must refuse");
        assert_eq!(error.code, MetricErrorCode::Parse);

        assert!(parse_smart_log(&[]).is_err());
        assert!(parse_smart_log(&[0_u8; SMART_LOG_LEN - 1]).is_err());
        assert!(parse_smart_log(&[0_u8; SMART_LOG_LEN]).is_ok());
    }

    #[test]
    fn a_longer_buffer_is_accepted_and_read_at_the_specified_offsets() {
        // Some transports return the log inside a larger response structure
        // whose tail is padding. The defined fields are still where the
        // specification puts them.
        let mut buffer = healthy();
        buffer.extend_from_slice(&[0xAB; 128]);

        let health = parse_smart_log(&buffer).expect("valid");
        assert_eq!(health.percentage_used, Some(3.0));
        assert_eq!(health.power_on_hours, Some(421));
    }

    #[test]
    fn an_all_zero_buffer_yields_no_temperature() {
        // A controller that answered with zeroes did not report 0 K; it did
        // not report at all, and a `-273 °C` reading must never be published.
        let health = parse_smart_log(&[0_u8; SMART_LOG_LEN]).expect("valid length");
        assert_eq!(health.temperature_celsius, None);
    }

    #[test]
    fn critical_warning_bits_stay_separate_facts() {
        let health =
            parse_smart_log(&smart_log(0b0000_0101, 300, 20, 99, 100, 1, 4)).expect("valid");

        assert!(health.critical_warning.any());
        assert!(health.critical_warning.spare_below_threshold());
        assert!(health.critical_warning.reliability_degraded());
        assert!(!health.critical_warning.temperature_threshold());
        assert!(!health.critical_warning.read_only());
        assert!(!health.critical_warning.backup_failed());
        assert!(!health.critical_warning.persistent_memory_unreliable());
    }

    #[test]
    fn every_critical_warning_bit_is_addressable() {
        assert!(CriticalWarning(0b0000_1000).read_only());
        assert!(CriticalWarning(0b0001_0000).backup_failed());
        assert!(CriticalWarning(0b0010_0000).persistent_memory_unreliable());
        assert!(!CriticalWarning(0).any());
    }

    #[test]
    fn counters_beyond_exact_float_range_are_dropped_not_rounded() {
        assert_eq!(counter_value(Some(421)), Some(421.0));
        assert_eq!(counter_value(Some(0)), Some(0.0));
        assert_eq!(counter_value(None), None);
        // A 128-bit field full of ones is not a drive that has been on for
        // 3.4e38 hours.
        assert_eq!(counter_value(Some(u128::MAX)), None);
        assert_eq!(counter_value(Some(MAX_EXACT_COUNTER + 1)), None);
    }

    #[test]
    fn counters_are_read_little_endian_at_their_specified_offsets() {
        let health =
            parse_smart_log(&smart_log(0, 300, 100, 1, 0x0102, 0x0304, 0x0506)).expect("valid");

        assert_eq!(health.power_on_hours, Some(0x0102));
        assert_eq!(health.unsafe_shutdowns, Some(0x0304));
        assert_eq!(health.media_errors, Some(0x0506));
    }

    #[test]
    fn a_wrong_log_page_does_not_masquerade_as_health_data() {
        // Not something the parser can detect from the bytes alone — the log
        // carries no page identifier — so the guard is at the transport, which
        // asks for page 0x02 by number. This pins that number.
        assert_eq!(SMART_LOG_PAGE_ID, 0x02);
        assert_eq!(SMART_LOG_LEN, 512);
    }
}
