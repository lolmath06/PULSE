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

| Capability              | Windows              | X11 / XWayland                                         | Wayland (native)                                                                    |
| ----------------------- | -------------------- | ------------------------------------------------------ | ----------------------------------------------------------------------------------- |
| Always on top           | supported (expected) | supported — **measured on XWayland**                   | **not guaranteed** — no protocol; **measured on GNOME: goes behind a focused app**  |
| Click-through           | supported (expected) | supported — **measured on XWayland**                   | **measured working on GNOME** (reported _limited_: compositor decides)              |
| Absolute positioning    | supported (expected) | supported                                              | **unsupported** — clients cannot place windows                                      |
| Transparent window      | supported (expected) | X11: limited (needs a compositor); XWayland: supported | supported                                                                           |
| Global shortcut         | supported (expected) | X11: supported; XWayland: **limited**                  | XDG Desktop Portal `GlobalShortcuts` if the desktop offers it, else **unsupported** |
| Multi-monitor placement | supported (expected) | supported                                              | **unsupported** — the compositor chooses                                            |
| Tray                    | supported (expected) | limited — GNOME needs the AppIndicator extension       | limited — same                                                                      |

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

## Two separate things on GNOME Wayland

**Always on top — not available to an ordinary application window.**
Physically confirmed on Fedora 39 / GNOME 45: once another application is
focused, it is drawn above the overlay. Wayland has no protocol for a client
to keep itself above others and PULSE does not work around that — no GNOME
Shell injection, no extension, no compositor settings, no forcing PULSE
through XWayland. GNOME's own window menu (Alt+Space → _Always on Top_) is
the user's choice. A GNOME-specific companion may be considered in a later
platform phase. PULSE reports it _limited_.

**Global shortcut — through the XDG Desktop Portal.** The Tauri plugin grabs
keys through X11; on native Wayland that grab lands on XWayland, which sees
keys only while an X11 window is focused, so it _registered_ and never fired
with Firefox focused. PULSE now picks a backend per session:

| Session        | Backend                                                                    |
| -------------- | -------------------------------------------------------------------------- |
| Windows        | plugin (`RegisterHotKey`) — unchanged                                      |
| X11 / XWayland | plugin (`XGrabKey`) — unchanged                                            |
| Wayland        | `org.freedesktop.portal.GlobalShortcuts` (action `toggle-overlays-edit`)   |
| Wayland, none  | **unsupported**, with the reason — never a silent fallback to the X11 grab |

Portal lifecycle: `CreateSession` → `BindShortcuts` (the desktop may show a
dialog to approve or choose the keys; declining leaves the shortcut
**not bound**, reported as such) → `Activated` toggles Edit/Locked once per
press, `Deactivated` is ignored, `ShortcutsChanged` updates the keys shown →
`Session.Close` on disable and on quit. Changing the shortcut binds a new
session and closes the old one only once the desktop accepted it; at most one
session stays open. A declined shortcut is retried only when you press
_Apply_ again, never on its own.

Availability: GNOME offers the portal from **GNOME 48**, KDE Plasma from
5.27. **Fedora 39 (GNOME 45, xdg-desktop-portal-gnome 45.1) does not** — the
interface is absent from the session bus — so there PULSE reports the global
shortcut **unsupported** with that reason. Use the tray, an overlay's bar or
PULSE → Overlays instead.

## What the manual tests mean

- **Click-through — success:** with a locked overlay over another application
  (e.g. Firefox), a click lands in that application, which becomes focused;
  PULSE does **not** gain focus; the overlay remains drawn. The application
  behind taking focus is the intended behaviour and must not be "fixed".
  **Failure:** the overlay swallows the click or takes focus. The overlay
  going behind the application or disappearing is an **always-on-top**
  limitation, reported separately.
- **Global shortcut — success:** with another application focused, the
  shortcut toggles every overlay between Edit and Locked. On native Wayland
  PULSE → Overlays says which backend is in force: _XDG Desktop Portal_
  (expect the desktop to ask for approval the first time) or _not available on
  this session_ with the reason.

## Fullscreen games

| Game mode             | Desktop overlay                                                         |
| --------------------- | ----------------------------------------------------------------------- |
| Windowed              | works wherever always-on-top works                                      |
| Borderless fullscreen | generally works (it is a normal window); depends on the compositor/OS   |
| Exclusive fullscreen  | a normal window is usually **hidden**; PULSE does **not** claim support |

PULSE does not inject into games, hook DirectX/OpenGL/Vulkan, or draw inside
the game's swap chain. Exclusive fullscreen is out of scope by design.
