# PULSE Mini — Permanent Desktop Overlay

> Status: **Phase 0 design notes, superseded in part by Phase 11.**
>
> Phase 11 built the permanent desktop overlay described below — borderless,
> transparent, always-on-top where the platform allows, click-through when
> locked — as **overlays** (`docs/overlay/`), any number of them, each showing
> any widgets. The name _Mini_ now means something simpler: a small,
> **ordinary** PULSE window showing one dashboard (interactive, decorated,
> never always-on-top). The requirements and platform analysis below remain
> the background for the overlay work; see
> [`../overlay/architecture.md`](../overlay/architecture.md) and
> [`../overlay/platform-capabilities.md`](../overlay/platform-capabilities.md)
> for what was built and measured.

## What Mini is — and is not

Mini is **not** a shrunken PULSE window. It is a **permanent desktop overlay**
that stays on screen for as long as PULSE runs, alongside whatever the user is
actually doing. Think of a thin strip of live numbers welded to the edge of the
desktop, not an application window the user switches to.

Concretely it may be configured as:

- an extremely thin line along the bottom of the screen;
- a line along the top;
- a vertical bar on the left or right edge;
- floating text with no background at all;
- transparent or semi-transparent;
- undecorated (no title bar, no borders, no shadow);
- always-on-top;
- optionally click-through, so it never intercepts input.

## Requirements

```text
- borderless
- transparent background
- always-on-top
- optional click-through
- configurable screen anchor
- free positioning where possible
- horizontal or vertical layout
- multi-monitor
- Windows support
- Fedora Wayland support where technically possible
- Fedora X11 support where applicable
- configurable metrics
- configurable colors
- fonts
- opacity
- backgrounds
- borders
- glow/neon effects
- presets
```

## Architectural consequences (already honoured in Phase 0)

1. **The overlay is a separate Tauri window, not a UI state.** A window has its
   own decorations, transparency and always-on-top flags; a CSS class does not.
   Phase 0 names the main window `"main"` in `tauri.conf.json` and scopes the
   default capability to it, so a second window can be added with its own,
   narrower capability set.
2. **Data production must survive the main UI being hidden.** Sampling, history
   and scheduling therefore live in Rust (`metrics/`, `state/`), never in the
   webview. This is the main reason the metrics engine is a backend concern.
3. **The overlay must be cheap.** The metrics engine samples only what is
   subscribed to; an overlay showing three values must cost three values.
4. **Mode ≠ dashboard.** Mini is a _mode_ with its own constrained rendering,
   which is why the two concepts are kept separate from the start
   (see [`overview.md`](overview.md#6-modes-vs-dashboards)).
5. **Styling must be tokenised.** Colours, fonts, opacity and glow are per-user
   configuration. Phase 0 puts every colour in `src/styles/theme.css` so the
   later theming and preset system can drive them.

## Platform reality check

This is the part that must be researched before Mini is implemented, not
during.

### Windows

Well supported. Tauri/`tao` exposes what is needed:

- `decorations: false`, `transparent: true`, `alwaysOnTop: true`;
- click-through via `set_ignore_cursor_events`;
- absolute positioning and per-monitor placement via the monitor API;
- `skipTaskbar: true` to keep the overlay out of Alt-Tab and the taskbar.

Open questions: behaviour over exclusive-fullscreen games (an always-on-top
overlay is generally _not_ drawn over an exclusive-fullscreen surface — borderless
windowed is the realistic target), and DPI handling across mixed-DPI monitors.

### Fedora — Wayland

**The hard case, and the reason this document exists.**

Under Wayland a client cannot position its own windows. `set_position()` is a
no-op, there is no global coordinate space, and "always-on-top" is a compositor
decision, not a client one. A naïve implementation will silently do nothing.

The realistic approaches, to be evaluated when Mini is built:

- **`wlr-layer-shell`** (`zwlr_layer_shell_v1`) — the correct primitive for a
  desktop overlay: anchoring to a screen edge, an explicit layer above normal
  windows, exclusive zones, and per-output placement. Supported by wlroots
  compositors (Sway, Hyprland) and by KDE Plasma. **Not supported by GNOME
  Mutter**, which is Fedora Workstation's default session.
- **GNOME Shell extension** — on a default Fedora Workstation, drawing into the
  shell is the only way to get a true always-on-top strip. This means shipping
  a companion extension, which is a significant, separate piece of work.
- **Graceful degradation** — an undecorated, transparent, non-click-through
  window that the user positions once, accepting that PULSE cannot force its
  placement or stacking.

No approach is universal. Mini on Fedora will need a documented capability
matrix per compositor, and the UI must tell the user honestly what their session
supports rather than pretending the setting applied. This is why the backend
already detects the display server (`display_server()` in the platform layer)
and surfaces it in Phase 0.

### Fedora — X11

Straightforward, and worth supporting because X11 sessions remain common:
override-redirect / `_NET_WM_STATE_ABOVE`, `_NET_WM_WINDOW_TYPE_DOCK`, input
shapes for click-through, and real global coordinates for multi-monitor
placement. Tauri exposes most of this through `tao`.

## Deliberately deferred

Everything above. Phase 0 ships only:

- the `mini` entry in `AppMode` and in the navigation;
- a placeholder page;
- display-server detection in the platform layer;
- these notes.
