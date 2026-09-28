# Dashboard — Widgets

## Model

`src/dashboard/model.ts`. A widget is data, shared by dashboards, Mini and
overlays:

| Field      | Meaning                                                                        |
| ---------- | ------------------------------------------------------------------------------ |
| `id`       | random `[a-z0-9-]` id — never derived from typed text                          |
| `kind`     | `visualization` · `value` · `group` · `summary`                                |
| `title`    | custom title or `null` (the metric labels are used)                            |
| `bindings` | up to 8 `{ key, source, label }`                                               |
| `visual`   | a Phase 10 `{ presetId, modified, config, range }` — every renderer and option |
| `dataMode` | `auto` · `history` · `live`                                                    |
| `group`    | rows or inline, tiny trends on/off (group and summary)                         |
| `frame`    | show title, padding, background, background opacity, border, corner radius     |
| `layout`   | grid rectangle `{ x, y, w, h }` in cells (dashboards)                          |
| `size`     | pixel size `{ width, height }` (overlays, Mini)                                |

Four kinds, not forty. _CPU Total_, _CPU 23 %_, _CPU 23 % ▁▂▃▅_, _GPU group_
and _System summary_ are all these kinds with different bindings and a Phase 10
configuration.

### Normalisation

Every stored, imported or templated widget goes through `normalizeWidget`:
invalid id → new id; unknown kind → `visualization`; invalid metric key or
source reference → dropped (a widget with no valid binding is dropped);
`NaN`, negative or gigantic geometry → clamped to the kind's limits; an
unknown renderer or an invalid colour → the metric's default (Phase 10
`normalizeConfig`). Tested with deliberately corrupt input.

## CPU Total

The CPU Total widgets bind exactly `cpu.usage.total@cpu:system` — the
backend's own machine-wide measurement. Nothing recomputes it from the logical
processors in the browser (tested). Logical processors are separate widgets
(_One logical processor_, source chosen in Customize).

## Bindings and sources

`src/dashboard/bindings.ts`.

- **Privacy-safe references.** A binding never stores a raw device source id.
  The backend command `get_source_refs` gives, per catalog source, what may be
  persisted: logical sources as they are (`cpu:system`, `cpu:logical-3`,
  `cpu:package-0`, `memory:system`, `…:system`), device sources as the same
  64-bit digest the history database uses (`network:3fa1…`). Without that map
  a device source is refused rather than stored raw.
- **Automatic** picks a reasonable source each time: a hardware interface
  (Ethernet/Wi-Fi) before virtual ones for network, the first readable source
  otherwise. Customize says _Automatic — <source>_ and that it is a default,
  not a claim.
- **Source unavailable.** A fixed source that no longer exists resolves to
  _Source unavailable_; the widget keeps its configuration and Customize lists
  the current candidates to remap to.

## Data: live or history

`src/dashboard/widgetData.ts`.

| Mode    | Source                                          | Used by default for                          |
| ------- | ----------------------------------------------- | -------------------------------------------- |
| live    | the shared live feed: 1 s, last 5 min in memory | value, bar, gauge, sparkline, group, summary |
| history | SQLite history at the chosen range              | line and area                                |

The **live feed** (`src-tauri/src/live/`, `src/live/liveFeed.ts`):

- one backend sampler thread; it samples the **union** of every window's
  subscription once per second — a hundred CPU widgets in three windows are one
  reference in one engine call (tested);
- each window sends **one** subscription (reference-counted over its widgets)
  and drops it while hidden;
- bounded rings: 300 points per reference, dropped with the last subscriber;
- never persisted: history stays at 5 s in SQLite, untouched;
- idle when nothing is subscribed — the thread sleeps;
- refuses `storage.health.*` (an NVMe admin command, read on demand only) and
  per-process sources, with the reason shown in the widget. Regression test:
  ten ticks never send a health key to the engine.

Unavailable readings stay gaps (`null`), never zeros; a widget whose metric the
backend cannot read shows the backend's reason.

## Tiny widgets — responsive density

Every widget decides **what it draws from the box it actually gets**, the same
way on a dashboard, in Mini and in an overlay
(`src/visualization/presentation.ts`, `src/dashboard/geometry.ts`):

| Density | Box                         | Drawn                                                                                                                                                          |
| ------- | --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| micro   | height < 56 or width < 110  | the primary value (label and unit only if they fit); a sparkline **beside** it, never on top; no stats, axes, grid, legend, secondary text, markers or tooltip |
| compact | height < 130 or width < 200 | one-line header, chart, no stats/legend/axes                                                                                                                   |
| normal  | otherwise                   | full header, statistics, legend; axes when there is room                                                                                                       |
| large   | ≥ 640 × 260                 | everything                                                                                                                                                     |

- Parts are given explicit heights and dropped in order (statistics → legend →
  full header → inline header) until the chart keeps its minimum height, so
  nothing is ever painted over something else and nothing overflows.
- Renderers degrade **for rendering only**: a gauge smaller than 64 px draws
  its value; a bar narrower than 90 px draws its value; a bar drops its label
  (under 160 px) before its track or value; a narrow sparkline (< 110 px) is
  the line alone; text drops its label, then its unit, before shrinking.
- The stored configuration is **never** changed: enlarge the widget and the
  detail returns.
- The widget frame drops its title under 72 px of height and its padding
  shrinks to ≤ 3 px under 140×70.
- In edit mode a widget narrower than 300 px or lower than 90 px shows only
  its drag handle and a **“…”** menu (Customize, Duplicate, To overlay,
  Remove) plus the resize grip.

Tested sizes (`presentation.test.tsx`, `Dashboard.test.tsx`): value 60×20,
80×24, 110×44, 160×40; sparkline 80×24, 110×44, 120×32, 160×40; line/area
110×44, 160×80, 320×140, 800×300; group 110×44, 160×80, 280×140. On a
dashboard the smallest widget is 1×1 cell; in an overlay any pixel size from
24×16.

## Templates

Customize → _Save as template_ stores the widget (without position) in the
`templates` section. Templates appear in _Add widget → Templates_, can be
renamed and deleted, and work in dashboards and overlays. Built-in library
entries cannot be deleted.

## Customize

The Phase 10 `CustomizePanel`, unchanged in its options, with an `extra` slot
for the widget's own: title, show title, background, background opacity,
padding, border, corners, metric labels and sources, group layout and trends,
data mode, save as template. No visual option is duplicated.
