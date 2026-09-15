## What this changes

<!-- A short description of the change and why it is needed. -->

## Type of change

- [ ] Bug fix
- [ ] New feature
- [ ] Refactor
- [ ] Documentation
- [ ] Build / CI / tooling

## Platform validation

> PULSE targets Windows and Fedora Linux as two first-class platforms.
> **Never tick a box for a platform you did not actually test.** "Compiles in
> CI, not physically tested" is an honest and acceptable answer.

|                  | Tested physically | Verified by CI only | Not applicable |
| ---------------- | ----------------- | ------------------- | -------------- |
| **Fedora Linux** | [ ]               | [ ]                 | [ ]            |
| **Windows**      | [ ]               | [ ]                 | [ ]            |

Notes on what was tested, and on anything left unverified:

<!-- e.g. "Fedora 39 Wayland: window opens, navigation fine. Windows: CI only, no machine available." -->

## Architecture checklist

- [ ] The frontend does not access the system directly (`/proc`, `/sys`, WMI, PDH, NVML…)
- [ ] OS branching (`#[cfg(target_os = ...)]`) stays inside `src-tauri/src/platform/`
- [ ] Platform-only crates are declared under `[target.'cfg(target_os = "...")'.dependencies]`
- [ ] `invoke` is only called from `src/services/`
- [ ] Rust payload types are mirrored in `src/types/` (camelCase)

## Checks

- [ ] `pnpm check:all` passes locally
- [ ] Tests added or updated for testable logic
- [ ] Documentation updated in this PR
- [ ] `CHANGELOG.md` updated under `[Unreleased]` (if user-visible)

## Related issues

<!-- Closes #123 -->
