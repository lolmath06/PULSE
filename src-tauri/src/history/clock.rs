//! Wall-clock time, injectable.
//!
//! History timestamps are **UTC epoch milliseconds** from the wall clock: they
//! must mean the same instant after a restart, a reboot or a time-zone change.
//! Scheduling, by contrast, uses the monotonic clock (see
//! [`super::sampler::TickSchedule`]) so that a clock step never makes the
//! scheduler fire a burst of catch-up batches.
//!
//! Everything that reads the wall clock takes a [`Clock`], so tests drive
//! `t0`, `+5 s`, `+10 s`, a two-hour gap and a relaunch without waiting.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A source of UTC epoch milliseconds.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}

/// The real wall clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
            // A clock set before 1970 is broken; 0 sorts before every real
            // sample rather than panicking.
            .unwrap_or(0)
    }
}

/// A clock that only moves when told to.
#[derive(Debug, Default)]
pub struct ManualClock(AtomicI64);

impl ManualClock {
    pub fn new(start_ms: i64) -> Self {
        Self(AtomicI64::new(start_ms))
    }

    pub fn set(&self, ms: i64) {
        self.0.store(ms, Ordering::SeqCst);
    }

    pub fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_clock_reads_a_plausible_epoch() {
        let now = SystemClock.now_ms();
        assert!(now > 1_577_836_800_000, "after 2020");
        assert!(now < 4_102_444_800_000, "before 2100");
    }

    #[test]
    fn a_manual_clock_moves_only_when_told() {
        let clock = ManualClock::new(1_000);
        assert_eq!(clock.now_ms(), 1_000);

        clock.advance(5_000);
        assert_eq!(clock.now_ms(), 6_000);

        clock.set(42);
        assert_eq!(clock.now_ms(), 42);
    }
}
