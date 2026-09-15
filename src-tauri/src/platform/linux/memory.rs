//! Linux physical memory, read from `/proc/meminfo`.
//!
//! Parsing is a pure function over the file's text, so every malformed shape
//! is covered by fixtures rather than hoped about.

use std::fs;
use std::sync::Arc;

use crate::metrics::model::{
    MetricDefinition, MetricError, MetricErrorCode, MetricRef, MetricSample, ProviderId,
};
use crate::metrics::providers::MetricProvider;
use crate::metrics::wellknown::memory::{self, MemoryReading};

const PROC_MEMINFO: &str = "/proc/meminfo";

/// Identifier of the Linux memory provider.
pub const PROVIDER_ID: &str = "linux.memory";

/// Parses `MemTotal` and `MemAvailable` out of `/proc/meminfo`, in **bytes**.
///
/// `/proc/meminfo` reports kibibytes with a `kB` suffix (the kernel writes
/// `kB` but means KiB, i.e. 1024 bytes). The conversion happens here so that
/// nothing above the platform layer ever handles a non-canonical unit.
///
/// `MemAvailable` is used rather than `MemFree`: free memory excludes
/// reclaimable page cache, so a healthy Linux machine would look nearly out of
/// memory. `MemAvailable` is the kernel's own estimate of what a new
/// allocation could obtain, and has been present since Linux 3.14.
pub fn parse_meminfo(content: &str) -> Result<MemoryReading, MetricError> {
    let total = find_field(content, "MemTotal")?;
    let available = find_field(content, "MemAvailable")?;

    MemoryReading::new(total, available)
}

/// Finds one `Name:  value kB` field and returns it in bytes.
fn find_field(content: &str, name: &str) -> Result<u64, MetricError> {
    let raw = content
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == name).then_some(value.trim())
        })
        .ok_or_else(|| {
            MetricError::new(
                MetricErrorCode::Parse,
                format!("{name} is missing from {PROC_MEMINFO}"),
            )
        })?;

    let mut parts = raw.split_ascii_whitespace();

    let number = parts.next().ok_or_else(|| {
        MetricError::new(
            MetricErrorCode::Parse,
            format!("{name} in {PROC_MEMINFO} has no value"),
        )
    })?;

    let value: u64 = number.parse().map_err(|error| {
        MetricError::new(
            MetricErrorCode::Parse,
            format!("invalid {name} value '{number}' in {PROC_MEMINFO}: {error}"),
        )
    })?;

    // The unit suffix is `kB` in practice; a unitless value means bytes.
    // Anything else is a format PULSE does not understand, and guessing would
    // produce a figure wrong by a factor of 1024.
    match parts.next() {
        None => Ok(value),
        Some(unit) if unit.eq_ignore_ascii_case("kb") => value.checked_mul(1024).ok_or_else(|| {
            MetricError::new(
                MetricErrorCode::Parse,
                format!("{name} value '{number} kB' overflows when converted to bytes"),
            )
        }),
        Some(unit) => Err(MetricError::new(
            MetricErrorCode::Parse,
            format!("unexpected unit '{unit}' for {name} in {PROC_MEMINFO}"),
        )),
    }
}

/// Reads and parses `/proc/meminfo`.
fn read_memory() -> Result<MemoryReading, MetricError> {
    let content = fs::read_to_string(PROC_MEMINFO).map_err(|error| {
        MetricError::new(
            MetricErrorCode::Io,
            format!("could not read {PROC_MEMINFO}: {error}"),
        )
    })?;

    parse_meminfo(&content)
}

/// Publishes the `memory.*` metrics on Linux.
///
/// Requires no elevated privileges: `/proc/meminfo` is world-readable. It is
/// also stateless — memory is an instantaneous reading, not a rate — so unlike
/// CPU it is available from the very first request.
#[derive(Debug)]
pub struct LinuxMemoryProvider {
    id: ProviderId,
}

impl LinuxMemoryProvider {
    pub fn new() -> Self {
        Self {
            id: ProviderId::new(PROVIDER_ID).expect("provider id must be valid"),
        }
    }
}

impl Default for LinuxMemoryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for LinuxMemoryProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError> {
        Ok(memory::definitions(&self.id))
    }

    fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError> {
        // One read answers all four metrics, so they are always mutually
        // consistent. The reading-to-samples mapping is shared with Windows.
        Ok(memory::samples(requested, read_memory()))
    }
}

/// Builds the Linux memory provider.
pub fn provider() -> Arc<dyn MetricProvider> {
    Arc::new(LinuxMemoryProvider::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A realistic `/proc/meminfo`, abbreviated.
    const REAL_MEMINFO: &str = "\
MemTotal:       32784204 kB
MemFree:         1234567 kB
MemAvailable:   20123456 kB
Buffers:          123456 kB
Cached:         10000000 kB
SwapCached:            0 kB
Active:          9876543 kB
Inactive:        8765432 kB
SwapTotal:       8388604 kB
SwapFree:        8388604 kB
";

    #[test]
    fn parses_a_real_meminfo_and_converts_to_bytes() {
        let reading = parse_meminfo(REAL_MEMINFO).expect("valid");

        assert_eq!(reading.total(), 32_784_204 * 1024);
        assert_eq!(reading.available(), 20_123_456 * 1024);
    }

    #[test]
    fn applies_the_pulse_used_convention() {
        let reading = parse_meminfo(REAL_MEMINFO).expect("valid");

        assert_eq!(reading.used(), (32_784_204 - 20_123_456) * 1024);
        assert_eq!(reading.used() + reading.available(), reading.total());
        assert!((0.0..=100.0).contains(&reading.usage_percent()));
    }

    #[test]
    fn ignores_memfree_in_favour_of_memavailable() {
        // MemFree here is far smaller than MemAvailable. Using MemFree would
        // report a healthy machine as 96% full.
        let reading = parse_meminfo(REAL_MEMINFO).expect("valid");

        assert_eq!(reading.available(), 20_123_456 * 1024);
        assert_ne!(reading.available(), 1_234_567 * 1024);
    }

    #[test]
    fn line_order_does_not_matter() {
        let reversed = "\
MemAvailable:   20123456 kB
Cached:         10000000 kB
MemFree:         1234567 kB
MemTotal:       32784204 kB
";
        let reading = parse_meminfo(reversed).expect("valid");

        assert_eq!(reading.total(), 32_784_204 * 1024);
        assert_eq!(reading.available(), 20_123_456 * 1024);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let content = "\
SomethingNew:      42 kB
MemTotal:       1024 kB
AnotherThing:  999999 kB
MemAvailable:    512 kB
FutureField:       1 kB
";
        let reading = parse_meminfo(content).expect("valid");

        assert_eq!(reading.total(), 1024 * 1024);
        assert_eq!(reading.available(), 512 * 1024);
    }

    #[test]
    fn tolerates_irregular_whitespace() {
        let reading =
            parse_meminfo("MemTotal:1024 kB\nMemAvailable:     512    kB\n").expect("valid");

        assert_eq!(reading.total(), 1024 * 1024);
        assert_eq!(reading.available(), 512 * 1024);
    }

    #[test]
    fn a_unitless_value_is_taken_as_bytes() {
        let reading = parse_meminfo("MemTotal: 2048\nMemAvailable: 1024\n").expect("valid");

        assert_eq!(reading.total(), 2048);
        assert_eq!(reading.available(), 1024);
    }

    #[test]
    fn rejects_a_missing_memtotal() {
        let error =
            parse_meminfo("MemFree: 100 kB\nMemAvailable: 200 kB\n").expect_err("must be rejected");

        assert_eq!(error.code, MetricErrorCode::Parse);
        assert!(error.message.contains("MemTotal"));
    }

    #[test]
    fn rejects_a_missing_memavailable() {
        // Linux < 3.14. PULSE reports this rather than silently substituting
        // MemFree, which would mean something different.
        let error =
            parse_meminfo("MemTotal: 100 kB\nMemFree: 50 kB\n").expect_err("must be rejected");

        assert_eq!(error.code, MetricErrorCode::Parse);
        assert!(error.message.contains("MemAvailable"));
    }

    #[test]
    fn rejects_an_invalid_value() {
        for content in [
            "MemTotal: abc kB\nMemAvailable: 100 kB\n",
            "MemTotal: -100 kB\nMemAvailable: 100 kB\n",
            "MemTotal: 1.5 kB\nMemAvailable: 100 kB\n",
            "MemTotal:  kB\nMemAvailable: 100 kB\n",
        ] {
            let error = parse_meminfo(content).expect_err("must be rejected");
            assert_eq!(error.code, MetricErrorCode::Parse);
        }
    }

    #[test]
    fn rejects_an_unexpected_unit_rather_than_guessing() {
        // Guessing would be wrong by a factor of 1024 or 1024².
        let error =
            parse_meminfo("MemTotal: 100 MB\nMemAvailable: 50 MB\n").expect_err("must be rejected");

        assert_eq!(error.code, MetricErrorCode::Parse);
        assert!(error.message.contains("unexpected unit"));
    }

    #[test]
    fn rejects_a_value_that_overflows_when_converted() {
        let content = format!("MemTotal: {} kB\nMemAvailable: 1 kB\n", u64::MAX);
        let error = parse_meminfo(&content).expect_err("must be rejected");

        assert_eq!(error.code, MetricErrorCode::Parse);
        assert!(error.message.contains("overflows"));
    }

    #[test]
    fn rejects_a_zero_total() {
        let error =
            parse_meminfo("MemTotal: 0 kB\nMemAvailable: 0 kB\n").expect_err("must be rejected");

        assert_eq!(error.code, MetricErrorCode::Parse);
    }

    #[test]
    fn rejects_available_above_total() {
        let error = parse_meminfo("MemTotal: 100 kB\nMemAvailable: 200 kB\n")
            .expect_err("must be rejected");

        assert_eq!(error.code, MetricErrorCode::Parse);
        assert!(error.message.contains("exceeds total"));
    }

    // --- host tests: assert invariants only, never specific values --------
    // Gated to Linux: the parsing tests above run on every platform, but these
    // read the real /proc, which only exists here.

    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_meminfo_on_this_host_parses_and_is_coherent() {
        let reading = read_memory().expect("/proc/meminfo must be readable without privileges");

        assert!(reading.total() > 0);
        assert!(reading.available() <= reading.total());
        assert!(reading.used() <= reading.total());
        assert_eq!(reading.used() + reading.available(), reading.total());
        assert!((0.0..=100.0).contains(&reading.usage_percent()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_provider_declares_exactly_the_four_memory_metrics() {
        let provider = LinuxMemoryProvider::new();
        let definitions = provider.describe().expect("describe");

        assert_eq!(definitions.len(), 4);
        assert_eq!(provider.id().as_str(), "linux.memory");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_provider_samples_coherent_values_on_this_host() {
        let provider = LinuxMemoryProvider::new();
        let samples = provider
            .sample(&[
                memory::total_ref(),
                memory::used_ref(),
                memory::available_ref(),
                memory::usage_percent_ref(),
            ])
            .expect("sample");

        assert_eq!(samples.len(), 4);
        assert!(samples.iter().all(|s| s.availability.is_available()));

        let number = |index: usize| {
            samples[index]
                .value
                .as_ref()
                .and_then(|value| value.as_number())
                .expect("numeric value")
        };

        let (total, used, available, percent) = (number(0), number(1), number(2), number(3));

        assert!(total > 0.0);
        assert!(available <= total);
        assert!(used <= total);
        assert!((used + available - total).abs() < 1.0);
        assert!((0.0..=100.0).contains(&percent));
    }
}
