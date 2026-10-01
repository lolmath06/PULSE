# PULSE Widgets

> **Historical — the Phase 0 contract.** The widget engine has since been
> built; today's widgets, renderers and layout are documented in
> [`../dashboard/widgets.md`](../dashboard/widgets.md),
> [`../visualization/renderers.md`](../visualization/renderers.md) and
> [`../dashboard/layout.md`](../dashboard/layout.md). This page is kept as the
> original statement of intent.

## What a widget is

A widget is:

- a **pure rendering** of one or more metric subscriptions,
- plus a **serialisable configuration**,
- placed in a **dashboard layout**.

A widget knows nothing about the operating system. It never learns whether a
temperature came from `hwmon` or from a vendor SDK — that difference was already
resolved by the platform layer, far below it. This is precisely what makes
widgets portable across Windows and Fedora at no cost.

## Anatomy

```ts
interface WidgetDefinition {
  id: string; // stable type id, e.g. "cpu.gauge"
  metrics: MetricId[]; // what it subscribes to
  defaultSize: GridSize;
  configSchema: ConfigSchema; // drives the settings UI
}

interface WidgetInstance {
  instanceId: string;
  definitionId: string;
  position: GridPosition;
  size: GridSize;
  config: WidgetConfig; // serialisable, user-owned
}
```

`WidgetInstance` is user data: it is what gets saved, exported, shared as a
preset, and must therefore stay versioned and forward-compatible.

## Rules

1. **Widgets subscribe; they do not poll.** The metrics engine owns cadence.
2. **Widgets must render every metric state**, including
   `Unavailable { reason }` and `RequiresPermission`. A widget that only renders
   the happy path is incomplete — on Fedora without configured sensors, or on
   Windows without a temperature source, the unhappy path _is_ the normal path.
3. **Widgets are resizable and must degrade.** The same CPU widget may be a
   full graph at large size and a single number at small size. Mini in
   particular renders extremely constrained variants.
4. **Configuration is serialisable and versioned.** Presets, dashboard export
   and multi-monitor setups all depend on it.
5. **No OS branching in a widget.** If a widget needs to know the platform, the
   abstraction below it is wrong.

## Planned families

CPU, GPU, memory, storage, network, sensors (temperature/fan), system info,
and composite widgets (multi-metric panels, gauges, sparklines, text readouts).

## Layout

The Personal dashboard is meant to be extremely free: arbitrary placement,
resizing, and 20+ widgets on screen. That target drives two early decisions:

- rendering must stay cheap enough for many simultaneous widgets, which is why
  sampling is shared and subscription-driven;
- layout must be data, not code, so it can be saved, exported and restored.

## Relationship to modes

A dashboard says _what_ is shown; a mode says _how_ PULSE behaves. The same
widget set can render in Standard and Gaming mode. Mini is a mode with its own
constrained widget variants — not a dashboard that happens to be small.

See [`../architecture/overview.md`](../architecture/overview.md#6-modes-vs-dashboards).
