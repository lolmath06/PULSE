# Overlay — User guide

## Create an overlay

- **PULSE → Overlays → New overlay**: _Empty_, or a preset — _Tiny stats_
  (CPU | GPU | RAM), _Thermal strip_ (CPU and GPU temperatures), _Gaming_
  (CPU trend, CPU temperature, GPU group, RAM, download), _Minimal corner_
  (CPU and RAM, no background). Everything is editable afterwards.
- **From a dashboard widget**: _Edit layout_ → hover a widget → _To overlay_ →
  _New overlay_ or _Add to “…”_. The copy keeps the metric, renderer and style.

## Edit and lock

- An overlay starts in **Edit** mode: a thin bar laid over the top — drag it
  to move the window — with _Lock_ and _Open PULSE_, and a grip in the
  bottom-right corner to resize. The bar never changes the overlay's size.
- **Lock** hides every control. Where the system allows, clicks then go
  through the overlay to the application behind it, and the overlay can never
  take the keyboard focus.

**How to test click-through:** lock the overlay over a Firefox window and
click on it. **Success:** Firefox receives the click _and_ the focus, PULSE
does not become focused, and the overlay stays drawn where it was. The
application behind becoming active is the expected result, not a bug.
**Failure:** the click is swallowed by the overlay, PULSE takes the focus, or
the overlay goes behind Firefox / disappears. Whether it stays visually above
is the separate _always on top_ capability (on GNOME Wayland: Alt+Space →
_Always on Top_ on the overlay).

- To edit again: PULSE → Overlays → _Edit overlay_ / _Edit all_, the tray's
  _Edit overlays_, or the global shortcut.

## Customize

In PULSE → Overlays, each overlay has: name, visible, Edit/Lock, layout (row,
column, grid), background (on/off, colour, opacity — the text stays opaque),
border, shadow, corners, gap, size (_Fit to widgets_), and position where the
system allows it.

**Layout is exact.** Every widget keeps its own pixel size and gets its own
box: a _row_ places them side by side, a _column_ stacks them, a _grid_ uses
columns as wide as their widest widget and rows as tall as their tallest.
The padding surrounds everything. _Fit to widgets_ sets the window to exactly
that size, and the preview in PULSE → Overlays draws the same composition,
scaled down uniformly when it does not fit the page. Each widget has a pixel size, _Customize_ (the full Phase 10
panel: renderer, colours, line, fill, background, fonts, history range…),
reorder, _Copy to dashboard_ and remove.

## Global shortcut

Default **Ctrl+Shift+F12**: locks every overlay if one is editable, unlocks
them all otherwise. Change it in PULSE → Overlays → _Global shortcut_; if the
combination is taken, PULSE says so and keeps the previous one. _Disable_
removes it.

On a **Wayland** session the shortcut goes through your desktop's portal
(XDG Desktop Portal `GlobalShortcuts`): the first time, the desktop may ask
you to approve it or to choose the keys, and it may let you change them later
in its own settings — PULSE then shows the keys the desktop reports. If you
decline, the shortcut is **not bound** and PULSE says so; press _Apply_ to be
asked again. If your desktop has no such portal (GNOME before 48 — Fedora 39
included), PULSE says global shortcuts are **not available** on the session:
use the tray, an overlay's bar or PULSE → Overlays.

**How to test it:** focus _another_ application (e.g. Firefox), press the
shortcut, and watch the overlay switch between Edit and Locked.

**Always on top is a different matter.** On GNOME Wayland an ordinary window
cannot keep itself above a focused application, and PULSE does not pretend
to; click-through still works.

## Closing PULSE

- Default: closing the main window **quits PULSE** and closes the overlays.
- _Keep running while overlays are visible_: while at least one overlay is
  visible, closing the main window only **hides** it. The overlays, the live
  feed and the history recorder keep running. _Open PULSE_ (an overlay in
  Edit mode, or the tray) shows the **same** main window again — never a
  second one. This does not depend on the tray: GNOME may hide tray icons.
  Closing the last overlay brings the main window back. With no visible
  overlay, closing the main window quits.
- **Quit PULSE** (tray or the command) always quits: overlays close, the live
  feed and history stop, the configuration is flushed and the database
  checkpointed, and no PULSE process is left.

## What your system allows

PULSE → Overlays → _What overlays can do here_ lists every capability with its
reason. On GNOME Wayland in particular: an application cannot keep itself
above others (use Alt+Space → _Always on Top_ on the overlay), cannot choose
its position (drag it in Edit mode), and click-through depends on the
compositor. See [`platform-capabilities.md`](platform-capabilities.md).
Measured on GNOME 45: a focused application is drawn over the overlay.

## Games

Windowed and borderless-fullscreen games usually let an always-on-top overlay
show. Exclusive fullscreen usually hides it — PULSE does not inject into games
and does not claim otherwise.

### Optional gaming test (manual, by you)

1. Create a compact overlay: CPU %, CPU temperature, GPU %, GPU temperature,
   RAM (e.g. _Gaming_ preset, then adjust).
2. Lock it.
3. Launch Minecraft **windowed** or **borderless**.
4. Check: the overlay stays above the game, clicks reach the game, the overlay
   never takes the focus.
5. Do not expect exclusive fullscreen to work.
