//! Cross-platform application logic.
//!
//! Services orchestrate work and hold no OS-specific code: anything that
//! differs between Windows and Linux is resolved by the `platform` layer.

pub mod metrics;
pub mod platform_info;
