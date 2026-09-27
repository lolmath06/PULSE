# Visualization Engine — Customization

## Configuration model

`VisualizationConfig` (`src/visualization/config.ts`), version 1, plain JSON:

| Group        | Fields                                                                                                                                                                       |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `renderer`   | line · area · sparkline · value · bar · gauge                                                                                                                                |
| `size`       | preset (small · medium · large · custom), width (px or fill), height (px)                                                                                                    |
| `line`       | width (0.5–8 px), curve (straight · smooth · stepped), points (none · small · visible)                                                                                       |
| `fill`       | mode (none · solid · gradient), opacity 0–1                                                                                                                                  |
| `colors`     | mode (theme · manual · threshold), primary, secondary, text, grid, background, border, fill (or follow line), gradient start/end, threshold bands, per-series colour + label |
| `background` | mode (none · solid · gradient), opacity 0–1                                                                                                                                  |
| `frame`      | border (none · thin), radius 0–32 px, shadow (none · subtle · glow)                                                                                                          |
| `text`       | scale 60–200 %, weight (regular · medium · bold), label on/off, unit on/off, decimals (auto · 0–4)                                                                           |
| `axes`       | x, y, grid                                                                                                                                                                   |
| `scale`      | auto · fixed, min, max (validated `max > min`)                                                                                                                               |
| `smoothing`  | none · light (3) · smooth (7) — **visual only**                                                                                                                              |
| `display`    | legend, tooltip, current, min, max, average, compact                                                                                                                         |
| `gauge`      | thickness, arc (full · ¾ · half)                                                                                                                                             |

Fonts are PULSE's own; no arbitrary font can be injected. Colours are stored as
`#rrggbb` only.

## Colour modes

- **Theme** — PULSE tokens (`--pulse-viz-1…4`, `--pulse-viz-grid`); a future
  theme recolours every chart. Picking any colour switches to Manual.
- **Manual** — the user's colours, chosen with the platform's own colour
  picker plus an exact hex field.
- **Threshold** — bands the user defines (`≤ 60`, `≤ 85`, `above`), applied to
  the line (hard-stop gradient), markers, value, bar and gauge. For percentages
  the starting bands are 60/85; they are **visual presets**, not a judgement
  PULSE makes about the machine, and every bound and colour is editable.

## Presets

A preset is a partial style over PULSE's base and the chart's defaults. It
never sets a scale, precision, thresholds or series names; only Compact changes
the renderer.

| Preset      | What actually changes                                                                     |
| ----------- | ----------------------------------------------------------------------------------------- |
| Clean       | 2 px smooth line, 28 % gradient, grid + axes, dark panel, thin border                     |
| Minimal     | 1.25 px line, no fill, no grid/axes/panel/border, only the current value, light smoothing |
| Technical   | 1.5 px straight line, sample markers, all stats, sky/amber palette, 4 px corners          |
| Gaming      | 3 px line, 55 % gradient, crimson/gold palette, gradient panel, glow, bold 110 % text     |
| Compact     | sparkline, Small size, compact mode, 80 % text, 60 % panel                                |
| Neon        | cyan/magenta on near-black, glow, 2.5 px line                                             |
| Transparent | no panel at all (opacity 0), no axes — overlay-ready                                      |

**Preset ≠ locked.** After choosing one, any change marks the chart
_Custom (from <preset>)_. **Reset visualization** returns to that preset exactly.
**Save as preset** stores the current style under a name; it is offered to every
chart.

## Customize panel

One component (`CustomizePanel.tsx`) for every chart — a right-hand drawer with
a live preview (itself a `MetricVisualization` in a fixed 332×150 box). Every
control applies immediately to both the preview and the chart on the page;
there is no Save button. Escape closes it.

## Persistence

`localStorage`, key **`pulse.visualization.v1`**:

```json
{ "version": 1,
  "charts": { "cpu.total": { "presetId": "clean", "modified": true,
                             "config": { … }, "range": "6h" } },
  "customPresets": [ { "id": "custom:…", "name": "My green", "style": { … } } ] }
```

Why `localStorage`: presentation preferences of a single-user desktop app,
stored in the webview's own data directory under PULSE's app data, synchronous
(no flash of the default style), and shared by every window of the same origin —
which Phase 11 widgets will be. None of it belongs in the metric database.

- **Versioning:** the version is in the key and the payload. A future format
  uses a new key and migrates from this one, so an older PULSE never clobbers it.
  A payload that is not version 1 is ignored rather than misread.
- **Tolerance:** each chart's config is re-read field by field
  (`normalizeConfig`): unknown properties are dropped, an invalid field falls
  back alone, an invalid fixed scale becomes Auto. Round trip is exact (tested).
- **Not stored:** device or interface selections — their identifiers can be
  derived from a MAC address or a serial — so a relaunch picks the default
  device again. Only code-chosen chart ids, styles, names the user typed and a
  range are stored.
- The range of each chart persists with it.
- Dev (`http://localhost:1421`) and release builds have different webview
  origins, so they keep separate preferences.
