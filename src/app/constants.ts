/**
 * Version displayed by the UI.
 *
 * The authoritative version lives in `src-tauri/Cargo.toml` and reaches the UI
 * through `get_platform_info().appVersion`. This constant is the fallback used
 * before the backend has answered, or when the UI runs in a plain browser.
 */
export const APP_VERSION = '0.1.0-dev';

export const APP_TAGLINE = 'Your system, at a glance.';
