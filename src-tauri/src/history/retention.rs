//! How long history is kept, and at what resolution.
//!
//! ```text
//!  now ─────────── 24 h ────────────────────────── 7 d ──►  older
//!  │ raw samples (every 5 s)   │ 1-minute aggregates     │ deleted
//!  │ exact values              │ min · max · avg · count │
//! ```
//!
//! Raw rows older than 24 hours are folded into one-minute buckets and then
//! deleted — **in the same transaction**, so a raw row is never removed unless
//! its aggregate was written. Aggregates older than 7 days are deleted.
//!
//! Compaction runs at startup and then once an hour from the scheduler thread,
//! never on every tick: a `DELETE` every five seconds would rewrite pages
//! constantly for no benefit.

use std::time::Duration;

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;

/// The retention rules the store applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionPolicy {
    /// Raw samples younger than this are kept as recorded.
    pub raw_ms: i64,
    /// Aggregates younger than this are kept; older ones are deleted.
    pub aggregate_ms: i64,
    /// The width of one aggregate bucket.
    pub bucket_ms: i64,
    /// How often the scheduler runs compaction after the startup pass.
    pub interval: Duration,
}

impl RetentionPolicy {
    /// PULSE's default: raw for 24 hours, one-minute aggregates for 7 days,
    /// compacted hourly.
    pub const DEFAULT: RetentionPolicy = RetentionPolicy {
        raw_ms: DAY_MS,
        aggregate_ms: 7 * DAY_MS,
        bucket_ms: MINUTE_MS,
        interval: Duration::from_secs(3_600),
    };

    /// Raw rows strictly older than this are compacted.
    ///
    /// Aligned down to a bucket boundary so a minute is never split between a
    /// raw half and an aggregated half.
    pub fn raw_cutoff(&self, now_ms: i64) -> i64 {
        let cutoff = now_ms - self.raw_ms;
        cutoff - cutoff.rem_euclid(self.bucket_ms)
    }

    /// Aggregates whose bucket starts strictly before this are deleted.
    pub fn aggregate_cutoff(&self, now_ms: i64) -> i64 {
        now_ms - self.aggregate_ms
    }
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_keeps_a_day_raw_and_a_week_aggregated() {
        let policy = RetentionPolicy::DEFAULT;
        assert_eq!(policy.raw_ms, 86_400_000);
        assert_eq!(policy.aggregate_ms, 7 * 86_400_000);
        assert_eq!(policy.bucket_ms, 60_000);
        assert_eq!(policy.interval, Duration::from_secs(3_600));
    }

    #[test]
    fn the_raw_cutoff_lands_on_a_minute_boundary() {
        let policy = RetentionPolicy::DEFAULT;
        let now = 1_800_000_012_345;
        let cutoff = policy.raw_cutoff(now);

        assert_eq!(cutoff % 60_000, 0);
        assert!(cutoff <= now - policy.raw_ms);
        assert!(now - policy.raw_ms - cutoff < 60_000);
    }
}
