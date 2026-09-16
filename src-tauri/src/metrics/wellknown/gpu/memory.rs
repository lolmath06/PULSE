//! GPU memory: what the four VRAM metrics mean, and the arithmetic behind them.
//!
//! # Which memory
//!
//! These metrics describe the GPU's **local, dedicated video memory** — the
//! VRAM soldered to the board. They deliberately do not describe:
//!
//! - system RAM an integrated GPU carves out of main memory;
//! - the "shared system memory" figure Windows reports alongside VRAM, which is
//!   an addressing limit rather than an amount of memory in use;
//! - a per-process video memory budget.
//!
//! An integrated GPU with no dedicated pool therefore reports these metrics as
//! unavailable rather than borrowing a system-RAM number and calling it VRAM.
//! A user comparing PULSE against their vendor's control panel would spot that
//! immediately, and would be right to.
//!
//! # The convention
//!
//! ```text
//! free          = total - used        (when the source reports only two)
//! usage_percent = used / total * 100
//! ```
//!
//! Applied identically on every platform and for every vendor, so
//! `gpu.memory.used` means the same thing whether it came from NVML on Windows
//! or from `mem_info_vram_used` on Fedora.

use crate::metrics::model::{MetricError, MetricErrorCode};

/// A validated snapshot of one GPU's dedicated video memory, in bytes.
///
/// Constructed through the two entry points below, which refuse the
/// combinations that would otherwise reach a widget as nonsense: a zero total
/// (nothing to divide by) and a used figure exceeding the total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuMemoryReading {
    total: u64,
    used: u64,
    free: u64,
}

impl GpuMemoryReading {
    /// Builds a reading from a source that reports **total and used**.
    ///
    /// `free` is derived with checked arithmetic. The AMDGPU sysfs case:
    /// `mem_info_vram_total` and `mem_info_vram_used` exist, and there is no
    /// separate free counter.
    pub fn from_total_and_used(total: u64, used: u64) -> Result<Self, MetricError> {
        Self::validate(total, used)?;

        let free = total.checked_sub(used).ok_or_else(|| {
            MetricError::new(
                MetricErrorCode::Parse,
                format!("GPU memory used ({used} bytes) exceeds total ({total} bytes)"),
            )
            .with_recoverable(true)
        })?;

        Ok(Self { total, used, free })
    }

    /// Builds a reading from a source that reports **all three** figures.
    ///
    /// The NVML case. The reported `free` is preferred over a derived one —
    /// the driver knows about reservations PULSE does not — but only when the
    /// three are mutually consistent. When they are not, the reading is
    /// refused rather than silently reconciled: a source contradicting itself
    /// is a source PULSE should not publish.
    pub fn from_total_used_and_free(total: u64, used: u64, free: u64) -> Result<Self, MetricError> {
        Self::validate(total, used)?;

        if free > total {
            return Err(MetricError::new(
                MetricErrorCode::Parse,
                format!("GPU memory free ({free} bytes) exceeds total ({total} bytes)"),
            )
            .with_recoverable(true));
        }

        // Drivers legitimately reserve a little memory that is neither used
        // nor free, so `used + free` need not equal `total` exactly. What
        // cannot happen is the two together exceeding it.
        // `matches!` rather than `Option::is_none_or`, which is newer than
        // this crate's declared `rust-version`.
        if !matches!(used.checked_add(free), Some(sum) if sum <= total) {
            return Err(MetricError::new(
                MetricErrorCode::Parse,
                format!(
                    "GPU memory used ({used}) plus free ({free}) exceeds total ({total} bytes)"
                ),
            )
            .with_recoverable(true));
        }

        Ok(Self { total, used, free })
    }

    fn validate(total: u64, used: u64) -> Result<(), MetricError> {
        if total == 0 {
            return Err(MetricError::new(
                MetricErrorCode::Parse,
                "reported GPU memory total is zero",
            )
            .with_recoverable(true));
        }

        if used > total {
            return Err(MetricError::new(
                MetricErrorCode::Parse,
                format!("GPU memory used ({used} bytes) exceeds total ({total} bytes)"),
            )
            .with_recoverable(true));
        }

        Ok(())
    }

    pub const fn total(&self) -> u64 {
        self.total
    }

    pub const fn used(&self) -> u64 {
        self.used
    }

    pub const fn free(&self) -> u64 {
        self.free
    }

    /// `used / total * 100`, always within 0–100.
    ///
    /// `total` is non-zero and `used <= total` by construction, so this can
    /// produce neither NaN, nor infinity, nor a value outside the range.
    pub fn usage_percent(&self) -> f64 {
        ((self.used as f64 / self.total as f64) * 100.0).clamp(0.0, 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn derives_free_from_total_and_used() {
        let reading = GpuMemoryReading::from_total_and_used(8 * GIB, 2 * GIB).expect("valid");

        assert_eq!(reading.total(), 8 * GIB);
        assert_eq!(reading.used(), 2 * GIB);
        assert_eq!(reading.free(), 6 * GIB);
        assert_eq!(reading.usage_percent(), 25.0);
    }

    #[test]
    fn prefers_the_reported_free_figure_when_the_source_gives_one() {
        // A driver reserving 256 MiB: used + free is legitimately below total,
        // and PULSE must not "correct" the driver by deriving free itself.
        let reserved = 256 * 1024 * 1024;
        let reading =
            GpuMemoryReading::from_total_used_and_free(8 * GIB, 2 * GIB, 6 * GIB - reserved)
                .expect("valid");

        assert_eq!(reading.free(), 6 * GIB - reserved);
        assert_ne!(reading.free(), reading.total() - reading.used());
    }

    #[test]
    fn the_percentage_is_computed_from_used_and_total() {
        let reading = GpuMemoryReading::from_total_and_used(8 * GIB, 6 * GIB).expect("valid");
        assert_eq!(reading.usage_percent(), 75.0);

        let reading = GpuMemoryReading::from_total_and_used(1_000, 333).expect("valid");
        assert!((reading.usage_percent() - 33.3).abs() < 1e-9);
    }

    #[test]
    fn the_boundaries_are_exact() {
        let empty = GpuMemoryReading::from_total_and_used(8 * GIB, 0).expect("valid");
        assert_eq!(empty.usage_percent(), 0.0);
        assert_eq!(empty.free(), 8 * GIB);

        let full = GpuMemoryReading::from_total_and_used(8 * GIB, 8 * GIB).expect("valid");
        assert_eq!(full.usage_percent(), 100.0);
        assert_eq!(full.free(), 0);
    }

    #[test]
    fn a_zero_total_is_refused_rather_than_dividing_by_it() {
        // Would otherwise produce NaN and reach the frontend as JSON null.
        let error = GpuMemoryReading::from_total_and_used(0, 0).expect_err("must be rejected");

        assert_eq!(error.code, MetricErrorCode::Parse);
        assert!(error.message.contains("zero"));

        assert!(GpuMemoryReading::from_total_used_and_free(0, 0, 0).is_err());
    }

    #[test]
    fn used_above_total_is_refused_rather_than_published_above_one_hundred() {
        let error =
            GpuMemoryReading::from_total_and_used(8 * GIB, 9 * GIB).expect_err("must be rejected");

        assert_eq!(error.code, MetricErrorCode::Parse);
        assert!(error.recoverable, "a bad read may well be transient");

        assert!(GpuMemoryReading::from_total_used_and_free(8 * GIB, 9 * GIB, 0).is_err());
    }

    #[test]
    fn free_above_total_is_refused() {
        assert!(GpuMemoryReading::from_total_used_and_free(8 * GIB, 0, 9 * GIB).is_err());
    }

    #[test]
    fn used_plus_free_exceeding_total_is_refused() {
        // A self-contradicting source: reconciling it silently would publish a
        // number PULSE cannot stand behind.
        let error = GpuMemoryReading::from_total_used_and_free(8 * GIB, 5 * GIB, 5 * GIB)
            .expect_err("must be rejected");

        assert!(error.message.contains("exceeds total"));
    }

    #[test]
    fn an_overflowing_sum_cannot_slip_through_as_consistent() {
        // used + free wraps to a small number without checked arithmetic.
        assert!(GpuMemoryReading::from_total_used_and_free(8 * GIB, u64::MAX, u64::MAX).is_err());
        assert!(GpuMemoryReading::from_total_used_and_free(u64::MAX, u64::MAX, 2).is_err());
    }

    #[test]
    fn every_published_value_is_finite_and_in_range() {
        // The contract the frontend relies on: no NaN, no infinity, no
        // negative, nothing above 100.
        for (total, used) in [
            (8 * GIB, 0),
            (8 * GIB, 8 * GIB),
            (1, 1),
            (u64::MAX, u64::MAX),
            (u64::MAX, 1),
            (24 * GIB, 17 * GIB),
        ] {
            let reading = GpuMemoryReading::from_total_and_used(total, used).expect("valid");
            let percent = reading.usage_percent();

            assert!(percent.is_finite(), "{total}/{used} produced {percent}");
            assert!((0.0..=100.0).contains(&percent));
            assert!(reading.free() <= reading.total());
            assert!(reading.used() <= reading.total());
        }
    }

    #[test]
    fn a_realistic_eight_gigabyte_card_reads_correctly() {
        // 8 GiB card with 1.8 GiB in use, as NVML would report it.
        let total = 8_589_934_592_u64;
        let used = 1_932_735_283_u64;
        let reading =
            GpuMemoryReading::from_total_used_and_free(total, used, total - used).expect("valid");

        assert_eq!(reading.free(), total - used);
        assert!((reading.usage_percent() - 22.5).abs() < 0.1);
    }
}
