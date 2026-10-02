# Windows physical validation — Phase 13B

## Status

**PASS — 2026-10-02**

PULSE has been physically exercised on Windows and the final Windows package
for the canonical Phase 13 commit has been smoke-tested on real hardware.

This record distinguishes physical evidence from CI evidence and does not claim
that every optional item from the original exhaustive checklist was exercised.

## Canonical build

- Canonical commit:
  `a7014c4c8d60995b4388ed63bc098e80521e19c2`
- Branch: `phase13-ci-release`
- Version: `0.1.0-dev`
- GitHub Actions CI run: `#10`
- Workflow run id: `36984860809`
- Artifact: `PULSE-windows-x64-a7014c4`

CI completed successfully for:

- Linux quality;
- Windows native quality;
- Rust 1.77.2 MSRV on Linux;
- Windows package.

## Final package validation

The artifact downloaded from CI contained:

- `PULSE-windows-x64-a7014c4-setup.exe`;
- `PULSE-windows-x64-a7014c4.msi`;
- `PULSE-windows-x64-a7014c4-portable.exe`;
- `SHA256SUMS.txt`;
- `build-info.txt`.

`build-info.txt` identified the exact canonical commit above.

On a physical Windows machine:

- the SHA-256 of all three binaries matched `SHA256SUMS.txt`;
- the portable executable launched and rendered the PULSE interface normally;
- the NSIS installer completed successfully;
- PULSE appeared in Windows Installed Apps;
- the installed application launched successfully;
- the NSIS uninstaller completed successfully;
- after uninstall, PULSE was no longer registered under the current user's
  Installed Apps registry entries;
- the MSI installed successfully;
- the MSI uninstalled successfully.

SmartScreen and UAC behaviour were not recorded separately during this final
package smoke test, so this document makes no additional physical claim about
those prompts.

## Physical runtime validation

A preceding Phase 13B Windows session exercised the actual application runtime.

Verified on physical Windows hardware:

- the Overview displayed plausible live hardware values;
- application language switching worked, including French and Japanese;
- language changes propagated to Mini and overlay windows without visible raw
  translation keys;
- Tiny Stats rendered as a native overlay;
- a locked overlay stayed above an ordinary application;
- locked overlay input was click-through;
- Ctrl+Shift+F12 switched between Edit and Locked while another application
  was focused;
- Windows native debug output showed the expected managed styles:
  `TOPMOST | LAYERED | TRANSPARENT | NOACTIVATE | TOOLWINDOW` while locked and
  `TOPMOST | TOOLWINDOW` while interactive;
- the process inspector was exercised with a disposable Notepad process and the
  process-control path was used to terminate that disposable process;
- configuration/history persistence and relaunch behaviour were exercised;
- the `Keep running` close behaviour kept PULSE alive while overlays were
  visible;
- explicit Quit stopped PULSE.

## Not separately exercised

The physical validation above is deliberately narrower than the original
exhaustive checklist. The following were **not separately proven** during the
recorded Phase 13B sessions:

- 20 consecutive click-through clicks on Windows;
- every visual style and every dashboard/mode combination;
- every metric or hardware sensor on every possible Windows hardware class;
- 100 / 125 / 150 / 200 percent DPI matrix;
- multi-monitor persistence and monitor removal;
- borderless/windowed-game overlay behaviour;
- exclusive fullscreen, which is outside PULSE's claimed desktop-overlay
  support;
- the full process-inspector matrix for signatures, protected processes,
  affinity, priority, suspend/resume and forced PID reuse;
- every item from the earlier historical manual-test protocols.

Those remain useful compatibility tests for future hardware or release
qualification, but they are not prerequisites for closing the current
development phase.

## Result

Phase 13B Windows validation is complete for the project's current closure
scope:

- real Windows runtime: **PASS**;
- final portable executable: **PASS**;
- final NSIS install / launch / uninstall: **PASS**;
- final MSI install / uninstall: **PASS**;
- artifact SHA-256 verification: **PASS**;
- native Windows CI and packaging: **PASS**.

No further Windows testing is required for Phase 13C documentation closure.
