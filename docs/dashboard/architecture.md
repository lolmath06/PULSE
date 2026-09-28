# Dashboard — Architecture

> Phase 11. Configurable dashboards of widgets, drawn by the Phase 10 engine,
> shared with the Mini window and desktop overlays.

```text
 ui-config.json  (one file, backend-owned, atomic, versioned)
     │  get_ui_config / set_ui_config_section / ui-config-changed
     ▼
 src/config/uiConfig.ts ── sections: visualization · dashboards · overlays · templates · settings
     │
     ├─ dashboards ─► DashboardPage ─► DashboardGrid ─► WidgetCard ─► WidgetContent ─► MetricVisualization
     ├─ overlays ───► OverlayApp (overlay windows) ────► WidgetCard ─► …same…
     └─ dashboards ─► MiniApp (Mini window) ───────────► WidgetCard ─► …same…

 widget data:  history (SQLite, get_metric_history)   or   live (LiveHub, 1 s, in memory)
```

There is **one** drawing path. A widget on the grid, in the Mini window and in
an overlay is the same `WidgetCard` → `WidgetContent` → Phase 10
`MetricVisualization`. No second chart engine exists for the overlay.

Dashboards, widgets and overlays are **not** metric providers: the engine
still has six providers, and none of this code samples hardware directly.

## Shared UI configuration (`UiConfigStore`)

Phase 10 stored visual preferences in each webview's `localStorage`. With
several windows — and different webview origins for development and release
builds — that would have meant diverging copies. Phase 11 moves every UI
setting to one document owned by the backend.

| Property      | How                                                                                                             |
| ------------- | --------------------------------------------------------------------------------------------------------------- |
| Location      | `<app config dir>/ui-config.json` — `~/.config/dev.pulse.app/` on Fedora, `%APPDATA%\dev.pulse.app\` on Windows |
| Version       | `"version": 1`; a newer file is never overwritten (read-only, defaults in memory)                               |
| Sections      | `visualization`, `dashboards`, `overlays`, `templates`, `settings` — each a JSON object; nothing else is kept   |
| Size          | 4 MiB at most                                                                                                   |
| Atomic writes | `ui-config.json.tmp` → fsync → previous file copied to `.bak` → rename over the real file                       |
| Corruption    | the file is moved to `ui-config.corrupt-<ms>.json` (kept, never deleted), the `.bak` is tried, else defaults    |
| Coalescing    | the writer thread waits 400 ms of quiet; a dragged slider is one write                                          |
| Sync          | every accepted write is broadcast as `ui-config-changed` with the section's new value                           |

The backend validates the **shape** (known sections, objects, size). Each
feature normalises its section's **contents** field by field on read — unknown
properties dropped, invalid values replaced alone — so an imported or hand-
edited file can never crash a window.

The frontend store (`src/config/uiConfig.ts`) debounces writes per section
(150 ms), ignores its own echoes, and falls back to an in-memory document when
no backend exists (tests, a plain browser).

### Phase 10 migration

On the first load without a `visualization` section, the Phase 10 payload in
`localStorage` (`pulse.visualization.v1`) is copied into it once
(`src/config/migrations.ts`). The old key is left in place and never read
again. Verified on Fedora: the development build's Phase 10 preferences
appeared in `ui-config.json` on first launch.

## Dashboards

`src/dashboard/dashboards.ts` — pure functions from one section to the next:
create, rename, duplicate (new ids everywhere), delete (never the last one),
reset to the default widgets (with confirmation; templates are untouched),
lock/unlock, add/remove/duplicate/update/move/resize widgets, import/export.

- **Default dashboard:** a system summary strip, CPU Total, memory,
  CPU & GPU temperature, GPU usage, network and storage — seven widgets, with
  stable ids.
- **Locked by default.** _Edit layout_ reveals the handles; _Lock layout_
  hides them.
- **Import/export:** a JSON object `{ format: "pulse.dashboard", version: 1,
dashboard }`. Bindings hold privacy-safe source references (below), so an
  export contains no MAC address, serial, path or user name. On another
  machine a device-bound widget shows _Source unavailable_ and can be remapped.

## Widgets and live data

See [`widgets.md`](widgets.md) for the model, kinds, bindings and data modes,
and [`layout.md`](layout.md) for the grid.

## Mini window

`index.html?window=mini` — a small, **ordinary** window (decorated, focusable,
in the taskbar, never always-on-top, never click-through) that shows one
dashboard stacked in a single column. Opened from _Mini_ or _Overlays_.
Overlays are the always-on-top, click-through surface; see
[`../overlay/architecture.md`](../overlay/architecture.md).
