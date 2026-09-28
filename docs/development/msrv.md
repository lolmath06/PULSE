# Minimum Supported Rust Version

PULSE's minimum Rust version is **1.77.2** (`rust-version` in
`src-tauri/Cargo.toml` and `tools/windows-check/Cargo.toml`).

**It is a promise about the whole locked build**, not about PULSE's own source
alone: `cargo build --locked` with Rust 1.77.2 must succeed, dependencies
included.

## How it is verified

With the real historical toolchain, installed through rustup (no sudo, no
system package):

```bash
rustup toolchain install 1.77.2 --profile minimal --component clippy
rustup target add x86_64-pc-windows-msvc --toolchain 1.77.2

cargo +1.77.2 check --manifest-path src-tauri/Cargo.toml
cargo +1.77.2 check --manifest-path src-tauri/Cargo.toml --all-targets
cargo +1.77.2 build --manifest-path src-tauri/Cargo.toml     # compiles bundled SQLite
cargo +1.77.2 test  --manifest-path src-tauri/Cargo.toml     # history tests included
cargo +1.77.2 check --manifest-path tools/windows-check/Cargo.toml --target x86_64-pc-windows-msvc
```

CI's `msrv` job runs the build and the tests with `dtolnay/rust-toolchain@1.77.2`
and `--locked` on `ubuntu-latest` and `windows-latest`.

A dependency's `rust-version` is **not** trusted on its own: many crates do
not declare one (rusqlite, libsqlite3-sys, dlopen2), and some declare one their
code does not honour. Only a build with 1.77.2 is evidence.

### Last verification (Phase 10 corrective, Fedora 39)

| Command (Rust 1.77.2 `25ef9e3d8 2024-04-09`)                                             | Result                 |
| ---------------------------------------------------------------------------------------- | ---------------------- |
| `check` (src-tauri)                                                                      | pass                   |
| `check --all-targets` (src-tauri)                                                        | pass                   |
| `build` (src-tauri) — bundled SQLite 3.46.0 compiled from `sqlite3.c`                    | pass                   |
| `test` (src-tauri) — 1563 passed, 1 ignored, incl. 60 history tests on real SQLite files | pass                   |
| `check` and `check --all-targets`, windows-check, `x86_64-pc-windows-msvc`               | pass                   |
| `clippy --all-targets -D warnings` with clippy **0.1.77**                                | 16 findings, see below |

**Clippy 0.1.77 is informational, not a gate.** It reports 16 findings in
code from earlier phases (`needless_borrows_for_generic_args` on
`serde_json::to_value(&x)`, `format_collect`) that the project's gating clippy
(current stable) accepts. The one finding in Phase 10 code was a false positive:
removing the `let` it flagged fails to compile (E0597), so it carries a
targeted `#[allow]`. The gate stays `cargo clippy` on stable, as in CI.

## Keeping the lockfile compatible

Re-resolve with Cargo's MSRV-aware fallback, then verify with 1.77.2:

```bash
CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo update --manifest-path src-tauri/Cargo.toml
cargo +1.77.2 build --manifest-path src-tauri/Cargo.toml
```

Pins that the resolver cannot find by itself, because the crates declare no
`rust-version`:

| Crate                          | Pinned          | Why                                                                                                     |
| ------------------------------ | --------------- | ------------------------------------------------------------------------------------------------------- |
| `rusqlite`                     | `0.32` (0.32.1) | libsqlite3-sys 0.31+ uses `#[expect]` (1.81), 0.38 `cfg_select!`                                        |
| `windows-version`              | `0.1` (0.1.7)   | 0.100 is edition 2024 and declares Rust 1.95                                                            |
| `dlopen2` / `dlopen2_derive`   | 0.8.0 / 0.4.0   | 0.8.1+ / 0.4.2+ are edition 2024 (via `tao`, Linux)                                                     |
| `tauri-plugin-global-shortcut` | `~2.3` (2.3.2)  | 2.4 declares Rust 1.90 (Phase 11)                                                                       |
| `getrandom` / `wasi`           | 0.3.3 / 0.14.2  | newer ones pull edition-2024 `wit-bindgen`; wasm-only, pinned so no locked manifest needs a newer Cargo |

The last two are lockfile pins (`cargo update --precise`), so a careless
`cargo update` undoes them — the 1.77.2 build or the CI `msrv` job will say so.
