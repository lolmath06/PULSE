# pulse-overlay@jamby — PULSE overlay bridge (prototype)

**Prototype for PULSE Phase 11.5A — NOT PHYSICALLY PROVEN.**

A GNOME Shell 45 extension that, on the compositor side:

- keeps PULSE's overlay windows above other windows (`Meta.Window.make_above`),
  event-driven, no polling;
- registers Ctrl+Shift+F12 with Mutter and forwards it to PULSE over the
  session bus (`dev.pulse.app`, `ToggleOverlayEditMode`).

This directory is the source of truth; `../install.sh` copies it to
`~/.local/share/gnome-shell/extensions/pulse-overlay@jamby/`. Design,
safety boundaries and the physical proof procedure:
`docs/overlay/gnome-bridge-poc.md`.

- `extension.js` — Shell glue only.
- `lib.js` — every decision (window matching, lifecycle); unit-tested in
  `../tests/bridge.test.js` without GNOME Shell.
- `schemas/` — the private key `toggle-overlays` (`<Control><Shift>F12`).
