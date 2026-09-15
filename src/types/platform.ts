/**
 * Mirror of the Rust `PlatformInfo` payload returned by `get_platform_info`.
 *
 * Keep this in sync with `src-tauri/src/platform/mod.rs`. The frontend never
 * inspects the host system directly; every system-facing value crosses this
 * boundary as a structured payload.
 */
export type PlatformKind = 'windows' | 'linux' | 'unsupported';

export interface PlatformInfo {
  /** Coarse platform family PULSE has a backend implementation for. */
  readonly platform: PlatformKind;
  /** Rust `std::env::consts::OS`, e.g. `linux`, `windows`. */
  readonly os: string;
  /** Rust `std::env::consts::ARCH`, e.g. `x86_64`, `aarch64`. */
  readonly arch: string;
  /** Human readable distribution / OS name when detectable. */
  readonly osVersion: string | null;
  /** Linux display server, when known: `wayland`, `x11` or null elsewhere. */
  readonly displayServer: string | null;
  /** PULSE version, sourced from Cargo.toml at compile time. */
  readonly appVersion: string;
}
