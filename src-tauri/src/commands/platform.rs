//! Commands exposing host platform information to the UI.

use crate::platform::PlatformInfo;
use crate::services::platform_info;

/// Returns structured information about the host PULSE is running on.
///
/// This is the Phase 0 integration check for the
/// `React -> Tauri -> Rust -> React` round trip; it is not a metrics source.
#[tauri::command]
pub fn get_platform_info() -> PlatformInfo {
    platform_info::collect()
}
