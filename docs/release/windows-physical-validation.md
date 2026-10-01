# Windows physical validation (Phase 13B) — checklist

> **Every item: NOT EXECUTED.** This is the checklist for a real Windows
> machine, used only after CI is fully green. CI results never tick a box
> here, and nothing run on Fedora counts as a Windows test.

## Before starting

- Artifact: `PULSE-windows-x64-<sha>` from a green CI run — record the short
  SHA: `______`
- `Get-FileHash -Algorithm SHA256` of the `-setup.exe` matches
  `SHA256SUMS.txt` — record it: `______`
- Windows edition/build: `______` · display scaling: `____ %` · monitors: `__`
- GPU and driver: `______`

## Installation

- [ ] NOT EXECUTED — the setup `.exe` launches (SmartScreen warning expected: unsigned)
- [ ] NOT EXECUTED — installation succeeds without an administrator prompt
- [ ] NOT EXECUTED — PULSE starts from the Start menu
- [ ] NOT EXECUTED — PULSE appears in _Installed apps_ with an uninstaller
- [ ] NOT EXECUTED — (secondary) the `.msi` installs and uninstalls
- [ ] NOT EXECUTED — (secondary) the portable `.exe` starts without installing

## Main app

- [ ] NOT EXECUTED — Overview and dashboards render (WebView2)
- [ ] NOT EXECUTED — styles render (Clean, Glass, Neon at least)
- [ ] NOT EXECUTED — modes switch (Gaming, Development, Personal, Mini)
- [ ] NOT EXECUTED — settings persist across a restart (`%APPDATA%\dev.pulse.app\ui-config.json`)

## Metrics

- [ ] NOT EXECUTED — CPU total and per core
- [ ] NOT EXECUTED — memory
- [ ] NOT EXECUTED — advanced CPU (frequency, topology)
- [ ] NOT EXECUTED — GPU (DXGI adapters; NVML if an NVIDIA driver is installed)
- [ ] NOT EXECUTED — thermals (what Windows exposes; absent sensors explained, never 0)
- [ ] NOT EXECUTED — storage (disks, volumes, I/O)
- [ ] NOT EXECUTED — network (interfaces, traffic, Wi-Fi)
- [ ] NOT EXECUTED — processes list
- [ ] NOT EXECUTED — process inspector (path, signature, owner)

## Overlays

- [ ] NOT EXECUTED — create an overlay; move and resize it in Edit
- [ ] NOT EXECUTED — Edit: interactive (drag, resize, _Lock_, _Open PULSE_)
- [ ] NOT EXECUTED — Locked: no bar, no grip
- [ ] NOT EXECUTED — always on top of an ordinary focused application
- [ ] NOT EXECUTED — never steals focus (typing in the app below continues)
- [ ] NOT EXECUTED — click-through: 20 consecutive clicks through a locked overlay all reach the app below
- [ ] NOT EXECUTED — Ctrl+Shift+F12 toggles Edit/Locked with another app focused
- [ ] NOT EXECUTED — several overlays at once
- [ ] NOT EXECUTED — Top Bar and Bottom Bar span the screen at their edge
- [ ] NOT EXECUTED — Left/Right Rail span the height at their edge
- [ ] NOT EXECUTED — Tiny Stats legible

## Windows-specific

- [ ] NOT EXECUTED — no taskbar entry for an overlay
- [ ] NOT EXECUTED — no Alt+Tab entry for an overlay
- [ ] NOT EXECUTED — topmost over ordinary apps after they are focused
- [ ] NOT EXECUTED — topmost over a borderless/windowed game, if available
- [ ] NOT EXECUTED — locked overlay stays **visible** (layered window)
- [ ] NOT EXECUTED — DPI: 100 / 125 / 150 % keep size and position
- [ ] NOT EXECUTED — moving an overlay to another monitor, if available
- [ ] NOT EXECUTED — `PULSE_OVERLAY_INPUT_DEBUG=1`: locked shows `TOPMOST | LAYERED | TRANSPARENT | NOACTIVATE | TOOLWINDOW`

## Lifecycle

- [ ] NOT EXECUTED — _Keep running_: closing the main window keeps overlays
- [ ] NOT EXECUTED — _Open PULSE_ from an overlay brings back the same window
- [ ] NOT EXECUTED — Quit from the tray stops everything
- [ ] NOT EXECUTED — restart restores dashboards, overlays, style
- [ ] NOT EXECUTED — no `pulse.exe` or WebView2 child left after Quit

## Result

PASS only if every mandatory item passes on the recorded artifact. Note any
failure with the item, what happened, and a screenshot.
