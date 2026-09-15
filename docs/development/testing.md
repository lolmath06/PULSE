# Testing

PULSE mixes automated checks with deliberate manual validation, because a system
monitor's most important behaviour — reading real hardware on a real OS — cannot
be faked in CI.

## Automated

### Rust — `pnpm rust:test`

Unit tests live next to the code in `#[cfg(test)]` modules.

Phase 0 covers:

- platform detection matches the compilation target;
- `host()` returns an implementation consistent with `PlatformKind::current()`;
- `PlatformInfo` serialises as camelCase, so the Rust payload and
  `src/types/platform.ts` cannot silently drift apart;
- Linux display-server detection across Wayland, X11 and headless sessions;
- `/etc/os-release` parsing, including quoted, unquoted, empty and absent values;
- Windows version formatting, including the Windows 10 / 11 build-number split.

Parsing and detection logic is written as **pure functions taking their inputs
as arguments** (see `detect_display_server`, `parse_pretty_name`,
`format_os_version`) precisely so it can be tested without the host it describes.
Follow that pattern: it is what lets Windows logic be reasoned about from a
Fedora machine and vice versa.

### Frontend — `pnpm test`

Vitest with Testing Library, jsdom environment.

Phase 0 covers the navigation shell (every mode is reachable, routes render the
right page), display formatting helpers, and — importantly — that the UI
degrades gracefully when the Tauri backend is unreachable.

### Static checks

`pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm rust:fmt`,
`pnpm rust:lint`. All of it at once: `pnpm check:all`.

## CI

`.github/workflows/ci.yml` runs on push and pull request, with jobs on
**`ubuntu-latest`** and **`windows-latest`**: install, lint, typecheck, frontend
tests, frontend build, `cargo fmt --check`, `cargo clippy -D warnings`,
`cargo test`, and a `cargo build` of the Tauri backend.

### What CI does _not_ prove

> **`ubuntu-latest` is not a Fedora test.**

It catches Linux regressions — broken `cfg` gating, missing paths, build
failures — and that is valuable. But it is a different kernel, a different
WebKitGTK build, a different desktop environment and a different set of loaded
sensor modules. Anything touching `/sys`, `hwmon`, GNOME or Wayland behaviour
must be validated on real Fedora.

Likewise, `windows-latest` proves the code compiles and the unit tests pass on
the real MSVC toolchain. It does not prove the window looks right, the overlay
positions correctly, or a sensor reads a plausible value.

## Manual validation

Required for anything system-facing, on **both** platforms:

> A system-facing feature is not considered complete until its behavior on both
> Windows and Fedora Linux has been designed and, whenever materially testable,
> validated.

### Phase 0 checklist

On each platform:

1. `pnpm app:dev` starts PULSE and a window opens.
2. The dark theme renders correctly; the layout is not broken.
3. Overview, Gaming, Development, Personal and Mini are all reachable.
4. Navigation does not reload the app (no white flash, no re-mount).
5. The OS, architecture and version reported by the backend are correct.
6. On Fedora: the display server is reported correctly (`Wayland` or `X11`).
7. The application closes normally and restarts cleanly.
8. The terminal shows no errors or warnings.

When a platform cannot be tested physically, say so explicitly. CI is a
provisional signal, never a claim of validation.
