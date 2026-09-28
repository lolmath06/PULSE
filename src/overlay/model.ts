import type { WidgetInstance } from '@/dashboard/model';
import { clampSize, defaultPixelSize, normalizeWidget, uniqueWidgetIds } from '@/dashboard/model';
import { isHexColor } from '@/visualization/config';
import { visualDefaultsFor } from '@/dashboard/dashboards';
import { createWidget, findBlueprint } from '@/dashboard/library';
import { isValidId, newId } from '@/dashboard/ids';

/**
 * The `overlays` section: desktop overlays and what they show.
 *
 * The frontend owns an overlay's contents — widgets, layout, chrome. The
 * backend owns its window and reads `id`, `name`, `visible`, `locked` and
 * `geometry` from the same objects (`src-tauri/src/overlay/spec.rs`), writing
 * back only the position after a drag and the lock state after the shortcut.
 */

export const OVERLAYS_VERSION = 1;
/** Kept equal to the backend's `MAX_OVERLAYS`. */
export const MAX_OVERLAYS = 16;
export const MAX_OVERLAY_WIDGETS = 12;

export const OVERLAY_LAYOUTS = ['horizontal', 'vertical', 'grid'] as const;
export type OverlayLayout = (typeof OVERLAY_LAYOUTS)[number];

export interface OverlayChrome {
  /** `null`: no background at all. */
  readonly background: string | null;
  /** Opacity of the background only — the text and the lines stay opaque. */
  readonly opacity: number;
  readonly border: 'none' | 'thin';
  readonly shadow: boolean;
  readonly radius: number;
  readonly padding: number;
}

export interface OverlayGeometry {
  readonly monitor: string | null;
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface Overlay {
  readonly id: string;
  readonly name: string;
  readonly visible: boolean;
  readonly locked: boolean;
  readonly layout: OverlayLayout;
  readonly columns: number;
  readonly gap: number;
  readonly chrome: OverlayChrome;
  readonly geometry: OverlayGeometry;
  readonly widgets: readonly WidgetInstance[];
}

export interface OverlaysSection {
  readonly version: typeof OVERLAYS_VERSION;
  readonly items: readonly Overlay[];
}

export const EMPTY_OVERLAYS: OverlaysSection = { version: OVERLAYS_VERSION, items: [] };

export const DEFAULT_CHROME: OverlayChrome = {
  background: '#0d1013',
  opacity: 0.72,
  border: 'none',
  shadow: false,
  radius: 8,
  padding: 6,
};

export const DEFAULT_GEOMETRY: OverlayGeometry = {
  monitor: null,
  x: 40,
  y: 40,
  width: 320,
  height: 64,
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function num(value: unknown, fallback: number, min: number, max: number): number {
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.min(max, Math.max(min, value))
    : fallback;
}

export function normalizeChrome(raw: unknown): OverlayChrome {
  const source = isRecord(raw) ? raw : {};
  return {
    background:
      source.background === null
        ? null
        : isHexColor(source.background)
          ? source.background.toLowerCase()
          : DEFAULT_CHROME.background,
    opacity: num(source.opacity, DEFAULT_CHROME.opacity, 0, 1),
    border: source.border === 'thin' ? 'thin' : 'none',
    shadow: source.shadow === true,
    radius: Math.round(num(source.radius, DEFAULT_CHROME.radius, 0, 32)),
    padding: Math.round(num(source.padding, DEFAULT_CHROME.padding, 0, 32)),
  };
}

export function normalizeGeometry(raw: unknown): OverlayGeometry {
  const source = isRecord(raw) ? raw : {};
  return {
    monitor: typeof source.monitor === 'string' ? source.monitor.slice(0, 128) : null,
    x: num(source.x, DEFAULT_GEOMETRY.x, -100_000, 100_000),
    y: num(source.y, DEFAULT_GEOMETRY.y, -100_000, 100_000),
    width: num(source.width, DEFAULT_GEOMETRY.width, 24, 4000),
    height: num(source.height, DEFAULT_GEOMETRY.height, 16, 3000),
  };
}

export function normalizeOverlay(raw: unknown): Overlay | null {
  if (!isRecord(raw) || !isValidId(raw.id)) return null;
  const widgets = (Array.isArray(raw.widgets) ? raw.widgets : [])
    .slice(0, MAX_OVERLAY_WIDGETS)
    .map((widget) => normalizeWidget(widget, visualDefaultsFor))
    .filter((widget): widget is WidgetInstance => widget !== null);
  return {
    id: raw.id,
    name: typeof raw.name === 'string' && raw.name.trim() ? raw.name.slice(0, 40) : 'Overlay',
    visible: raw.visible !== false,
    locked: raw.locked === true,
    layout: (OVERLAY_LAYOUTS as readonly string[]).includes(raw.layout as string)
      ? (raw.layout as OverlayLayout)
      : 'horizontal',
    columns: Math.round(num(raw.columns, 2, 1, 6)),
    gap: Math.round(num(raw.gap, 6, 0, 32)),
    chrome: normalizeChrome(raw.chrome),
    geometry: normalizeGeometry(raw.geometry),
    widgets: uniqueWidgetIds(widgets),
  };
}

export function normalizeOverlays(raw: unknown): OverlaysSection {
  if (!isRecord(raw) || raw.version !== OVERLAYS_VERSION || !Array.isArray(raw.items)) {
    return EMPTY_OVERLAYS;
  }
  const seen = new Set<string>();
  const items: Overlay[] = [];
  for (const entry of raw.items) {
    const overlay = normalizeOverlay(entry);
    if (!overlay || seen.has(overlay.id)) continue;
    seen.add(overlay.id);
    items.push(overlay);
    if (items.length === MAX_OVERLAYS) break;
  }
  return { version: OVERLAYS_VERSION, items };
}

/** The size the widgets need, chrome included, in logical pixels. */
export function contentSize(
  overlay: Pick<Overlay, 'layout' | 'columns' | 'gap' | 'chrome' | 'widgets'>,
) {
  const pad = overlay.chrome.padding * 2;
  const sizes = overlay.widgets.map((widget) => widget.size);
  if (sizes.length === 0) return { width: 160, height: 48 };
  const gaps = (count: number) => Math.max(0, count - 1) * overlay.gap;
  if (overlay.layout === 'horizontal') {
    return {
      width: sizes.reduce((sum, size) => sum + size.width, 0) + gaps(sizes.length) + pad,
      height: Math.max(...sizes.map((size) => size.height)) + pad,
    };
  }
  if (overlay.layout === 'vertical') {
    return {
      width: Math.max(...sizes.map((size) => size.width)) + pad,
      height: sizes.reduce((sum, size) => sum + size.height, 0) + gaps(sizes.length) + pad,
    };
  }
  const columns = Math.max(1, Math.min(overlay.columns, sizes.length));
  const rows = Math.ceil(sizes.length / columns);
  const cellWidth = Math.max(...sizes.map((size) => size.width));
  const cellHeight = Math.max(...sizes.map((size) => size.height));
  return {
    width: columns * cellWidth + gaps(columns) + pad,
    height: rows * cellHeight + gaps(rows) + pad,
  };
}

/** A widget as it appears in an overlay: same everything, its own id and a pixel size. */
export function overlayWidgetFrom(widget: WidgetInstance): WidgetInstance {
  return {
    ...widget,
    id: newId('w'),
    size: clampSize(widget.size, defaultPixelSize(widget.kind, widget.visual.config.renderer)),
  };
}

// --- actions ---------------------------------------------------------------

function withFittedGeometry(overlay: Overlay): Overlay {
  const size = contentSize(overlay);
  return { ...overlay, geometry: { ...overlay.geometry, width: size.width, height: size.height } };
}

export function createOverlay(
  section: OverlaysSection,
  name: string,
  widgets: readonly WidgetInstance[] = [],
  patch: Partial<Omit<Overlay, 'id' | 'widgets'>> = {},
): { section: OverlaysSection; id: string | null } {
  if (section.items.length >= MAX_OVERLAYS) return { section, id: null };
  const overlay = withFittedGeometry({
    id: newId('o'),
    name: name.trim().slice(0, 40) || 'Overlay',
    visible: true,
    locked: false,
    layout: 'horizontal',
    columns: 2,
    gap: 6,
    chrome: DEFAULT_CHROME,
    geometry: DEFAULT_GEOMETRY,
    ...patch,
    widgets: widgets.slice(0, MAX_OVERLAY_WIDGETS).map(overlayWidgetFrom),
  });
  const offset = section.items.length * 24;
  const placed = { ...overlay, geometry: { ...overlay.geometry, x: 40 + offset, y: 40 + offset } };
  return { section: { ...section, items: [...section.items, placed] }, id: placed.id };
}

export function updateOverlay(
  section: OverlaysSection,
  id: string,
  change: (overlay: Overlay) => Overlay,
): OverlaysSection {
  return { ...section, items: section.items.map((item) => (item.id === id ? change(item) : item)) };
}

export function deleteOverlay(section: OverlaysSection, id: string): OverlaysSection {
  return { ...section, items: section.items.filter((item) => item.id !== id) };
}

export function addOverlayWidget(
  section: OverlaysSection,
  id: string,
  widget: WidgetInstance,
): OverlaysSection {
  return updateOverlay(section, id, (overlay) =>
    overlay.widgets.length >= MAX_OVERLAY_WIDGETS
      ? overlay
      : withFittedGeometry({
          ...overlay,
          widgets: [...overlay.widgets, overlayWidgetFrom(widget)],
        }),
  );
}

export function removeOverlayWidget(
  section: OverlaysSection,
  id: string,
  widgetId: string,
): OverlaysSection {
  return updateOverlay(section, id, (overlay) => ({
    ...overlay,
    widgets: overlay.widgets.filter((widget) => widget.id !== widgetId),
  }));
}

export function moveOverlayWidget(
  section: OverlaysSection,
  id: string,
  widgetId: string,
  delta: -1 | 1,
): OverlaysSection {
  return updateOverlay(section, id, (overlay) => {
    const index = overlay.widgets.findIndex((widget) => widget.id === widgetId);
    const target = index + delta;
    if (index < 0 || target < 0 || target >= overlay.widgets.length) return overlay;
    const widgets = [...overlay.widgets];
    [widgets[index], widgets[target]] = [widgets[target]!, widgets[index]!];
    return { ...overlay, widgets };
  });
}

export function updateOverlayWidget(
  section: OverlaysSection,
  id: string,
  widgetId: string,
  change: (widget: WidgetInstance) => WidgetInstance,
): OverlaysSection {
  return updateOverlay(section, id, (overlay) => ({
    ...overlay,
    widgets: overlay.widgets.map((widget) => (widget.id === widgetId ? change(widget) : widget)),
  }));
}

export function fitToContent(section: OverlaysSection, id: string): OverlaysSection {
  return updateOverlay(section, id, withFittedGeometry);
}

// --- presets ---------------------------------------------------------------

export interface OverlayPreset {
  readonly id: string;
  readonly name: string;
  readonly description: string;
  readonly blueprints: readonly string[];
  readonly patch: Partial<Omit<Overlay, 'id' | 'widgets'>>;
}

const TRANSPARENT: OverlayChrome = { ...DEFAULT_CHROME, background: null, opacity: 0 };

/**
 * Compositional starting points. Every part is editable afterwards; none
 * shows a number PULSE cannot measure (there is no FPS here, because PULSE
 * has no real FPS source).
 */
export const OVERLAY_PRESETS: readonly OverlayPreset[] = [
  {
    id: 'tiny-stats',
    name: 'Tiny stats',
    description: 'CPU 24 % | GPU 84 % | RAM 41 %',
    blueprints: ['cpu-value', 'gpu-value', 'memory-value'],
    patch: { layout: 'horizontal' },
  },
  {
    id: 'thermal-strip',
    name: 'Thermal strip',
    description: 'CPU and GPU temperatures in a row.',
    blueprints: ['cpu-temp-value', 'gpu-temp-value'],
    patch: { layout: 'horizontal' },
  },
  {
    id: 'gaming',
    name: 'Gaming',
    description: 'CPU, GPU, VRAM, RAM and network, stacked.',
    blueprints: [
      'cpu-sparkline',
      'cpu-temp-value',
      'gpu-group',
      'memory-value',
      'network-down-value',
    ],
    patch: { layout: 'vertical', chrome: { ...DEFAULT_CHROME, opacity: 0.55 } },
  },
  {
    id: 'minimal-corner',
    name: 'Minimal corner',
    description: 'CPU and RAM, no background at all.',
    blueprints: ['cpu-value', 'memory-value'],
    patch: { layout: 'vertical', chrome: TRANSPARENT },
  },
];

export function createOverlayFromPreset(section: OverlaysSection, preset: OverlayPreset) {
  const widgets = preset.blueprints
    .map((id) => findBlueprint(id))
    .filter((blueprint) => blueprint !== undefined)
    .map((blueprint) => createWidget(blueprint));
  return createOverlay(section, preset.name, widgets, preset.patch);
}
