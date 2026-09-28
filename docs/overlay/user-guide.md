# Overlay — User guide

## Create an overlay

- **PULSE → Overlays → New overlay**: _Empty_, or a preset — _Tiny stats_
  (CPU | GPU | RAM), _Thermal strip_ (CPU and GPU temperatures), _Gaming_
  (CPU trend, CPU temperature, GPU group, RAM, download), _Minimal corner_
  (CPU and RAM, no background). Everything is editable afterwards.
- **From a dashboard widget**: _Edit layout_ → hover a widget → _To overlay_ →
  _New overlay_ or _Add to “…”_. The copy keeps the metric, renderer and style.

## Edit and lock

- An overlay starts in **Edit** mode: a thin bar on top — drag it to move the
  window — with _Lock_ and _Open PULSE_, and a grip in the bottom-right corner
  to resize.
- **Lock** hides every control. Where the system allows, clicks then go
  through the overlay to the application behind it, and the overlay can never
  take the keyboard focus.
- To edit again: PULSE → Overlays → _Edit overlay_ / _Edit all_, the tray's
  _Edit overlays_, or the global shortcut.

## Customize

In PULSE → Overlays, each overlay has: name, visible, Edit/Lock, layout (row,
column, grid), background (on/off, colour, opacity — the text stays opaque),
border, shadow, corners, gap, size (_Fit to widgets_), and position where the
system allows it. Each widget has a pixel size, _Customize_ (the full Phase 10
panel: renderer, colours, line, fill, background, fonts, history range…),
reorder, _Copy to dashboard_ and remove.

## Global shortcut

Default **Ctrl+Shift+F12**: locks every overlay if one is editable, unlocks
them all otherwise. Change it in PULSE → Overlays → _Global shortcut_; if the
combination is taken, PULSE says so and keeps the previous one. _Disable_
removes it.

On **GNOME Wayland** global shortcuts only reach PULSE while an X11 window has
focus — use the tray or PULSE → Overlays instead.

## Closing PULSE

- Default: closing the main window **quits PULSE** and closes the overlays.
- _Keep running while overlays are visible_: closing the main window hides it;
  reopen it from the tray or an overlay's _Open PULSE_. Closing the last
  overlay brings the main window back.
- **Quit PULSE** (tray or main window) stops everything.

## What your system allows

PULSE → Overlays → _What overlays can do here_ lists every capability with its
reason. On GNOME Wayland in particular: an application cannot keep itself
above others (use Alt+Space → _Always on Top_ on the overlay), cannot choose
its position (drag it in Edit mode), and click-through depends on the
compositor. See [`platform-capabilities.md`](platform-capabilities.md).

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
