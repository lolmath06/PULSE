//! Windows physical memory, read via `GlobalMemoryStatusEx`.
//!
//! As with CPU, the arithmetic is pure and unit-tested on Fedora; only the
//! `unsafe` call needs a Windows host.

use crate::metrics::model::MetricError;
use crate::metrics::wellknown::memory::MemoryReading;

/// Identifier of the Windows memory provider.
pub const PROVIDER_ID: &str = "windows.memory";

/// Builds a [`MemoryReading`] from `MEMORYSTATUSEX`'s physical memory fields.
///
/// Only `ullTotalPhys` and `ullAvailPhys` are used, both already in bytes.
///
/// `dwMemoryLoad` is deliberately ignored even though Windows offers it: it is
/// a rounded integer percentage, whereas PULSE computes
/// `used / total * 100` from the byte counts. Using the OS figure would make
/// `memory.usage.percent` disagree with `memory.used` and `memory.total` on
/// the same screen, and would differ from how Fedora reports it.
pub fn reading_from_memory_status(
    total_phys: u64,
    avail_phys: u64,
) -> Result<MemoryReading, MetricError> {
    MemoryReading::new(total_phys, avail_phys)
}

/// The parts that call into the Windows API.
#[cfg(target_os = "windows")]
mod imp {
    use std::sync::Arc;

    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    use crate::metrics::model::{
        MetricDefinition, MetricError, MetricErrorCode, MetricRef, MetricSample, ProviderId,
    };
    use crate::metrics::providers::MetricProvider;
    use crate::metrics::wellknown::memory::{self, MemoryReading};

    use super::{reading_from_memory_status, PROVIDER_ID};

    /// Calls `GlobalMemoryStatusEx` and converts the result.
    ///
    /// # Safety
    ///
    /// `MEMORYSTATUSEX` is zero-initialised and its `dwLength` field is set to
    /// the struct's size, as the API requires — it uses that field to decide
    /// how much to write. The pointer references a live local, and the return
    /// value is checked before any field is read.
    pub fn read_memory() -> Result<MemoryReading, MetricError> {
        let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;

        // SAFETY: see the function docs. `status` is live and correctly sized.
        let ok = unsafe { GlobalMemoryStatusEx(&mut status) };

        if ok == 0 {
            return Err(MetricError::new(
                MetricErrorCode::Io,
                format!(
                    "GlobalMemoryStatusEx failed (error {})",
                    std::io::Error::last_os_error()
                ),
            ));
        }

        reading_from_memory_status(status.ullTotalPhys, status.ullAvailPhys)
    }

    /// Publishes the `memory.*` metrics on Windows.
    ///
    /// Requires no administrator rights, and is stateless — memory is an
    /// instantaneous reading, so it is available from the very first request.
    #[derive(Debug)]
    pub struct WindowsMemoryProvider {
        id: ProviderId,
    }

    impl WindowsMemoryProvider {
        pub fn new() -> Self {
            Self {
                id: ProviderId::new(PROVIDER_ID).expect("provider id must be valid"),
            }
        }
    }

    impl Default for WindowsMemoryProvider {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MetricProvider for WindowsMemoryProvider {
        fn id(&self) -> &ProviderId {
            &self.id
        }

        fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError> {
            Ok(memory::definitions(&self.id))
        }

        fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError> {
            // The same shared mapping Linux uses, so the four metrics are
            // derived identically on both platforms.
            Ok(memory::samples(requested, read_memory()))
        }
    }

    /// Builds the Windows memory provider.
    pub fn provider() -> Arc<dyn MetricProvider> {
        Arc::new(WindowsMemoryProvider::new())
    }
}

#[cfg(target_os = "windows")]
pub use imp::{provider, WindowsMemoryProvider};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::model::MetricErrorCode;

    #[test]
    fn builds_a_reading_from_the_physical_memory_fields() {
        // 32 GiB installed, 20 GiB available.
        let reading = reading_from_memory_status(34_359_738_368, 21_474_836_480).expect("valid");

        assert_eq!(reading.total(), 34_359_738_368);
        assert_eq!(reading.available(), 21_474_836_480);
        assert_eq!(reading.used(), 12_884_901_888);
        assert!((reading.usage_percent() - 37.5).abs() < 1e-9);
    }

    #[test]
    fn applies_the_same_convention_as_fedora() {
        // The identical assertion exists in the Linux memory tests; both must
        // satisfy used + available == total.
        let reading = reading_from_memory_status(16_000_000_000, 6_000_000_000).expect("valid");

        assert_eq!(reading.used(), 10_000_000_000);
        assert_eq!(reading.used() + reading.available(), reading.total());
    }

    #[test]
    fn computes_the_percentage_from_bytes_not_from_the_os_rounding() {
        // used = 5_050_000_000 of 8_000_000_000 -> 63.125%.
        // Windows' own dwMemoryLoad is an integer and would report 63, losing
        // the fraction and disagreeing with the used/total figures shown beside
        // it. PULSE computes from the byte counts instead.
        let reading = reading_from_memory_status(8_000_000_000, 2_950_000_000).expect("valid");

        assert_eq!(reading.used(), 5_050_000_000);
        assert!((reading.usage_percent() - 63.125).abs() < 1e-9);
        assert_ne!(reading.usage_percent(), 63.0);
    }

    #[test]
    fn rejects_a_zero_total() {
        let error = reading_from_memory_status(0, 0).expect_err("must be rejected");
        assert_eq!(error.code, MetricErrorCode::Parse);
    }

    #[test]
    fn rejects_available_above_total() {
        let error = reading_from_memory_status(1_000, 2_000).expect_err("must be rejected");
        assert_eq!(error.code, MetricErrorCode::Parse);
    }

    #[test]
    fn extreme_but_valid_values_stay_in_range() {
        for (total, available) in [(1_u64, 0_u64), (1, 1), (u64::MAX, 0), (u64::MAX, u64::MAX)] {
            let reading = reading_from_memory_status(total, available).expect("valid");
            let percent = reading.usage_percent();

            assert!(percent.is_finite());
            assert!((0.0..=100.0).contains(&percent));
            assert_eq!(reading.used() + reading.available(), reading.total());
        }
    }
}
