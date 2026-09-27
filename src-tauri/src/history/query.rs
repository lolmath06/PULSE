//! What the UI may ask of history, and what it gets back.
//!
//! # Bounded by construction
//!
//! A range is one of five fixed windows, and each is answered at a resolution
//! chosen so that **no series ever exceeds about [`TARGET_POINTS`] points**,
//! however long PULSE has been recording. The UI can never ask for 50 000 raw
//! rows; the backend decides the bucket.
//!
//! | Range | Bucket (5 s cadence) | Points at most |
//! | ----- | -------------------- | -------------- |
//! | 15 m  | raw 5 s              | 180            |
//! | 1 h   | raw 5 s              | 720            |
//! | 6 h   | 30 s                 | 720            |
//! | 24 h  | 2 min                | 720            |
//! | 7 d   | 15 min               | 672            |
//!
//! # Buckets keep the extremes
//!
//! An aggregated point carries `min`, `max`, the sample-weighted average and
//! the sample count — never the average alone, so a one-sample spike to 100 %
//! inside a two-minute bucket still reaches the chart as that bucket's `max`.

use serde::{Deserialize, Serialize};

use crate::metrics::model::MetricRef;

/// The most points a series should carry to the UI.
pub const TARGET_POINTS: i64 = 720;

/// Bucket sizes a plan may choose from, in milliseconds.
///
/// "Nice" values, so bucket boundaries land on round clock times.
const BUCKET_LADDER_MS: &[i64] = &[
    1_000, 5_000, 10_000, 15_000, 30_000, 60_000, 120_000, 300_000, 600_000, 900_000, 1_800_000,
    3_600_000,
];

/// How many expected intervals may pass before the chart shows a gap.
///
/// Three: one late batch is jitter, three missing ones is an absence — the
/// machine slept, PULSE was closed, or the metric stopped being available.
pub const GAP_FACTOR: i64 = 3;

/// A visible time window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HistoryRange {
    #[serde(rename = "15m")]
    FifteenMinutes,
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "6h")]
    SixHours,
    #[serde(rename = "24h")]
    OneDay,
    #[serde(rename = "7d")]
    SevenDays,
}

impl HistoryRange {
    pub const ALL: [HistoryRange; 5] = [
        HistoryRange::FifteenMinutes,
        HistoryRange::OneHour,
        HistoryRange::SixHours,
        HistoryRange::OneDay,
        HistoryRange::SevenDays,
    ];

    pub const fn duration_ms(self) -> i64 {
        const MINUTE: i64 = 60_000;
        match self {
            HistoryRange::FifteenMinutes => 15 * MINUTE,
            HistoryRange::OneHour => 60 * MINUTE,
            HistoryRange::SixHours => 6 * 60 * MINUTE,
            HistoryRange::OneDay => 24 * 60 * MINUTE,
            HistoryRange::SevenDays => 7 * 24 * 60 * MINUTE,
        }
    }
}

/// How one range is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryPlan {
    pub from_ms: i64,
    pub to_ms: i64,
    /// Width of one returned point. Equal to the cadence for raw answers.
    pub bucket_ms: i64,
    /// True when every point is one real sample with its exact timestamp.
    pub raw: bool,
    /// A step between consecutive points larger than this is a gap.
    pub gap_threshold_ms: i64,
}

impl QueryPlan {
    /// Plans `range`, ending at `now_ms`, for a recorder ticking every
    /// `cadence_ms`.
    pub fn new(range: HistoryRange, now_ms: i64, cadence_ms: i64) -> Self {
        let duration = range.duration_ms();
        let ideal = (duration + TARGET_POINTS - 1) / TARGET_POINTS;
        let floor = ideal.max(cadence_ms);
        let bucket_ms = BUCKET_LADDER_MS
            .iter()
            .copied()
            .find(|bucket| *bucket >= floor)
            .unwrap_or(floor);
        let raw = bucket_ms <= cadence_ms;

        Self {
            from_ms: now_ms - duration,
            to_ms: now_ms,
            bucket_ms,
            raw,
            gap_threshold_ms: bucket_ms * GAP_FACTOR,
        }
    }
}

/// One point of a series.
///
/// For a raw answer `min`, `max` and `count` are omitted: the point *is* the
/// sample. For an aggregated one, `value` is the sample-weighted average of the
/// bucket and `t` is the bucket's start.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    /// UTC epoch milliseconds.
    pub t: i64,
    #[serde(rename = "v")]
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max: Option<f64>,
    #[serde(rename = "n", skip_serializing_if = "Option::is_none", default)]
    pub count: Option<u32>,
}

impl HistoryPoint {
    pub const fn raw(t: i64, value: f64) -> Self {
        Self {
            t,
            value,
            min: None,
            max: None,
            count: None,
        }
    }
}

/// The most recent real sample of a series inside the window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestSample {
    pub t: i64,
    #[serde(rename = "v")]
    pub value: f64,
}

/// One metric's history over the window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySeries {
    pub metric: MetricRef,
    /// Ascending by `t`. Gaps are **absent points**, never zeros.
    pub points: Vec<HistoryPoint>,
    /// The last recorded sample in the window, exact even when `points` is
    /// aggregated — this is what a visualization shows as *current*.
    pub latest: Option<LatestSample>,
    /// Rows that could not be read and were skipped.
    #[serde(skip_serializing_if = "is_zero", default)]
    pub skipped_rows: u32,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// The answer to one history request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryResponse {
    pub range: HistoryRange,
    #[serde(flatten)]
    pub plan: QueryPlan,
    /// One per requested metric, in request order.
    pub series: Vec<HistorySeries>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;

    #[test]
    fn ranges_serialise_as_their_short_names() {
        let names: Vec<String> = HistoryRange::ALL
            .iter()
            .map(|range| serde_json::to_string(range).expect("serialise"))
            .collect();
        assert_eq!(names, ["\"15m\"", "\"1h\"", "\"6h\"", "\"24h\"", "\"7d\""]);
        assert_eq!(
            serde_json::from_str::<HistoryRange>("\"6h\"").expect("parse"),
            HistoryRange::SixHours
        );
        assert!(serde_json::from_str::<HistoryRange>("\"2y\"").is_err());
    }

    #[test]
    fn the_documented_resolution_table_holds_at_five_seconds() {
        let plan = |range| QueryPlan::new(range, NOW, 5_000);

        assert_eq!(plan(HistoryRange::FifteenMinutes).bucket_ms, 5_000);
        assert!(plan(HistoryRange::FifteenMinutes).raw);
        assert_eq!(plan(HistoryRange::OneHour).bucket_ms, 5_000);
        assert!(plan(HistoryRange::OneHour).raw);
        assert_eq!(plan(HistoryRange::SixHours).bucket_ms, 30_000);
        assert!(!plan(HistoryRange::SixHours).raw);
        assert_eq!(plan(HistoryRange::OneDay).bucket_ms, 120_000);
        assert_eq!(plan(HistoryRange::SevenDays).bucket_ms, 900_000);
    }

    #[test]
    fn no_range_can_exceed_the_point_budget_at_any_supported_cadence() {
        for cadence in crate::history::Cadence::ALL {
            for range in HistoryRange::ALL {
                let plan = QueryPlan::new(range, NOW, cadence.millis());
                let points = range.duration_ms() / plan.bucket_ms + 1;
                assert!(
                    points <= TARGET_POINTS + 1,
                    "{range:?} at {cadence:?}: {points} points"
                );
                assert!(plan.bucket_ms >= cadence.millis());
            }
        }
    }

    #[test]
    fn a_raw_answer_gaps_after_fifteen_seconds_at_five_second_cadence() {
        let plan = QueryPlan::new(HistoryRange::FifteenMinutes, NOW, 5_000);
        assert_eq!(plan.gap_threshold_ms, 15_000);
        assert_eq!(plan.to_ms - plan.from_ms, 15 * 60_000);
    }

    #[test]
    fn a_raw_point_omits_its_aggregate_fields() {
        let json = serde_json::to_value(HistoryPoint::raw(5, 1.5)).expect("serialise");
        assert_eq!(json, serde_json::json!({ "t": 5, "v": 1.5 }));
    }
}
