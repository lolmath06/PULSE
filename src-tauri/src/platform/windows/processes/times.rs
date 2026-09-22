//! `FILETIME` arithmetic for the Windows process collector.
//!
//! Pure, and therefore compiled and unit-tested on **every** host. There is no
//! reason for Windows time arithmetic to be untestable from a Fedora
//! workstation, and every reason for it not to be: an off-by-a-hundred here
//! would make every Windows CPU percentage wrong by two orders of magnitude,
//! and nobody would notice until a Windows machine was available.
//!
//! # A FILETIME is two 32-bit halves of one 64-bit number
//!
//! `GetProcessTimes` fills four `FILETIME` structures, each a
//! `{ dwLowDateTime, dwHighDateTime }` pair that must be recombined as
//! `high << 32 | low`. Reading `dwLowDateTime` alone — which looks plausible,
//! because it is usually the only half that changes — wraps every seven
//! minutes of CPU time.
//!
//! # The unit is 100 nanoseconds
//!
//! Not milliseconds, not nanoseconds: `FILETIME` counts 100 ns intervals. The
//! two CPU figures are converted to nanoseconds here so the shared rate
//! arithmetic sees the same unit it gets from Linux clock ticks.
//!
//! # Creation time is the identity, not a date
//!
//! `ftCreationTime` is 100 ns intervals since 1 January 1601 UTC. PULSE never
//! turns it into a date; it only ever compares it, as the start token half of
//! a [`ProcessInstanceId`]. That is exactly what makes a recycled PID a
//! different process.
//!
//! [`ProcessInstanceId`]: crate::processes::ProcessInstanceId

/// Recombines a `FILETIME`'s two halves into the single number it is.
pub const fn filetime_to_u64(low: u32, high: u32) -> u64 {
    ((high as u64) << 32) | (low as u64)
}

/// Converts a count of 100 ns intervals to nanoseconds.
///
/// Returns `None` on overflow rather than wrapping: a wrapped total would
/// become a small number and read as a plausible, tiny CPU time.
pub const fn hundred_nanos_to_nanos(ticks: u64) -> Option<u64> {
    ticks.checked_mul(100)
}

/// Total CPU time of a process, in nanoseconds.
///
/// Kernel time plus user time — the two halves of what the process actually
/// executed. `ftCreationTime` and `ftExitTime` are wall-clock stamps and are
/// deliberately not part of this sum.
pub fn cpu_time_nanos(kernel_100ns: u64, user_100ns: u64) -> Option<u64> {
    hundred_nanos_to_nanos(kernel_100ns.checked_add(user_100ns)?)
}

/// The start token half of a process's identity.
///
/// The raw creation `FILETIME`, uninterpreted. Two processes that share a PID
/// cannot share this: Windows stamps it at creation with 100 ns resolution and
/// never rewrites it.
pub const fn start_token(creation_low: u32, creation_high: u32) -> u64 {
    filetime_to_u64(creation_low, creation_high)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filetime_is_recombined_from_both_halves() {
        assert_eq!(filetime_to_u64(0, 0), 0);
        assert_eq!(filetime_to_u64(1, 0), 1);
        assert_eq!(filetime_to_u64(0, 1), 1 << 32);
        assert_eq!(filetime_to_u64(u32::MAX, u32::MAX), u64::MAX);
        assert_eq!(
            filetime_to_u64(0x89AB_CDEF, 0x0123_4567),
            0x0123_4567_89AB_CDEF
        );
    }

    #[test]
    fn reading_only_the_low_half_would_have_wrapped() {
        // The bug this function exists to prevent, stated as a test. 0x1_0000_0000
        // hundred-nanosecond intervals is about seven minutes of CPU time.
        let correct = filetime_to_u64(0, 1);
        assert_ne!(correct, 0, "the low half alone reads as zero here");
        assert_eq!(correct, 4_294_967_296);
    }

    #[test]
    fn one_hundred_nanosecond_intervals_become_nanoseconds() {
        assert_eq!(hundred_nanos_to_nanos(0), Some(0));
        assert_eq!(hundred_nanos_to_nanos(1), Some(100));
        // One second of CPU time.
        assert_eq!(hundred_nanos_to_nanos(10_000_000), Some(1_000_000_000));
    }

    #[test]
    fn an_overflowing_conversion_yields_nothing_rather_than_a_wrapped_number() {
        assert_eq!(hundred_nanos_to_nanos(u64::MAX), None);
        assert_eq!(cpu_time_nanos(u64::MAX, 1), None);
        assert_eq!(cpu_time_nanos(u64::MAX / 2, u64::MAX / 2), None);
    }

    #[test]
    fn cpu_time_is_kernel_plus_user() {
        // 2 s kernel, 3 s user.
        let kernel = 20_000_000;
        let user = 30_000_000;
        assert_eq!(cpu_time_nanos(kernel, user), Some(5_000_000_000));
    }

    #[test]
    fn a_brand_new_process_has_no_cpu_time_and_that_is_a_real_zero() {
        assert_eq!(cpu_time_nanos(0, 0), Some(0));
    }

    #[test]
    fn the_start_token_is_the_raw_creation_filetime() {
        // 2024-01-01T00:00:00Z is 0x01DA_3C24_5C9C_0000 in FILETIME terms; the
        // exact value does not matter, only that both halves survive.
        assert_eq!(start_token(0x5C9C_0000, 0x01DA_3C24), 0x01DA_3C24_5C9C_0000);
        assert_eq!(start_token(0, 0), 0, "the idle process starts at zero");
    }

    #[test]
    fn two_processes_created_a_tick_apart_get_different_tokens() {
        assert_ne!(start_token(1, 0), start_token(2, 0));
        assert_ne!(start_token(u32::MAX, 0), start_token(0, 1));
    }

    #[test]
    fn identity_survives_pid_reuse() {
        use crate::processes::ProcessInstanceId;

        let first = ProcessInstanceId::new(1234, start_token(0x1111_1111, 0x01DA_3C24));
        let second = ProcessInstanceId::new(1234, start_token(0x2222_2222, 0x01DA_3C24));

        assert_ne!(
            first, second,
            "the same PID created at two instants is two processes"
        );
    }
}
