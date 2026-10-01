import type { WidgetInstance } from '@/dashboard/model';
import { clampSize, defaultPixelSize, normalizeWidget, uniqueWidgetIds } from '@/dashboard/model';
import { isHexColor } from '@/visualization/config';
import { visualDefaultsFor } from '@/dashboard/dashboards';
import { createWidget, findBlueprint } from '@/dashboard/library';
import { isValidId, newId } from '@/dashboard/ids';
import type { StyleId } from '@/design/styles';
import { isStyleId } from '@/design/styles';
import { englishText, hasKey, t } from '@/i18n/i18n';
import { isTextKey } from '@/i18n/text';

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

/**
 * How an overlay is sized along its layout's axis: `content` fits its
 * widgets; `fill` keeps the stored width (row) or height (column) and spreads
 * the widgets across it — a full-width bar, a full-height rail.
 */
export type OverlaySpan = 'content' | 'fill';

/** Where an overlay came from, so it can be recognised and reset. */
export interface OverlayOrigin {
  readonly pack: string;
  readonly version: number;
}

export interface Overlay {
  readonly id: string;
  readonly name: string;
  /** A built-in name's translation key (a pack's); `name` holds its English text. */
  readonly nameKey?: string;
  readonly span: OverlaySpan;
  readonly origin: OverlayOrigin | null;
  /**
   * The style its window wears (colours, type, glow), or `null` for the
   * app's. The chrome below is always explicit: choosing a style writes the
   * style's chrome here once, so an overlay's size never changes behind it.
   */
  readonly styleId: StyleId | null;
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

function normalizeOrigin(raw: unknown): OverlayOrigin | null {
  if (!isRecord(raw) || typeof raw.pack !== 'string' || !isValidId(raw.pack)) return null;
  const version =
    typeof raw.version === 'number' && Number.isInteger(raw.version) ? raw.version : 1;
  return { pack: raw.pack, version: Math.max(1, Math.min(1000, version)) };
}

export function normalizeOverlay(raw: unknown): Overlay | null {
  if (!isRecord(raw) || !isValidId(raw.id)) return null;
  const widgets = (Array.isArray(raw.widgets) ? raw.widgets : [])
    .slice(0, MAX_OVERLAY_WIDGETS)
    .map((widget) => normalizeWidget(widget, visualDefaultsFor))
    .filter((widget): widget is WidgetInstance => widget !== null);
  return {
    id: raw.id,
    name:
      typeof raw.name === 'string' && raw.name.trim()
        ? raw.name.slice(0, 40)
        : englishText('overlays.defaultName'),
    ...(isTextKey(raw.nameKey) ? { nameKey: raw.nameKey } : {}),
    styleId: isStyleId(raw.styleId) ? raw.styleId : null,
    span: raw.span === 'fill' ? 'fill' : 'content',
    origin: normalizeOrigin(raw.origin),
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

/** One child's box inside an overlay, in logical pixels. */
export interface OverlayBox {
  readonly id: string;
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface OverlayLayoutResult {
  readonly width: number;
  readonly height: number;
  readonly boxes: readonly OverlayBox[];
}

/** The size of an overlay with no widget yet. */
export const EMPTY_OVERLAY_SIZE = { width: 160, height: 48 } as const;

/**
 * The exact box of every widget in an overlay — the single source of truth
 * for the real overlay window, the editor preview and *Fit to widgets*.
 *
 * Each child gets exactly its configured pixel size, placed by the layout:
 *
 * - **row**: x advances by `width + gap`; height = tallest child;
 * - **column**: y advances by `height + gap`; width = widest child;
 * - **grid**: `columns` columns; each column as wide as its widest child,
 *   each row as tall as its tallest child.
 *
 * The overlay's padding surrounds everything. Boxes never intersect, by
 * construction, and the overlay size always contains them all.
 */
export function overlayLayout(
  overlay: Pick<Overlay, 'layout' | 'columns' | 'gap' | 'chrome' | 'widgets'> &
    Partial<Pick<Overlay, 'span' | 'geometry'>>,
): OverlayLayoutResult {
  const natural = naturalLayout(overlay);
  if (overlay.span !== 'fill' || !overlay.geometry || overlay.widgets.length === 0) return natural;
  return filledLayout(overlay, natural, overlay.geometry);
}

/**
 * A `fill` overlay: the stored width (row) or height (column) is kept, never
 * smaller than the widgets need, and the extra room is shared equally —
 * each widget's box grows, its content centred in it.
 */
function filledLayout(
  overlay: Pick<Overlay, 'layout' | 'widgets'>,
  natural: OverlayLayoutResult,
  geometry: OverlayGeometry,
): OverlayLayoutResult {
  const n = overlay.widgets.length;
  if (overlay.layout === 'horizontal') {
    const width = Math.max(natural.width, Math.round(geometry.width));
    const extra = (width - natural.width) / n;
    const boxes = natural.boxes.map((box, i) => ({
      ...box,
      x: box.x + extra * i,
      width: box.width + extra,
    }));
    return { width, height: natural.height, boxes };
  }
  if (overlay.layout === 'vertical') {
    const height = Math.max(natural.height, Math.round(geometry.height));
    const extra = (height - natural.height) / n;
    const boxes = natural.boxes.map((box, i) => ({
      ...box,
      y: box.y + extra * i,
      height: box.height + extra,
    }));
    return { width: natural.width, height, boxes };
  }
  return natural;
}

function naturalLayout(
  overlay: Pick<Overlay, 'layout' | 'columns' | 'gap' | 'chrome' | 'widgets'>,
): OverlayLayoutResult {
  const pad = overlay.chrome.padding;
  const gap = overlay.gap;
  const widgets = overlay.widgets;
  if (widgets.length === 0) return { ...EMPTY_OVERLAY_SIZE, boxes: [] };

  if (overlay.layout === 'horizontal') {
    let x = pad;
    const boxes = widgets.map((widget) => {
      const box = {
        id: widget.id,
        x,
        y: pad,
        width: widget.size.width,
        height: widget.size.height,
      };
      x += widget.size.width + gap;
      return box;
    });
    return {
      width: x - gap + pad,
      height: pad * 2 + Math.max(...widgets.map((widget) => widget.size.height)),
      boxes,
    };
  }

  if (overlay.layout === 'vertical') {
    let y = pad;
    const boxes = widgets.map((widget) => {
      const box = {
        id: widget.id,
        x: pad,
        y,
        width: widget.size.width,
        height: widget.size.height,
      };
      y += widget.size.height + gap;
      return box;
    });
    return {
      width: pad * 2 + Math.max(...widgets.map((widget) => widget.size.width)),
      height: y - gap + pad,
      boxes,
    };
  }

  const columns = Math.max(1, Math.min(overlay.columns, widgets.length));
  const rows = Math.ceil(widgets.length / columns);
  const columnWidths = Array.from({ length: columns }, (_, column) =>
    Math.max(
      ...widgets.filter((_, i) => i % columns === column).map((widget) => widget.size.width),
    ),
  );
  const rowHeights = Array.from({ length: rows }, (_, row) =>
    Math.max(
      ...widgets
        .filter((_, i) => Math.floor(i / columns) === row)
        .map((widget) => widget.size.height),
    ),
  );
  const offset = (sizes: readonly number[], index: number) =>
    pad + sizes.slice(0, index).reduce((sum, size) => sum + size + gap, 0);
  const boxes = widgets.map((widget, i) => ({
    id: widget.id,
    x: offset(columnWidths, i % columns),
    y: offset(rowHeights, Math.floor(i / columns)),
    width: widget.size.width,
    height: widget.size.height,
  }));
  const total = (sizes: readonly number[]) =>
    pad * 2 + sizes.reduce((sum, size) => sum + size, 0) + gap * (sizes.length - 1);
  return { width: total(columnWidths), height: total(rowHeights), boxes };
}

/** The size the widgets need, chrome included, in logical pixels. */
export function contentSize(
  overlay: Pick<Overlay, 'layout' | 'columns' | 'gap' | 'chrome' | 'widgets'> &
    Partial<Pick<Overlay, 'span' | 'geometry'>>,
) {
  const { width, height } = overlayLayout(overlay);
  return { width, height };
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
    name: name.trim().slice(0, 40) || englishText('overlays.defaultName'),
    styleId: null,
    span: 'content',
    origin: null,
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

/** A copy of an overlay: new ids, offset a little, same everything else. */
export function duplicateOverlay(section: OverlaysSection, id: string): OverlaysSection {
  const source = section.items.find((item) => item.id === id);
  if (!source || section.items.length >= MAX_OVERLAYS) return section;
  const { nameKey: _builtIn, ...rest } = source;
  // A copy is the user's own overlay, named in the language they see.
  const copy: Overlay = {
    ...rest,
    id: newId('o'),
    name: t('common.copyOf', { name: overlayName(source) }).slice(0, 40),
    widgets: source.widgets.map((widget) => ({ ...widget, id: newId('w') })),
    geometry: { ...source.geometry, x: source.geometry.x + 24, y: source.geometry.y + 24 },
  };
  return { ...section, items: [...section.items, copy] };
}

/** Adds an overlay from one of the user's saved packs. */
export function createOverlayFromUserPack(
  section: OverlaysSection,
  name: string,
  saved: Overlay,
): { section: OverlaysSection; id: string | null } {
  if (section.items.length >= MAX_OVERLAYS) return { section, id: null };
  const offset = section.items.length * 24;
  // Named after the user's pack, never after the built-in pack it began as.
  const { nameKey: _builtIn, ...rest } = saved;
  const overlay: Overlay = {
    ...rest,
    id: newId('o'),
    name: name.slice(0, 40),
    visible: true,
    locked: false,
    widgets: saved.widgets.map((widget) => ({ ...widget, id: newId('w') })),
    geometry:
      saved.span === 'fill'
        ? saved.geometry
        : { ...saved.geometry, x: 40 + offset, y: 40 + offset },
  };
  return { section: { ...section, items: [...section.items, overlay] }, id: overlay.id };
}

export function fitToContent(section: OverlaysSection, id: string): OverlaysSection {
  return updateOverlay(section, id, withFittedGeometry);
}

/** What a style gives an overlay's frame: chrome and gap, written once. */
export interface OverlayStyleChrome {
  readonly chrome: OverlayChrome;
  readonly gap: number;
}

/**
 * Dresses an overlay in a style: its window wears the style's tokens, and the
 * style's chrome and gap are written into the overlay — explicitly, once — so
 * the overlay can still be tuned afterwards and its size never changes behind
 * it. The window is refitted to the new padding.
 */
export function applyOverlayStyle(
  section: OverlaysSection,
  id: string,
  styleId: StyleId | null,
  look: OverlayStyleChrome,
): OverlaysSection {
  return updateOverlay(section, id, (overlay) =>
    withFittedGeometry({ ...overlay, styleId, chrome: look.chrome, gap: look.gap }),
  );
}

/** An overlay's name as shown: a built-in name translated, the user's verbatim. */
export function overlayName(overlay: Pick<Overlay, 'name' | 'nameKey'>): string {
  return overlay.nameKey && hasKey(overlay.nameKey) ? t(overlay.nameKey) : overlay.name;
}

// --- presets ---------------------------------------------------------------

/** Name and description: `presets.overlayPresets.<id>.*` translations. */
export interface OverlayPreset {
  readonly id: string;
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
    blueprints: ['cpu-value', 'gpu-value', 'memory-value'],
    patch: { layout: 'horizontal' },
  },
  {
    id: 'thermal-strip',
    blueprints: ['cpu-temp-value', 'gpu-temp-value'],
    patch: { layout: 'horizontal' },
  },
  {
    id: 'gaming',
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
    blueprints: ['cpu-value', 'memory-value'],
    patch: { layout: 'vertical', chrome: TRANSPARENT },
  },
];

export function createOverlayFromPreset(section: OverlaysSection, preset: OverlayPreset) {
  const widgets = preset.blueprints
    .map((id) => findBlueprint(id))
    .filter((blueprint) => blueprint !== undefined)
    .map((blueprint) => createWidget(blueprint));
  const nameKey = `presets.overlayPresets.${preset.id}.name`;
  return createOverlay(section, englishText(nameKey), widgets, { ...preset.patch, nameKey });
}
