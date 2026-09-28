//! Tauri commands for the live widget feed.
//!
//! Each window has **one** subscription — the union of what its visible
//! widgets show — identified by its label. The backend unions the windows and
//! samples once per tick; see `crate::live`.

use std::sync::Arc;

use tauri::{Emitter, State, WebviewWindow};

use crate::live::{LiveEventSink, LiveSeries, LiveService, LiveTick, SubscriptionResult};
use crate::metrics::MetricRef;

/// Emitted once per tick to every window.
pub const LIVE_EVENT: &str = "live-sample";

/// Forwards ticks to the webviews.
pub struct TauriLiveEvents(pub tauri::AppHandle);

impl LiveEventSink for TauriLiveEvents {
    fn tick(&self, tick: &LiveTick) {
        if let Err(error) = self.0.emit(LIVE_EVENT, tick) {
            eprintln!("PULSE: could not emit {LIVE_EVENT}: {error}");
        }
    }
}

/// Replaces this window's live subscription. An empty list unsubscribes —
/// what a hidden window does.
#[tauri::command]
pub fn set_live_subscription(
    live: State<'_, Arc<LiveService>>,
    window: WebviewWindow,
    metrics: Vec<MetricRef>,
) -> SubscriptionResult {
    live.set_subscription(window.label(), &metrics)
}

/// What the in-memory rings hold for `metrics` — the last few minutes, so a
/// sparkline does not start empty.
#[tauri::command]
pub fn get_live_buffer(
    live: State<'_, Arc<LiveService>>,
    metrics: Vec<MetricRef>,
) -> Vec<LiveSeries> {
    live.hub().buffer(&metrics)
}
