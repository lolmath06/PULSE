//! Desktop overlays: the platform-independent core.
//!
//! An overlay is a real, separate Tauri window — frameless, transparent, above
//! other windows where the platform allows, click-through when locked where the
//! platform allows — that renders widgets with the same engine as the
//! dashboard. It is a safe desktop window: no injection into games, no hooking
//! of DirectX, OpenGL or Vulkan.
//!
//! This module holds every decision that can be made without a window, so the
//! Windows harness type checks it and tests cover it on Fedora:
//!
//! - [`capabilities`] — what this session can honestly do, per display server;
//! - [`geometry`] — monitor-relative logical placement, DPI, recovery;
//! - [`spec`] — the backend-owned fields of the `overlays` configuration;
//! - [`settings`] — close behaviour and the global shortcut, with conflict
//!   handling;
//! - [`global_shortcut`] — which shortcut backend a session uses, and the XDG
//!   Desktop Portal shortcut session as a state machine.
//!
//! The Tauri calls that apply these decisions live in `crate::desktop`, which
//! contains no `cfg(target_os)` branch: platform differences are data here.

pub mod capabilities;
pub mod geometry;
pub mod global_shortcut;
pub mod settings;
pub mod spec;
