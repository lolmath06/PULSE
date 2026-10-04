# Overlay — Architecture

> Phase 11. Widgets in their own desktop windows, above other applications
> where the platform allows, click-through when locked where the platform
> allows. A safe desktop window: no DLL injection, no game hooking, no
> DirectX, OpenGL or Vulkan interception.

## Windows

Each overlay is a **real, separate Tauri window** labelled `overlay-<id>`,
loading `index.html?window=overlay&id=<id>`:

| Property        | Setting                                                        |
| --------------- | -------------------------------------------------------------- |
| Frame           | none (`decorations: false`, `shadow: false`)                   |
| Background      | transparent window; the overlay draws its own optional chrome  |
| Stacking        | `always_on_top`, `visible_on_all_workspaces`                   |
| Taskbar         | `skip_taskbar`                                                 |
| Focus           | created **unfocusable** (`focused: false`, `focusable: false`) |
| Size / position | from the configuration, monitor-relative (see below)           |

The ids come from the frontend's random `[a-z0-9-]` generator and are
re-validated by the backend (`overlay::spec::is_valid_id`) before a label is
built. Nothing a user typed ever becomes a window label.

## Who owns what

```text
            overlays section (ui-config.json)
   frontend owns: widgets, layout, chrome, name
   backend owns:  visible, locked, geometry  ──► reconcile(): windows match the config
                        ▲
   written by: the editor · the global shortcut · the tray · a drag in Edit mode
```

`desktop::reconcile` runs after every change to the `overlays` section,
whoever made it: it creates missing visible windows, destroys hidden or deleted
ones, applies Edit/Locked, and moves or resizes a window only when the stored
geometry differs from what it last applied or captured — so a drag never fights
itself. Windows are created from the async runtime, never from a synchronous
command (which deadlocks on Windows).

All platform decisions live in `src-tauri/src/overlay/` (Tauri-free, checked by
the Windows harness): capabilities per display server, geometry, the
section's backend fields, settings and shortcut logic. `src-tauri/src/desktop.rs`
only applies them through Tauri and has no `cfg(target_os)` branch.

## Edit and Locked

|               | Edit                                                             | Locked                                                          |
| ------------- | ---------------------------------------------------------------- | --------------------------------------------------------------- |
| Chrome        | a thin bar (drag region: _Lock_, _Open PULSE_) and a resize grip | none — only the widgets                                         |
| Input         | receives clicks                                                  | `set_ignore_cursor_events(true)` — click-through where honoured |
| Focus         | focusable                                                        | **not focusable**: a game keeps the keyboard                    |
| Move / resize | drag the bar (the compositor moves the window), grip resizes     | impossible                                                      |

Switching: each overlay's _Lock_; _Edit all_ / _Lock all_ in PULSE → Overlays;
the tray; the global shortcut (toggle: if any overlay is editable, lock all,
otherwise unlock all).

The frontend subscribes before loading its configuration snapshot, buffers
events during the load and rejects stale revisions per section. Locked
surfaces are inert and have no edit-control DOM. Edit controls occupy separate
rows above and below the metrics, never an absolute layer over them.
`overlayLayout(overlay, availableWidth)` wraps horizontal rows and reduces grid
columns without shrinking widgets. Passive `ResizeObserver`s measure the
window and edit rows; the overlay sets a native minimum size sufficient for
the resulting content. Fill layouts use natural widget sizes for that minimum
to avoid a resize feedback loop. The preview uses the stored window width.

On Linux, `tao` implements click-through as an input region of **1×1 pixel**
at the window's top-left corner, measured on Fedora/XWayland: clicks pass
through everywhere except that single pixel.

Closing an overlay from the window manager hides it in the configuration
(it returns with _Show all_) instead of destroying a window the configuration
would recreate.

## Geometry, monitors, DPI

`overlay::geometry`. A position is stored as **monitor name + position
relative to that monitor, in logical pixels**, plus a logical size.

- Placement converts with the **target monitor's own scale factor** into
  physical, desktop-absolute coordinates (tested at 100/125/150/200 % and with
  mixed-DPI monitor pairs).
- A missing monitor falls back to the primary one; an overlay outside its
  monitor is clamped back inside; an overlay is never larger than its monitor.
  The placement reports `recovered`.
- After a drag in Edit mode, the window's position is captured on the monitor
  holding its centre and saved (debounced 300 ms).
- On Wayland no position can be set or read: only the size is saved, and the
  stored position is never overwritten with a made-up one.

## Live data

Overlays subscribe to the same live feed as dashboards (one backend 1 s
sampler, union of all windows, deduplicated, in memory only). See
[`../dashboard/widgets.md`](../dashboard/widgets.md#data-live-or-history).
A hidden main window unsubscribes; visible overlays keep updating.

## Main window, tray, quit

- **Closing the main window** — setting _Closing the main window_:
  - **Quit PULSE** (default): overlays close, the live and history schedulers
    stop, SQLite is checkpointed, the configuration flushed, the process exits.
  - **Keep running while overlays are visible**: the main window hides; it
    comes back from the tray, an overlay's _Open PULSE_, or automatically when
    the last visible overlay is closed — PULSE never runs with nothing on screen.
- **Tray** (one icon, built once): Open PULSE · Edit overlays · Lock overlays ·
  Show / hide overlays · Quit PULSE.
- **Quit** from anywhere calls `app.exit(0)` → `RunEvent::Exit`: live feed
  stopped, history stopped and WAL checkpointed, config writer flushed.

## Global shortcut

`tauri-plugin-global-shortcut` 2.3 (the last line compatible with the Rust
1.77.2 MSRV). Default **Ctrl+Shift+F12**, configurable in PULSE → Overlays.
Changing it registers the new shortcut **first** and releases the old one only
on success: a conflict leaves the previous shortcut working and the UI says so
(`overlay::settings::HotkeyManager`, tested with a fake registrar). The saved
shortcut is registered again at startup, off the main thread.

## Permissions

`src-tauri/capabilities/overlay.json`, windows `overlay-*` only:
`core:default`, `core:window:allow-start-dragging`,
`core:window:allow-start-resize-dragging`, `core:window:allow-set-min-size`.
The main and Mini windows keep
`core:default`. No shell, filesystem or HTTP permission was added; every other
overlay action goes through PULSE's own commands.

## Not in Phase 11

Game injection or hooking of any kind, FPS (PULSE has no real FPS source and
never estimates one), automatic game detection, alerts, cloud sync.
