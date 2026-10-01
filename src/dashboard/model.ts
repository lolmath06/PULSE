import type { HistoryRange } from '@/types/history';
import { DEFAULT_HISTORY_RANGE, isHistoryRange } from '@/types/history';
import type { VisualizationConfig } from '@/visualization/config';
import { BASE_CONFIG, isHexColor, normalizeConfig } from '@/visualization/config';
import { isValidId, newId } from '@/dashboard/ids';
import type { StyleId } from '@/design/styles';
import { isTextKey } from '@/i18n/text';

/**
 * The widget model shared by the dashboard, the Mini window and overlays.
 *
 * A widget is data: *which* metrics (bindings), *how* they are drawn (a Phase
 * 10 visualization configuration), *where* (a grid rectangle on a dashboard,
 * a pixel size in an overlay) and its frame. The same instance can move from a
 * dashboard to an overlay and back without losing anything.
 *
 * Everything read back from storage or an imported file goes through the
 * `normalize*` functions: unknown fields are dropped, invalid ones fall back
 * alone, numbers are bounded, and a widget can never have a negative width, a
 * `NaN` coordinate or an unknown renderer.
 */

export const DASHBOARDS_VERSION = 1;
export const GRID_COLUMNS = 12;
export const ROW_HEIGHT = 44;
export const GRID_GAP = 10;
export const MAX_WIDGETS = 64;
export const MAX_DASHBOARDS = 24;
export const MAX_BINDINGS = 8;

export const WIDGET_KINDS = ['visualization', 'value', 'group', 'summary'] as const;
export type WidgetKind = (typeof WIDGET_KINDS)[number];

export interface KindLimits {
  readonly minW: number;
  readonly minH: number;
  readonly maxW: number;
  readonly maxH: number;
  readonly w: number;
  readonly h: number;
}

/** Grid limits per kind, in columns and rows. */
export const KIND_LIMITS: Readonly<Record<WidgetKind, KindLimits>> = {
  visualization: { minW: 2, minH: 2, maxW: 12, maxH: 12, w: 4, h: 4 },
  value: { minW: 1, minH: 1, maxW: 6, maxH: 4, w: 2, h: 2 },
  group: { minW: 2, minH: 2, maxW: 12, maxH: 8, w: 3, h: 3 },
  summary: { minW: 2, minH: 1, maxW: 12, maxH: 4, w: 6, h: 2 },
};

/** Pixel limits for a widget inside an overlay or the Mini window. */
export const PIXEL_LIMITS = { minWidth: 24, maxWidth: 1600, minHeight: 16, maxHeight: 900 };

export type SourceSelection =
  | { readonly mode: 'auto' }
  /** `ref` is the persistable form from `get_source_refs`: never a MAC or serial. */
  | { readonly mode: 'fixed'; readonly ref: string };

export interface WidgetBinding {
  readonly key: string;
  readonly source: SourceSelection;
  /** Overrides the label shown for this metric, e.g. `↓` for download. */
  readonly label: string | null;
  /**
   * A built-in label's translation key (see `src/i18n/text.ts`); `label` then
   * holds its English text. Absent once the user types their own label.
   */
  readonly labelKey?: string;
}

export const DATA_MODES = ['auto', 'history', 'live'] as const;
export type DataMode = (typeof DATA_MODES)[number];

export interface WidgetFrame {
  readonly showTitle: boolean;
  /** Inner padding in px, 0–24. */
  readonly padding: number;
  /** The widget's own background, or `null` for none. */
  readonly background: string | null;
  /** Opacity of that background only — never of the text or the line. */
  readonly opacity: number;
  readonly border: 'none' | 'thin';
  readonly radius: number;
}

export interface WidgetVisual {
  readonly presetId: string;
  readonly modified: boolean;
  readonly config: VisualizationConfig;
  readonly range: HistoryRange;
}

export interface WidgetGroupOptions {
  readonly orientation: 'rows' | 'inline';
  readonly sparklines: boolean;
}

export interface GridRect {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
}

export interface PixelSize {
  readonly width: number;
  readonly height: number;
}

export interface WidgetInstance {
  readonly id: string;
  readonly kind: WidgetKind;
  readonly title: string | null;
  /** A built-in title's translation key; `title` holds its English text. */
  readonly titleKey?: string;
  readonly bindings: readonly WidgetBinding[];
  readonly visual: WidgetVisual;
  readonly dataMode: DataMode;
  readonly group: WidgetGroupOptions;
  readonly frame: WidgetFrame;
  /** Position on a dashboard grid. */
  readonly layout: GridRect;
  /** Size inside an overlay or the Mini window. */
  readonly size: PixelSize;
}

export interface Dashboard {
  readonly id: string;
  readonly name: string;
  /** A built-in name's translation key; `name` holds its English text. */
  readonly nameKey?: string;
  readonly locked: boolean;
  /** The style this dashboard wears, or `null` for the app's. */
  readonly styleId: StyleId | null;
  /** The template it was made from, so *Reset* can return to it. */
  readonly origin: { readonly template: string; readonly version: number } | null;
  readonly widgets: readonly WidgetInstance[];
}

export interface DashboardsSection {
  readonly version: typeof DASHBOARDS_VERSION;
  readonly activeId: string;
  readonly items: readonly Dashboard[];
}

export const DEFAULT_FRAME: WidgetFrame = {
  showTitle: true,
  padding: 10,
  background: null,
  opacity: 1,
  border: 'thin',
  radius: 10,
};

export const DEFAULT_GROUP: WidgetGroupOptions = { orientation: 'rows', sparklines: false };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function finite(value: unknown, fallback: number, min: number, max: number): number {
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.min(max, Math.max(min, value))
    : fallback;
}

function integer(value: unknown, fallback: number, min: number, max: number): number {
  return Math.round(finite(value, fallback, min, max));
}

/**
 * A user-typed label: bounded, `null` when blank. Not trimmed, so a space
 * typed between two words survives the round trip through the store while
 * the user is still typing.
 */
function text(value: unknown, max: number): string | null {
  if (typeof value !== 'string' || value.trim().length === 0) return null;
  return value.slice(0, max);
}

const KEY = /^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*){1,5}$/;
const REF = /^[a-z][a-z0-9_]*:[a-z0-9][a-z0-9._-]{0,127}$/;

export function normalizeBinding(raw: unknown): WidgetBinding | null {
  if (!isRecord(raw) || typeof raw.key !== 'string' || !KEY.test(raw.key)) return null;
  const source = isRecord(raw.source) ? raw.source : {};
  return {
    key: raw.key,
    source:
      source.mode === 'fixed' && typeof source.ref === 'string' && REF.test(source.ref)
        ? { mode: 'fixed', ref: source.ref }
        : { mode: 'auto' },
    label: text(raw.label, 24),
    ...(isTextKey(raw.labelKey) ? { labelKey: raw.labelKey } : {}),
  };
}

export function clampRect(rect: GridRect, kind: WidgetKind, columns = GRID_COLUMNS): GridRect {
  const limits = KIND_LIMITS[kind];
  const w = integer(rect.w, limits.w, limits.minW, Math.min(limits.maxW, columns));
  const h = integer(rect.h, limits.h, limits.minH, limits.maxH);
  return {
    x: integer(rect.x, 0, 0, columns - w),
    y: integer(rect.y, 0, 0, 999),
    w,
    h,
  };
}

export function clampSize(size: Partial<PixelSize> | undefined, fallback: PixelSize): PixelSize {
  return {
    width: integer(size?.width, fallback.width, PIXEL_LIMITS.minWidth, PIXEL_LIMITS.maxWidth),
    height: integer(size?.height, fallback.height, PIXEL_LIMITS.minHeight, PIXEL_LIMITS.maxHeight),
  };
}

export function normalizeFrame(raw: unknown): WidgetFrame {
  const source = isRecord(raw) ? raw : {};
  return {
    showTitle: typeof source.showTitle === 'boolean' ? source.showTitle : DEFAULT_FRAME.showTitle,
    padding: integer(source.padding, DEFAULT_FRAME.padding, 0, 24),
    background:
      source.background === null
        ? null
        : isHexColor(source.background)
          ? source.background.toLowerCase()
          : DEFAULT_FRAME.background,
    opacity: finite(source.opacity, DEFAULT_FRAME.opacity, 0, 1),
    border:
      source.border === 'none' || source.border === 'thin' ? source.border : DEFAULT_FRAME.border,
    radius: integer(source.radius, DEFAULT_FRAME.radius, 0, 24),
  };
}

/** A default pixel size for a kind, used in overlays and Mini. */
export function defaultPixelSize(kind: WidgetKind, renderer?: string): PixelSize {
  if (kind === 'value') return { width: 110, height: 44 };
  if (kind === 'summary') return { width: 360, height: 56 };
  if (kind === 'group') return { width: 200, height: 96 };
  if (renderer === 'sparkline') return { width: 160, height: 40 };
  return { width: 280, height: 140 };
}

/**
 * Reads one widget. `visualDefaults` gives the configuration a missing or
 * invalid visual falls back to — the metric's own defaults.
 */
export function normalizeWidget(
  raw: unknown,
  visualDefaults: (
    bindings: readonly WidgetBinding[],
    kind: WidgetKind,
  ) => VisualizationConfig = () => BASE_CONFIG,
): WidgetInstance | null {
  if (!isRecord(raw)) return null;
  const kind = (WIDGET_KINDS as readonly string[]).includes(raw.kind as string)
    ? (raw.kind as WidgetKind)
    : 'visualization';
  const bindings = (Array.isArray(raw.bindings) ? raw.bindings : [])
    .slice(0, MAX_BINDINGS)
    .map(normalizeBinding)
    .filter((binding): binding is WidgetBinding => binding !== null);
  if (bindings.length === 0) return null;

  const visualRaw = isRecord(raw.visual) ? raw.visual : {};
  const defaults = visualDefaults(bindings, kind);
  const config = normalizeConfig(visualRaw.config, defaults);
  const group = isRecord(raw.group) ? raw.group : {};
  const layout = isRecord(raw.layout) ? raw.layout : {};
  const limits = KIND_LIMITS[kind];

  return {
    id: isValidId(raw.id) ? raw.id : newId('w'),
    kind,
    title: text(raw.title, 48),
    ...(isTextKey(raw.titleKey) ? { titleKey: raw.titleKey } : {}),
    bindings,
    visual: {
      presetId: typeof visualRaw.presetId === 'string' ? visualRaw.presetId.slice(0, 48) : 'clean',
      modified: visualRaw.modified === true,
      config,
      range: isHistoryRange(visualRaw.range) ? visualRaw.range : DEFAULT_HISTORY_RANGE,
    },
    dataMode: (DATA_MODES as readonly string[]).includes(raw.dataMode as string)
      ? (raw.dataMode as DataMode)
      : 'auto',
    group: {
      orientation: group.orientation === 'inline' ? 'inline' : 'rows',
      sparklines: group.sparklines === true,
    },
    frame: normalizeFrame(raw.frame),
    layout: clampRect(
      {
        x: layout.x as number,
        y: layout.y as number,
        w: (layout.w as number) ?? limits.w,
        h: (layout.h as number) ?? limits.h,
      },
      kind,
    ),
    size: clampSize(
      isRecord(raw.size) ? (raw.size as Partial<PixelSize>) : undefined,
      defaultPixelSize(kind, config.renderer),
    ),
  };
}

/** Gives every widget a unique id, regenerating duplicates. */
export function uniqueWidgetIds(widgets: readonly WidgetInstance[]): WidgetInstance[] {
  const seen = new Set<string>();
  return widgets.map((widget) => {
    if (!seen.has(widget.id)) {
      seen.add(widget.id);
      return widget;
    }
    const id = newId('w');
    seen.add(id);
    return { ...widget, id };
  });
}
