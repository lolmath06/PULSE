# Visualization Engine — Architecture

> Phase 10. One generic engine draws every metric, in every representation,
> at every size. Phase 11's dashboard widgets and desktop overlay reuse it
> as-is.

## Pipeline

```text
Metric catalog ─┐
                ├─► history query ─► toVisualizationData ─► MetricVisualization ─► renderer
live sample ────┘   (src/hooks)      (src/utils/history)     (src/visualization)     (SVG)
                                                                  ▲
                                               VisualizationConfig (persisted style)
```

- **`src/visualization/`** is the engine. It knows nothing about SQLite, Tauri,
  history, the Overview page or which chart library draws a path.
- **`src/utils/history.ts`** is the only place that knows both sides: it turns a
  `HistoryResponse` into `VisualizationData`, matching series by reference —
  a series the backend has no data for becomes **empty**, never a line at zero.
- **`src/components/History/`** are thin sections: which series, which meta,
  which defaults. None of them draws anything.

## The contract Phase 11 uses

> Render metric X with configuration Y inside a W × H rectangle.

```tsx
import { MetricVisualization } from '@/visualization';

<MetricVisualization
  data={data} // VisualizationData: series, gapThresholdMs, window, status
  meta={meta} // VisualizationMeta: label, unit, bounds?, decimals?
  config={config} // VisualizationConfig: renderer + every style option
  width={120} // optional: exact outer box
  height={40}
/>;
```

| Input                 | Shape                                                    | Notes                                                                                                              |
| --------------------- | -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `data.series[]`       | `{ id, label, points[{t, v, min?, max?, n?}], latest? }` | Points ascending; a gap is **absent points**                                                                       |
| `data.gapThresholdMs` | number                                                   | Steps larger than this break the line                                                                              |
| `data.window`         | `{ fromMs, toMs }` or null                               | X extent for line/area; sparklines fit the data                                                                    |
| `data.status`         | `loading` · `ready` · `unavailable` (+ `message`)        | Drives the honest empty states                                                                                     |
| `meta`                | `{ label, unit, bounds?, decimals?, secondary? }`        | `bounds` only when the metric has natural ones (0–100 %). Never invented.                                          |
| `config`              | `VisualizationConfig`                                    | See [customization.md](customization.md)                                                                           |
| `width`, `height`     | px, optional                                             | With both: fills exactly that rectangle (header and stats included). Without: container width × configured height. |

That is the whole dependency surface. A widget window or an always-on-top
overlay provides a rectangle, a config and data from the same
`useMetricHistory` hook (or a live sample: `series[].latest` alone is enough for
value/bar/gauge), and gets the same pixels as the main window.

### Ready for the overlay today

- **Transparent backgrounds:** `background.mode: 'none'` or any
  `background.opacity` down to 0 — the backdrop is a separate layer, so the
  line and text keep full opacity. The _Transparent_ preset does exactly this.
- **Micro sizes:** tested at 60×20, 80×24, 120×36 and 200×50 — no axes, no
  legend, no `NaN`, under 40 DOM nodes.
- **Compact mode:** strips axes, grid, legend, header and statistics; shows a
  one-line `CPU 27 %` overlay instead.
- **No page coupling:** nothing reads the router, the Overview layout or a
  global store other than the preference store, which is keyed by chart id.

### Not built in Phase 10

No always-on-top window, click-through, global hotkey, multi-monitor
placement, drag-and-drop dashboard, alerts or notifications.

## The library, and why it is wrapped

`package.json` had no chart library. PULSE draws plain **SVG** itself and uses
[`d3-shape`](https://d3js.org/d3-shape) (ISC, no transitive dependency beyond
`d3-path`) **only** to build path strings for the three curve styles and areas.

- Tree-shaken cost: ~7.7 KB minified / ~2.5 KB gzip.
- Whole Phase 10 frontend: JS bundle 408 041 → 480 764 B (+72.7 KB),
  gzip 123 832 → 146 262 B (+22.4 KB).
- No canvas, no WebGL, fully testable in jsdom, crisp at any size because the
  SVG is sized to its exact pixel box.

Why not a full charting library (Recharts, Chart.js, uPlot…): each would bring
its own tooltip, layout and theming model that PULSE would then fight to
customise this deeply, and most are 10–40× larger. The d3-shape import lives in
exactly one file (`renderers/TimeSeriesChart.tsx`); swapping it touches nothing
else.

## Renderer registry

`registry.ts` lists every renderer with its family (`timeseries` or
`instant`) and a `rendererSupport(kind, meta, config)` check. Adding one:

1. add the kind to `RENDERER_KINDS` in `config.ts`;
2. add an entry to `RENDERERS`;
3. add its component to the switch in `MetricVisualization.tsx`.

The Customize panel lists whatever the registry holds.

## CPU Total

The CPU chart plots `cpu.usage.total@cpu:system` — the backend's own
machine-wide measurement from the kernel's (or Windows') aggregate counters.
It is **not** an average of `cpu.usage.logical` recomputed in the browser: that
would average rounded percentages taken at slightly different instants, and on
Windows the per-processor counters can be unavailable while the total is fine.
Test: _CPU Total plots cpu.usage.total and nothing recomputed from logical
processors_. The per-processor view is separate: small multiples of the eight
most active processors (sparklines), with _Show all_.

## Live updates

`useMetricHistory` loads once, then reloads on `history-sample-recorded` —
**one** Tauri listener shared by every chart, attached with the first subscriber
and detached with the last. Bursts coalesce into one follow-up load. Panels off
screen (`IntersectionObserver`) stop reloading. No component contains a timer
(tested with a `setInterval` spy).
