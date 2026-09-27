# Visualization Engine — Renderers

Six renderers, one data model, one formatter. Every renderer reads _current_
from the same summary (`latest` recorded sample), so switching never changes the
number shown.

| Renderer  | Family     | Draws                                          | Needs            |
| --------- | ---------- | ---------------------------------------------- | ---------------- |
| Line      | timeseries | Values over time                               | ≥ 2 points       |
| Area      | timeseries | Line + filled area (solid or gradient)         | ≥ 2 points       |
| Sparkline | timeseries | Chrome-less trend, fitted to the data          | ≥ 2 points       |
| Value     | instant    | `CPU 27 %`, `27 %`, `27` — as large as the box | a current value  |
| Bar       | instant    | `CPU ███████░░░ 72 %`, one row per series      | a current value  |
| Gauge     | instant    | A ring (full, ¾, half), one per series         | **known bounds** |

## One engine for line, area and sparkline

`TimeSeriesChart` renders all three; the variant only changes defaults for
chrome and fill. Shared: gap segmentation, visual smoothing, scales, threshold
colours, markers, tooltip. An area is a line that fills; a sparkline is a line
without chrome.

- **Curves:** straight (`curveLinear`), smooth (`curveMonotoneX` — never
  overshoots a value), stepped (`curveStepAfter`).
- **Markers:** none, small (1.8 px), visible (3 px); skipped above 300 points per
  segment to keep the DOM small.
- **Fill:** none, solid (opacity), gradient (colour → transparent).
- **Bucket band:** aggregated answers draw a faint min–max band behind the line
  so peaks inside a bucket stay visible (not in compact mode).
- **Gaps:** a new path per continuous run.

## Empty and partial states

| Situation                   | Shown                                                                         |
| --------------------------- | ----------------------------------------------------------------------------- |
| Loading, nothing yet        | _Loading history…_                                                            |
| 0 points                    | _Collecting history…_                                                         |
| 1 point (time series)       | the value + _Collecting history… one sample so far_ — no fake line            |
| History unavailable         | _History unavailable: <reason>_; instant renderers still show one live sample |
| Gauge without bounds        | _A gauge needs known bounds…_ — the option is disabled in Customize           |
| One series of several empty | the others draw; legend says _· no data_                                      |

## Bounds, honestly

- **Gauge:** a valid fixed scale the user set, else the metric's natural
  bounds (0–100 % for CPU, memory, GPU), else **unavailable**. A temperature is
  never forced into 0–100.
- **Bar:** same, but an unbounded metric (throughput) is still drawable against
  the window's peak — and the bar says so (_Scaled to the window's peak, …_).

## Y scale

- **Fixed** — exactly `min`–`max`; `max <= min` is refused in the UI and reset
  to Auto on load.
- **Auto** — data extent (bucket extremes included) with 8 % padding. Rates and
  counts start at 0; temperatures do not (40 → 45 °C must stay visible); a
  percentage never pads past 0–100.

## Tooltip

Local time (seconds; date too for ranges over a day), then one row per series:
swatch, label, the **recorded** value — smoothing never reaches it — and, for a
bucket, its `min–max`. Hovering inside a gap shows nothing.

## Formatting

`visualization/format.ts`, shared by every renderer and axis: `27.4 %`,
`67 °C`, `12.8 MiB/s`, `1,234`, `—` for non-finite (never `NaN`). Precision
defaults per unit, overridable per chart.

## Sizes tested

60×20, 80×24, 120×36, 200×50 (sparkline) · 800×300, 1200×450 (line) · named
Small (72 px), Medium (180 px), Large (320 px), Custom (any W×H, clamped to
the container width).
