//! Windows CPU usage, read via `GetSystemTimes`.
//!
//! The arithmetic is deliberately separated from the FFI. Everything outside
//! the `imp` module is pure and compiles on every platform, so the Windows CPU
//! semantics are unit-tested from Fedora; only the thin `unsafe` wrapper needs
//! a Windows host.

use crate::metrics::wellknown::cpu::CpuCounters;

/// Identifier of the Windows CPU provider.
pub const PROVIDER_ID: &str = "windows.cpu";

/// Combines the two halves of a `FILETIME` into one 64-bit count.
///
/// A `FILETIME` is a 64-bit value in 100-nanosecond units, split across two
/// 32-bit fields because the struct predates guaranteed 64-bit alignment.
/// Microsoft's documentation is explicit that it must not be cast directly to
/// a `u64`; the halves are recombined instead.
pub const fn filetime_to_u64(filetime_low: u32, filetime_high: u32) -> u64 {
    ((filetime_high as u64) << 32) | (filetime_low as u64)
}

/// Derives platform-neutral CPU counters from Windows' three time totals.
///
/// **Windows counts idle time inside `kernel`**, which is the trap here: the
/// Linux formula would report a completely idle machine as heavily busy.
///
/// ```text
/// total = kernel + user      (kernel already contains idle)
/// busy  = total - idle
/// ```
///
/// Returns `None` when the values are inconsistent — idle exceeding the total,
/// or an arithmetic overflow — so a nonsensical reading becomes an error
/// rather than a percentage above 100.
pub fn counters_from_system_times(idle: u64, kernel: u64, user: u64) -> Option<CpuCounters> {
    let total = kernel.checked_add(user)?;
    let busy = total.checked_sub(idle)?;

    Some(CpuCounters::new(busy, total))
}

/// The parts that call into the Windows API.
#[cfg(target_os = "windows")]
mod imp {
    use std::sync::Arc;

    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::GetSystemTimes;

    use crate::metrics::model::{
        Availability, MetricDefinition, MetricError, MetricErrorCode, MetricRef, MetricSample,
        ProviderId,
    };
    use crate::metrics::providers::MetricProvider;
    use crate::metrics::wellknown::availability_for;
    use crate::metrics::wellknown::cpu::{self, CpuCounters, CpuUsage, CpuUsageTracker};

    use super::{counters_from_system_times, filetime_to_u64, PROVIDER_ID};

    /// Calls `GetSystemTimes` and converts the result.
    ///
    /// # Safety
    ///
    /// The single `unsafe` call writes three `FILETIME` values through pointers
    /// to local variables alive for the whole call. `GetSystemTimes` accepts
    /// exactly these three out-parameters and writes nothing else. The return
    /// value is checked before the outputs are read, so a failed call never
    /// yields uninitialised data.
    fn read_counters() -> Result<CpuCounters, MetricError> {
        let mut idle = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut kernel = idle;
        let mut user = idle;

        // SAFETY: see the function docs. All three pointers reference live
        // locals, and the result is checked before the values are read.
        let ok = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };

        if ok == 0 {
            return Err(MetricError::new(
                MetricErrorCode::Io,
                format!(
                    "GetSystemTimes failed ({})",
                    std::io::Error::last_os_error()
                ),
            ));
        }

        let idle = filetime_to_u64(idle.dwLowDateTime, idle.dwHighDateTime);
        let kernel = filetime_to_u64(kernel.dwLowDateTime, kernel.dwHighDateTime);
        let user = filetime_to_u64(user.dwLowDateTime, user.dwHighDateTime);

        counters_from_system_times(idle, kernel, user).ok_or_else(|| {
            MetricError::new(
                MetricErrorCode::Parse,
                format!(
                    "inconsistent CPU times from GetSystemTimes \
                     (idle={idle}, kernel={kernel}, user={user})"
                ),
            )
            .with_recoverable(true)
        })
    }

    /// Publishes `cpu.usage.total` on Windows.
    ///
    /// Requires no administrator rights: `GetSystemTimes` is available to any
    /// process.
    #[derive(Debug)]
    pub struct WindowsCpuProvider {
        id: ProviderId,
        tracker: CpuUsageTracker,
    }

    impl WindowsCpuProvider {
        /// Builds the provider and captures a CPU baseline immediately.
        ///
        /// Same reasoning as on Linux: priming at construction avoids a
        /// blocking sleep inside the first request.
        pub fn new() -> Self {
            let tracker = CpuUsageTracker::new();

            if let Ok(counters) = read_counters() {
                tracker.prime(counters);
            }

            Self {
                id: ProviderId::new(PROVIDER_ID).expect("provider id must be valid"),
                tracker,
            }
        }
    }

    impl Default for WindowsCpuProvider {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MetricProvider for WindowsCpuProvider {
        fn id(&self) -> &ProviderId {
            &self.id
        }

        fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError> {
            Ok(cpu::definitions(&self.id))
        }

        fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError> {
            let outcome = read_counters().and_then(|counters| self.tracker.update(counters));

            let samples = requested
                .iter()
                .map(|reference| match &outcome {
                    Ok(CpuUsage::Ready(percent)) => {
                        MetricSample::number(reference.clone(), *percent)
                    }
                    Ok(CpuUsage::NeedsAnotherSample(reason)) => MetricSample::unavailable(
                        reference.clone(),
                        Availability::temporarily_unavailable(reason.reason()),
                    ),
                    Err(error) => MetricSample::unavailable(
                        reference.clone(),
                        availability_for(error.clone()),
                    ),
                })
                .collect();

            Ok(samples)
        }
    }

    /// Builds the Windows CPU provider.
    pub fn provider() -> Arc<dyn MetricProvider> {
        Arc::new(WindowsCpuProvider::new())
    }
}

#[cfg(target_os = "windows")]
pub use imp::{provider, WindowsCpuProvider};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::wellknown::cpu::{CpuUsage, CpuUsageTracker, NeedsAnotherSample};

    #[test]
    fn recombines_the_two_halves_of_a_filetime() {
        assert_eq!(filetime_to_u64(0, 0), 0);
        assert_eq!(filetime_to_u64(1, 0), 1);
        assert_eq!(filetime_to_u64(0, 1), 1_u64 << 32);
        assert_eq!(filetime_to_u64(u32::MAX, 0), u32::MAX as u64);
        assert_eq!(filetime_to_u64(u32::MAX, u32::MAX), u64::MAX);
        // A concrete interleaved value.
        assert_eq!(
            filetime_to_u64(0x89AB_CDEF, 0x0123_4567),
            0x0123_4567_89AB_CDEF
        );
    }

    #[test]
    fn subtracts_idle_from_kernel_rather_than_adding_it() {
        // The Windows-specific trap: idle is already inside kernel.
        // kernel=800 (of which 700 idle), user=200 -> total 1000, busy 300.
        let counters = counters_from_system_times(700, 800, 200).expect("consistent");

        assert_eq!(counters.total, 1000);
        assert_eq!(counters.busy, 300);
        assert!(counters.is_consistent());
    }

    #[test]
    fn a_fully_idle_machine_reports_zero_busy() {
        let counters = counters_from_system_times(1000, 1000, 0).expect("consistent");

        assert_eq!(counters.total, 1000);
        assert_eq!(counters.busy, 0);
    }

    #[test]
    fn a_fully_busy_machine_reports_all_busy() {
        let counters = counters_from_system_times(0, 400, 600).expect("consistent");

        assert_eq!(counters.total, 1000);
        assert_eq!(counters.busy, 1000);
    }

    #[test]
    fn rejects_idle_larger_than_the_total() {
        assert!(counters_from_system_times(2000, 1000, 0).is_none());
    }

    #[test]
    fn rejects_an_overflowing_total() {
        assert!(counters_from_system_times(0, u64::MAX, 1).is_none());
    }

    #[test]
    fn a_delta_between_two_readings_gives_a_usage_percentage() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(counters_from_system_times(700, 800, 200).expect("consistent"));

        // Later: kernel +300 (of which 250 idle), user +50.
        // total delta 350, busy delta 100 -> ~28.57%.
        let outcome = tracker
            .update(counters_from_system_times(950, 1100, 250).expect("consistent"))
            .expect("no error");

        let CpuUsage::Ready(percent) = outcome else {
            panic!("expected a usable percentage, got {outcome:?}");
        };
        assert!((percent - 28.571_428).abs() < 1e-4, "got {percent}");
    }

    #[test]
    fn identical_readings_wait_instead_of_reporting_idle() {
        let counters = counters_from_system_times(700, 800, 200).expect("consistent");
        let tracker = CpuUsageTracker::new();
        tracker.prime(counters);

        assert_eq!(
            tracker.update(counters).expect("no error"),
            CpuUsage::NeedsAnotherSample(NeedsAnotherSample::NoElapsedTime)
        );
    }

    #[test]
    fn rewound_counters_reset_the_baseline() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(counters_from_system_times(7000, 8000, 2000).expect("consistent"));

        assert_eq!(
            tracker
                .update(counters_from_system_times(700, 800, 200).expect("consistent"))
                .expect("no error"),
            CpuUsage::NeedsAnotherSample(NeedsAnotherSample::CountersWentBackwards)
        );
    }

    #[test]
    fn usage_is_always_clamped_into_range() {
        let tracker = CpuUsageTracker::new();
        tracker.prime(counters_from_system_times(0, 1000, 0).expect("consistent"));

        // Every additional tick is busy: 100%, never more.
        let CpuUsage::Ready(percent) = tracker
            .update(counters_from_system_times(0, 2000, 0).expect("consistent"))
            .expect("no error")
        else {
            panic!("expected a percentage");
        };

        assert!((0.0..=100.0).contains(&percent));
        assert_eq!(percent, 100.0);
    }
}
