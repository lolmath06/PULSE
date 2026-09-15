//! Application state shared across Tauri commands.
//!
//! Phase 0 keeps this deliberately empty: it exists so that later phases
//! (metrics engine, history buffers, dashboard configuration) have an obvious,
//! already-wired place to live instead of introducing globals.

/// Root state object managed by Tauri.
#[derive(Debug, Default)]
pub struct AppState {}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }
}
