# Release process

## Release status

Phase 13 development and cross-platform validation are complete.

The first stable publication target is **PULSE 1.0.0**.

Linux Release packages are built on Ubuntu 22.04, with GLIBC 2.35 as their
maximum runtime baseline. The AppImage deliberately uses the target system's
WebKitGTK/GTK/GLib stack instead of mixing host libraries into it. After
packaging, the Release workflow extracts the AppImage, rejects bundled shared
libraries and WebKit helper processes, and fails if any shipped ELF requires a
newer `GLIBC_*` symbol version.

A branch push never publishes PULSE. A version becomes a GitHub Release only
through an explicit `v<version>` tag, followed by human review of the generated
draft.

Windows physical validation is recorded in
[`windows-physical-validation.md`](windows-physical-validation.md).
Automated Windows build and packaging coverage is documented in
[`windows-ci.md`](windows-ci.md).

## Release candidate

Before creating a version tag:

1. The version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and
   `package.json` must agree.
2. `bundle.windows.wix.version` must contain the corresponding numeric
   `major.minor.patch`.
3. Normal CI must be green.
4. Run the _Release_ workflow manually with `workflow_dispatch`.
   This is a dry run: it builds Windows and Linux packages as workflow
   artifacts and does **not** create a GitHub Release.
5. Inspect the generated Windows and Linux artifacts and perform any desired
   release-candidate package smoke tests.

## Publishing

Once the release candidate is approved:

1. Create the annotated version tag `v<version>` on the approved commit.
2. Push that tag explicitly.
3. `release.yml` verifies the tag against the application version.
4. It builds:
   - Windows NSIS installer;
   - Windows MSI;
   - Windows portable executable;
   - Linux `.deb`;
   - Linux `.rpm`;
   - Linux AppImage.
5. It combines the package checksums.
6. It creates a **draft GitHub Release**.
7. Review the draft, assets, checksums and release notes.
8. Publish the draft manually.

The tag-triggered workflow does not bypass human publication review.

## Signing

Windows and Linux packages are currently unsigned.

Windows SmartScreen may therefore warn about the Windows binaries. Code signing
requires a real certificate and publisher identity and is not silently
improvised as part of the release process.
