# pulse-overlay@jamby — PULSE Overlay Bridge

A GNOME Shell 45 extension that, on the compositor side:

- keeps PULSE's overlay windows above other windows (`Meta.Window.make_above`),
  event-driven, no polling;
- registers the overlay shortcut (`<Control><Shift>F12` by default) with Mutter
  and forwards it to PULSE over the session bus (`dev.pulse.app`,
  `ToggleOverlayEditMode`);
- greets PULSE once each time it starts (`Hello`), so PULSE can show that the
  running extension reaches it.

Physically verified on Fedora 39 / GNOME 45 (Wayland).

This directory is the source of truth; `../install.sh` copies it to
`~/.local/share/gnome-shell/extensions/pulse-overlay@jamby/`. Usage, the
D-Bus contract, safety boundaries and limits: `docs/overlay/gnome-bridge.md`.

- `extension.js` — Shell glue only.
- `lib.js` — every decision (window matching, lifecycle, hello); unit-tested
  in `../tests/bridge.test.js` without GNOME Shell.
- `schemas/` — the private key `toggle-overlays`. PULSE writes it when you
  change the shortcut in PULSE → Overlays while the bridge is active.
