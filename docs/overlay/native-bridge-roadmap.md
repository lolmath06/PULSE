# Native overlay bridges — roadmap

**Phase 12 update:** the GNOME bridge is in production
([`gnome-bridge.md`](gnome-bridge.md), physically verified) and the Windows
native backend is implemented ([`windows-native.md`](windows-native.md),
compiled, **not physically verified**). The rest of this page is the
original plan, kept for the record; the St-actor fallback was never needed.

## Linux — GNOME Wayland

1. **Proof** (11.5A): Shell extension → `make_above` + Mutter key binding →
   PULSE over session D-Bus. Gate: Matheo's two physical tests.
2. **If the proof passes — production bridge:** PULSE detects the extension
   (bus handshake / version) and reports always-on-top and the shortcut
   honestly from it; the shortcut is configured from PULSE; packaging and
   update of the extension; GNOME 46+ compatibility checked on real versions.
3. **If `make_above` fails the Z-order test — fallback, needs approval:** a
   GNOME Shell–native HUD drawn with `St` actors in the Shell's own scene
   graph (not an application window), fed by PULSE over D-Bus. First proof
   would show a single static test value.

Other Wayland compositors (KDE: KWin scripts / `GlobalShortcuts` portal;
wlroots: `wlr-layer-shell`) are not started.

## Windows — NOT IMPLEMENTED IN 11.5A · NOT PHYSICALLY TESTED

Nothing here has been written or run on Windows.

**Desktop / borderless games — native HWND overlay:**

- a dedicated Win32 window for the overlay, owned by PULSE;
- `HWND_TOPMOST` via `SetWindowPos`, with `SWP_NOACTIVATE` so showing or
  restacking never takes focus;
- `WS_EX_LAYERED` for per-pixel transparency;
- click-through when locked: `WS_EX_TRANSPARENT` (plus `WS_EX_NOACTIVATE`);
- `WS_EX_TOOLWINDOW` so it stays out of the taskbar and Alt+Tab;
- global shortcut with `RegisterHotKey` (the existing plugin already does
  this).

**Games — possible specialised path, to evaluate only:**

- an **Xbox Game Bar widget** as an optional "Gaming" backend, which the OS
  itself draws over fullscreen games;
- no DLL injection, no DirectX / Vulkan / OpenGL hooking, nothing that
  anti-cheat software could treat as tampering.

No Windows runtime claim is made anywhere.
