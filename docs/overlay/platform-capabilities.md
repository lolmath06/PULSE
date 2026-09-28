# Overlay — Platform capabilities

`overlay::capabilities` reports each capability as **supported**, **limited**
or **unsupported**, with its reason, for the display server the overlay
windows actually run on. PULSE → Overlays shows it. Runtime facts override the
expectation (a shortcut that failed to register becomes _unsupported_ with
the error).

## Detection

GTK's own rule: the first entry of `GDK_BACKEND` if set, otherwise Wayland
when `WAYLAND_DISPLAY` exists, otherwise X11. An X11 backend inside a Wayland
session is **XWayland**.

## Expectations and measurements

| Capability              | Windows              | X11 / XWayland                                         | Wayland (native)                                                       |
| ----------------------- | -------------------- | ------------------------------------------------------ | ---------------------------------------------------------------------- |
| Always on top           | supported (expected) | supported — **measured on XWayland**                   | **limited** — no protocol; GNOME: Alt+Space → _Always on Top_          |
| Click-through           | supported (expected) | supported — **measured on XWayland**                   | **limited** — input region requested, compositor decides; not verified |
| Absolute positioning    | supported (expected) | supported                                              | **unsupported** — clients cannot place windows                         |
| Transparent window      | supported (expected) | X11: limited (needs a compositor); XWayland: supported | supported                                                              |
| Global shortcut         | supported (expected) | X11: supported; XWayland: **limited**                  | **limited** — XWayland grab, fires only while an X11 window has focus  |
| Multi-monitor placement | supported (expected) | supported                                              | **unsupported** — the compositor chooses                               |
| Tray                    | supported (expected) | limited — GNOME needs the AppIndicator extension       | limited — same                                                         |

"Expected" means: what the platform API provides; **not yet verified on a
Windows machine**.

## Fedora 39 — measured (Phase 11)

Session: GNOME on Wayland (`XDG_SESSION_TYPE=wayland`), NVIDIA GPU on nouveau.

**Native Wayland** (`pnpm app:dev`, the default):

- PULSE reported: always-on-top _limited_, click-through _limited_,
  positioning _unsupported_, hotkey _limited_ (Ctrl+Shift+F12 registered, no
  conflict), tray _limited_ (created without error).
- Both pre-configured overlays were created as separate windows (three
  WebKit web processes: main + two overlays).
- Resizing recorded the size and left the stored position untouched.
- A synthetic Ctrl+Shift+F12 (XTEST) did **not** toggle the overlays —
  consistent with _limited_: the XWayland grab does not see keys while a
  Wayland window has focus. Always-on-top, click-through and the tray could
  not be verified physically: synthetic input does not reach Wayland windows
  from this session. **These need the manual protocol.**

**XWayland** (`GDK_BACKEND=x11`, plus `WEBKIT_DISABLE_COMPOSITING_MODE=1`
because WebKit crashes on nouveau under X11 otherwise):

- PULSE reported always-on-top, click-through and positioning _supported_,
  hotkey _limited_.
- Inspected with a read-only X11 tool: both overlays had
  `_NET_WM_STATE_ABOVE`, `_SKIP_TASKBAR`, `_SKIP_PAGER`, `_STICKY`. The locked
  overlay's input shape was **1×1 pixel** (click-through), the Edit overlay's
  its full size. The locked overlay was not focused. (The Edit overlay was
  focused by Mutter when it became focusable — acceptable while editing.)
- Screenshots showed live values and a live sparkline. With compositing
  disabled, glyphs of the previous frame could remain behind the new ones in
  transparent areas; this is tied to that forced WebKit mode and was not seen
  in opaque areas. To confirm on native Wayland.

## Fullscreen games

| Game mode             | Desktop overlay                                                         |
| --------------------- | ----------------------------------------------------------------------- |
| Windowed              | works wherever always-on-top works                                      |
| Borderless fullscreen | generally works (it is a normal window); depends on the compositor/OS   |
| Exclusive fullscreen  | a normal window is usually **hidden**; PULSE does **not** claim support |

PULSE does not inject into games, hook DirectX/OpenGL/Vulkan, or draw inside
the game's swap chain. Exclusive fullscreen is out of scope by design.
