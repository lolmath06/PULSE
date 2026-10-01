# Release process

## Today (Phase 13A)

- Every push to `main` or a `phase*` branch, and every pull request, runs CI;
  a green _Windows package_ job leaves a downloadable Windows artifact
  ([`windows-ci.md`](windows-ci.md)).
- `release.yml` is **prepared but unused**. Nothing is published by pushing a
  branch.
- The version is `0.1.0-dev`. **No `v1.0.0`**: Windows has not been validated
  on physical hardware.

## Releasing later

1. Windows physical validation passes
   ([`windows-physical-validation.md`](windows-physical-validation.md)) on the
   artifact of the commit to release; Fedora validation is current.
2. Set the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and
   `package.json` (they must agree — `release.yml` checks), and
   `bundle.windows.wix.version` (numeric, e.g. `1.0.0`).
3. Tag `v<version>` on that commit and push the tag.
4. `release.yml` checks the tag against the version, builds the Windows
   bundles (the same reusable job as CI) and the Linux bundles (`.deb`,
   `.rpm`, AppImage), combines `SHA256SUMS.txt`, and creates a **draft
   pre-release** with everything attached.
5. Review the draft — assets, sums, notes — and publish it by hand.

A manual run of _Release_ (workflow_dispatch) is a dry run: it builds and
uploads the bundles as workflow artifacts and creates no release.

## Signing

Not yet. Windows binaries are unsigned (SmartScreen may warn); Linux packages
are unsigned. Signing needs a real certificate and identity, which will be a
deliberate later decision — never improvised.
