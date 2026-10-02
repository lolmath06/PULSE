# Windows native CI and artifacts

## What runs natively on Windows

On `windows-latest`, with MSVC:

```text
pnpm install --frozen-lockfile
pnpm typecheck · pnpm lint · pnpm format:check · pnpm test · pnpm build
cargo check  --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo test   --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo +1.77.2 check --locked --manifest-path src-tauri/Cargo.toml --all-targets
pnpm tauri build --bundles nsis msi -- --locked
```

This compiles what Fedora never can:

- the whole Tauri application for Windows (WebView2, tao, the tray, the
  global-shortcut plugin);
- `src-tauri/src/overlay_native.rs`'s `cfg(target_os = "windows")` glue —
  `WebviewWindow::hwnd()`, the UI-thread dispatch — and the Win32 overlay
  backend in `platform/windows/overlay_window.rs` (`SetWindowSubclass`,
  `WM_STYLECHANGING`, `SetWindowLongPtrW`, `SetWindowPos(HWND_TOPMOST)`);
- every Windows metric provider, the process collector and inspector, with
  their real `windows`/`windows-sys` bindings;
- the bundled SQLite C amalgamation, with MSVC;
- the NSIS and MSI installers.

The native test run includes the overlay style tests (`overlay_window::tests`:
locked bits, edit bits, Lock → Edit → Lock across tao rewrites, the subclass
state) and every other Rust test. None of them touches a desktop.

## The artifact

Each _Windows package_ run uploads one artifact, kept 30 days:

**`PULSE-windows-x64-<short-sha>`** (the first 7 characters of the commit)

| File                                   | What                                                                                                                |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `PULSE-windows-x64-<sha>-setup.exe`    | NSIS installer — installs for the current user, no administrator prompt, uninstaller registered in _Installed apps_ |
| `PULSE-windows-x64-<sha>.msi`          | MSI installer (Windows Installer)                                                                                   |
| `PULSE-windows-x64-<sha>-portable.exe` | the raw application, no installer; needs the WebView2 runtime (present on Windows 11 and updated Windows 10)        |
| `SHA256SUMS.txt`                       | `<sha256>  <file>` for the three binaries                                                                           |
| `build-info.txt`                       | version, commit, ref, run id/attempt, runner image, Rust/Node/pnpm, target, bundles, UTC time                       |

The job **fails** if any of the three binaries is missing or empty — it
never uploads an empty or partial set.

### MSI version

MSI requires a numeric `major.minor.patch` version. For the stable `1.0.0`
release, the application version and `bundle.windows.wix.version` are both
`1.0.0`. The WiX upgrade code remains pinned so later MSI releases upgrade
PULSE in place.

## Downloading and verifying

1. GitHub → **Actions** → the _CI_ run for the commit → **Artifacts** →
   `PULSE-windows-x64-<sha>` (a zip; signed-in GitHub account required).
2. Unzip, then in PowerShell, in that folder:

   ```powershell
   Get-FileHash -Algorithm SHA256 .\PULSE-windows-x64-*-setup.exe
   Get-Content .\SHA256SUMS.txt
   ```

   The hashes must match. (`certutil -hashfile <file> SHA256` works too.)

3. Record the short SHA and the setup hash in the physical validation report.

## Unsigned builds

The binaries are **not code-signed**. Windows SmartScreen may show _Windows
protected your PC_; _More info → Run anyway_ proceeds. This is expected for
an unsigned build; no certificate or publisher identity is claimed.

## Which file to test

For physical validation, use the **`-setup.exe`** of the commit whose CI is
fully green (all four checks), and verify its SHA-256 first. The `.msi` and
the portable `.exe` are secondary checks.

## Final Phase 13 artifact

The final Phase 13 CI run used for closure was GitHub Actions run `#10`
(run id `36984860809`) on canonical commit
`a7014c4c8d60995b4388ed63bc098e80521e19c2`.

All four jobs passed:

- Linux quality;
- Windows native quality;
- MSRV 1.77.2 (Linux);
- Windows package.

The resulting artifact was `PULSE-windows-x64-a7014c4`.

Its portable executable, NSIS installer and MSI were subsequently exercised on
a physical Windows machine. All three SHA-256 values matched
`SHA256SUMS.txt`; portable launch, NSIS install/application launch/uninstall,
and MSI install/uninstall passed.

See
[`windows-physical-validation.md`](windows-physical-validation.md) for the
physical validation record.
