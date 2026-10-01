# Continuous integration

Three kinds of Windows evidence exist. They are **not** interchangeable:

| Level | What                            | Where                                | Proves                                                                                                                                      | Does not prove                      |
| ----- | ------------------------------- | ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------- |
| 1     | Cross-target harness            | Fedora / Linux (`pnpm rust:windows`) | PULSE's tauri-free Windows code type-checks for `x86_64-pc-windows-msvc`                                                                    | that anything links, builds or runs |
| 2     | **Native Windows CI**           | GitHub `windows-latest`              | the real crate — Tauri, the Win32 overlay glue, bundled SQLite — compiles with MSVC, its tests and clippy pass, and installers are produced | that PULSE works on a real desktop  |
| 3     | **Physical Windows validation** | a person on a real Windows machine   | overlays, click-through, hotkey, DPI, installers, metrics actually work                                                                     | —                                   |

CI is never described as level 3. The level-3 checklist is
[`windows-physical-validation.md`](windows-physical-validation.md).

## Workflows

| File                                    | Runs on                                           | Purpose                                                   |
| --------------------------------------- | ------------------------------------------------- | --------------------------------------------------------- |
| `.github/workflows/ci.yml`              | push to `main` or `phase*`, pull requests, manual | quality gates on Linux and native Windows, then packages  |
| `.github/workflows/windows-package.yml` | called by `ci.yml` and `release.yml`              | the Tauri Windows build, artifact verification and upload |
| `.github/workflows/release.yml`         | a pushed `v*` tag, or manual (dry run)            | Windows + Linux bundles; a **draft** pre-release for tags |

## CI jobs (stable check names)

| Check                      | Runner         | Steps                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| -------------------------- | -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Linux quality**          | ubuntu-latest  | commit attribution (`scripts/check-git-attribution.sh`: no Claude, Anthropic, Codex, OpenAI or ChatGPT author, committer or attribution trailer) · case-insensitive path and module-name collisions (`scripts/check-case-collisions.sh`: Windows cannot hold `Foo.ts` beside `foo.ts`, nor resolve `@/x/Foo` correctly beside `foo.ts` and `Foo.tsx`) · frontend format, lint, typecheck, tests, build · Rust fmt, clippy `--locked -D warnings`, tests `--locked` · Windows cross harness + its clippy · Linux app build (`tauri build --no-bundle -- --locked`) |
| **Windows native quality** | windows-latest | frontend typecheck, lint, format, tests, build · `cargo check --locked --all-targets` · `cargo test --locked` · `cargo clippy --locked --all-targets -D warnings` · `cargo +1.77.2 check --locked --all-targets`                                                                                                                                                                                                                                                                                                                       |
| **MSRV 1.77.2 (Linux)**    | ubuntu-latest  | `cargo check --locked --all-targets` and `cargo test --locked` with Rust 1.77.2                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| **Windows package**        | windows-latest | runs after _Windows native quality_; `pnpm tauri build --bundles nsis msi -- --locked`, verify, checksum, upload                                                                                                                                                                                                                                                                                                                                                                                                                       |

Every command is its own named step, so a failure names itself. These names
are the ones to make required checks on `main`.

`ubuntu-latest` catches Linux regressions but is not Fedora (different
kernel, WebKitGTK, desktop, sensors); Fedora remains validated by hand.

## Toolchains and reproducibility

- Node from `.nvmrc` (22); pnpm from `package.json` → `packageManager`
  (via `pnpm/action-setup`); Rust stable for the quality gates plus 1.77.2 for
  the MSRV promise (no `rust-toolchain` file is used).
- `pnpm install --frozen-lockfile` and `cargo … --locked` everywhere: CI never
  resolves a dependency.
- Caches: `actions/setup-node`'s pnpm store cache and `Swatinem/rust-cache`
  (keyed by lockfiles and toolchain, per job).
- `.gitattributes` forces LF, so Prettier's check behaves the same on Windows.

## Security

- `permissions: contents: read` for CI and for packaging. Pull requests get no
  write access.
- Only `release.yml`'s _Draft GitHub Release_ job has `contents: write`, only
  for a pushed tag, and it only creates a **draft**.
- Only established actions: `actions/checkout`, `actions/setup-node`,
  `actions/upload-artifact`, `actions/download-artifact`,
  `pnpm/action-setup`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`. No
  `curl | bash`. No secrets are used or printed; release uses the built-in
  `github.token` through `gh` (preinstalled on runners).

## No startup smoke test (yet)

A bounded "launch `pulse.exe`, check it is alive after a few seconds, stop
that PID" step was evaluated and **not added**:

- whether a hosted runner session reliably provides what WebView2 needs to
  open a window has not been observed for PULSE — a false failure there would
  be noise, not evidence;
- PULSE resolves `%APPDATA%` through the Windows known-folder API, so a run
  cannot be pointed at a scratch configuration.

It can be revisited once the first runs show how the runner behaves. It would
never be level-3 evidence either way.
