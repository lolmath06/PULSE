//! Tauri command surface.
//!
//! This is the *only* API the frontend can call. Commands stay thin: they
//! validate input, delegate to `services`, and return serialisable payloads.
//! Platform branching never happens here.

pub mod desktop;
pub mod history;
pub mod live;
pub mod metrics;
pub mod platform;
pub mod process_control;
pub mod processes;
pub mod ui_config;
