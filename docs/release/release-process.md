# Release process

## Current state — Phase 13C

PULSE's planned development and cross-platform validation scope is complete.

- Every push to `main` or a `phase*` branch, and every pull request, runs CI.
- Native Linux quality, native Windows quality, the Rust 1.77.2 MSRV gate and
  Windows packaging are green on the final Phase 13 commit.
- Windows physical runtime validation is complete.
- The final Windows artifact for
  `a7014c4c8d60995b4388ed63bc098e80521e19c2` has been physically smoke-tested:
  portable, NSIS installation/application launch/uninstall, MSI
  install/uninstall and SHA-256 verification all passed.
- The version intentionally remains `0.1.0-dev`.
- There is no release tag.
- There is no published GitHub Release.
- `release.yml` is prepared but has not been used to publish a release.

The absence of a versioned public release is therefore a **publishing
decision**, not an unfinished Windows-validation task.

See
[`windows-physical-validation.md`](windows-physical-validation.md) for the
physical evidence and [`windows-ci.md`](windows-ci.md) for the automated
Windows pipeline.

## Publishing a version later

When a public version is deliberately chosen:

1. Choose the release version.
2. Set that version in `src-tauri/tauri.conf.json`,
   `src-tauri/Cargo.toml` and `package.json`; they must agree.
3. Set `bundle.windows.wix.version` to the corresponding numeric
   `major.minor.patch` value.
4. Review whether code signing is desired for that release.
5. Create and push `v<version>` only after explicitly approving the release.
6. `release.yml` verifies the tag/version pair, builds Windows and Linux
   bundles, combines `SHA256SUMS.txt`, and creates a **draft pre-release**.
7. Review the draft assets, checksums and notes, then publish it manually if
   desired.

A manual `workflow_dispatch` run of _Release_ remains a dry run: it builds
artifacts but creates no public release.

## Signing

Windows and Linux packages are currently unsigned.

Windows SmartScreen may therefore warn about the binaries. Signing requires a
real certificate and publisher identity and remains a deliberate future
publishing decision rather than a Phase 13 validation requirement.
