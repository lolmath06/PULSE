import { t } from '@/i18n/i18n';

/**
 * The visualization configuration: everything that decides how one metric is
 * drawn, and nothing about which metric or where its data comes from.
 *
 * Plain, JSON-serialisable data. A configuration saved today is read back by
 * {@link normalizeConfig}, which checks every field on its own and falls back
 * to the default for any it cannot use — so an older file, a newer file with
 * extra properties, or a hand-edited one never breaks a chart.
 */

export const VISUALIZATION_CONFIG_VERSION = 1;

/** Every renderer. Adding one means adding it here and to the registry. */
export const RENDERER_KINDS = ['line', 'area', 'sparkline', 'value', 'bar', 'gauge'] as const;
export type RendererKind = (typeof RENDERER_KINDS)[number];

export const SIZE_PRESETS = ['small', 'medium', 'large', 'custom'] as const;
export type SizePreset = (typeof SIZE_PRESETS)[number];

export const CURVE_STYLES = ['straight', 'smooth', 'stepped'] as const;
export type CurveStyle = (typeof CURVE_STYLES)[number];

export const POINT_STYLES = ['none', 'small', 'visible'] as const;
export type PointStyle = (typeof POINT_STYLES)[number];

export const FILL_MODES = ['none', 'solid', 'gradient'] as const;
export type FillMode = (typeof FILL_MODES)[number];

export const BACKGROUND_MODES = ['none', 'solid', 'gradient'] as const;
export type BackgroundMode = (typeof BACKGROUND_MODES)[number];

export const COLOR_MODES = ['theme', 'manual', 'threshold'] as const;
export type ColorMode = (typeof COLOR_MODES)[number];

export const SMOOTHING_LEVELS = ['none', 'light', 'smooth'] as const;
export type Smoothing = (typeof SMOOTHING_LEVELS)[number];

export const SCALE_MODES = ['auto', 'fixed'] as const;
export type ScaleMode = (typeof SCALE_MODES)[number];

export const BORDER_STYLES = ['none', 'thin'] as const;
export type BorderStyle = (typeof BORDER_STYLES)[number];

export const SHADOW_STYLES = ['none', 'subtle', 'glow'] as const;
export type ShadowStyle = (typeof SHADOW_STYLES)[number];

export const FONT_WEIGHTS = ['regular', 'medium', 'bold'] as const;
export type FontWeight = (typeof FONT_WEIGHTS)[number];

export const GAUGE_ARCS = ['full', 'three-quarter', 'half'] as const;
export type GaugeArc = (typeof GAUGE_ARCS)[number];

/**
 * One colour band. Bands are ordered; a value takes the colour of the first
 * band whose `upTo` it does not exceed, and the last band (`upTo: null`)
 * catches everything above.
 *
 * These are **visual** thresholds the user chose, not a judgement PULSE makes
 * about the system.
 */
export interface ThresholdBand {
  readonly upTo: number | null;
  readonly color: string;
}

/** Per-series overrides, by position: `'0'` is the first series. */
export interface SeriesStyle {
  readonly color?: string;
  readonly label?: string;
}

export interface VisualizationConfig {
  readonly version: typeof VISUALIZATION_CONFIG_VERSION;
  readonly renderer: RendererKind;
  readonly size: {
    readonly preset: SizePreset;
    /** Custom width in px; `null` fills the container. */
    readonly width: number | null;
    /** Custom height in px; used when `preset` is `custom`. */
    readonly height: number | null;
  };
  readonly line: {
    /** Stroke width in px. */
    readonly width: number;
    readonly curve: CurveStyle;
    readonly points: PointStyle;
  };
  readonly fill: {
    readonly mode: FillMode;
    /** 0–1. */
    readonly opacity: number;
  };
  readonly colors: {
    readonly mode: ColorMode;
    readonly primary: string;
    readonly secondary: string;
    readonly text: string;
    readonly grid: string;
    readonly background: string;
    readonly border: string;
    /** `null` follows the series colour. */
    readonly fill: string | null;
    readonly gradientStart: string;
    readonly gradientEnd: string;
    readonly thresholds: readonly ThresholdBand[];
    readonly series: Readonly<Record<string, SeriesStyle>>;
  };
  readonly background: {
    readonly mode: BackgroundMode;
    /** 0–1. `0` is fully transparent — the future overlay's default. */
    readonly opacity: number;
  };
  readonly frame: {
    readonly border: BorderStyle;
    /** Corner radius in px. */
    readonly radius: number;
    readonly shadow: ShadowStyle;
  };
  readonly text: {
    /** Multiplies every font size, 0.6–2. */
    readonly scale: number;
    readonly weight: FontWeight;
    readonly showLabel: boolean;
    readonly showUnit: boolean;
    /** `null` uses the metric's own precision. */
    readonly decimals: number | null;
  };
  readonly axes: {
    readonly x: boolean;
    readonly y: boolean;
    readonly grid: boolean;
  };
  readonly scale: {
    readonly mode: ScaleMode;
    readonly min: number | null;
    readonly max: number | null;
  };
  /** Visual only: never changes a stored or displayed sample. */
  readonly smoothing: Smoothing;
  readonly display: {
    readonly legend: boolean;
    readonly tooltip: boolean;
    readonly current: boolean;
    readonly min: boolean;
    readonly max: boolean;
    readonly average: boolean;
    /** Strips axes, grid, legend and the header: the micro-graph mode. */
    readonly compact: boolean;
  };
  readonly gauge: {
    /** Ring thickness as a fraction of the radius, 0.05–0.4. */
    readonly thickness: number;
    readonly arc: GaugeArc;
  };
}

/** A recursive partial, for presets and patches. */
export type DeepPartial<T> = {
  readonly [K in keyof T]?: T[K] extends readonly unknown[]
    ? T[K]
    : T[K] extends object
      ? DeepPartial<T[K]>
      : T[K];
};

/** Pixel heights of the named sizes. Width always follows the container. */
export const SIZE_HEIGHTS: Readonly<Record<Exclude<SizePreset, 'custom'>, number>> = {
  small: 72,
  medium: 180,
  large: 320,
};

export const LIMITS = {
  lineWidth: { min: 0.5, max: 8 },
  opacity: { min: 0, max: 1 },
  radius: { min: 0, max: 32 },
  textScale: { min: 0.6, max: 2 },
  decimals: { min: 0, max: 4 },
  gaugeThickness: { min: 0.05, max: 0.4 },
  width: { min: 40, max: 4000 },
  height: { min: 16, max: 2000 },
} as const;

/**
 * PULSE's own look — the base every preset and every chart default starts
 * from. Theme-coloured, dark panel, subtle grid, gentle curve.
 */
export const BASE_CONFIG: VisualizationConfig = {
  version: VISUALIZATION_CONFIG_VERSION,
  renderer: 'line',
  size: { preset: 'medium', width: null, height: null },
  line: { width: 2, curve: 'smooth', points: 'none' },
  fill: { mode: 'gradient', opacity: 0.35 },
  colors: {
    mode: 'theme',
    primary: '#38d6c4',
    secondary: '#8f9cff',
    text: '#e6ebf1',
    grid: '#232a32',
    background: '#14181d',
    border: '#313a45',
    fill: null,
    gradientStart: '#38d6c4',
    gradientEnd: '#14181d',
    thresholds: [],
    series: {},
  },
  background: { mode: 'solid', opacity: 1 },
  frame: { border: 'thin', radius: 10, shadow: 'none' },
  text: { scale: 1, weight: 'medium', showLabel: true, showUnit: true, decimals: null },
  axes: { x: true, y: true, grid: true },
  scale: { mode: 'auto', min: null, max: null },
  smoothing: 'none',
  display: {
    legend: true,
    tooltip: true,
    current: true,
    min: true,
    max: true,
    average: true,
    compact: false,
  },
  gauge: { thickness: 0.16, arc: 'three-quarter' },
};

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Deep-merges `patch` over `base`. Arrays and scalars are replaced. */
export function mergeConfig<T>(base: T, patch: DeepPartial<T> | undefined): T {
  if (!patch) return base;
  if (!isPlainObject(base) || !isPlainObject(patch)) return (patch as T) ?? base;

  const out: Record<string, unknown> = { ...base };
  for (const [key, value] of Object.entries(patch)) {
    if (value === undefined) continue;
    const current = (base as Record<string, unknown>)[key];
    out[key] =
      isPlainObject(current) && isPlainObject(value) && key !== 'series'
        ? mergeConfig(current, value)
        : value;
  }
  return out as T;
}

const HEX = /^#[0-9a-f]{6}$/i;

/** A `#rrggbb` colour. The only format the configuration stores. */
export function isHexColor(value: unknown): value is string {
  return typeof value === 'string' && HEX.test(value);
}

function oneOf<T extends string>(options: readonly T[], value: unknown, fallback: T): T {
  return typeof value === 'string' && (options as readonly string[]).includes(value)
    ? (value as T)
    : fallback;
}

function clampNumber(value: unknown, min: number, max: number, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.min(max, Math.max(min, value))
    : fallback;
}

function nullableNumber(
  value: unknown,
  fallback: number | null,
  min = -Infinity,
  max = Infinity,
): number | null {
  if (value === null) return null;
  if (typeof value === 'number' && Number.isFinite(value))
    return Math.min(max, Math.max(min, value));
  return fallback;
}

function bool(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback;
}

function color(value: unknown, fallback: string): string {
  return isHexColor(value) ? value.toLowerCase() : fallback;
}

function thresholds(value: unknown, fallback: readonly ThresholdBand[]): ThresholdBand[] {
  if (!Array.isArray(value)) return [...fallback];
  const bands: ThresholdBand[] = [];
  for (const entry of value.slice(0, 8)) {
    if (!isPlainObject(entry) || !isHexColor(entry.color)) continue;
    const upTo = nullableNumber(entry.upTo, null);
    bands.push({ upTo, color: entry.color.toLowerCase() });
  }
  return sortThresholds(bands);
}

/** Orders bands ascending, the open-ended one last. */
export function sortThresholds(bands: readonly ThresholdBand[]): ThresholdBand[] {
  return [...bands].sort((a, b) => (a.upTo ?? Infinity) - (b.upTo ?? Infinity));
}

function seriesStyles(value: unknown): Record<string, SeriesStyle> {
  if (!isPlainObject(value)) return {};
  const out: Record<string, SeriesStyle> = {};
  for (const [slot, style] of Object.entries(value)) {
    if (!/^\d{1,2}$/.test(slot) || !isPlainObject(style)) continue;
    const entry: { color?: string; label?: string } = {};
    if (isHexColor(style.color)) entry.color = style.color.toLowerCase();
    if (typeof style.label === 'string' && style.label.trim()) {
      entry.label = style.label.trim().slice(0, 40);
    }
    if (entry.color || entry.label) out[slot] = entry;
  }
  return out;
}

/**
 * Reads a stored configuration against `defaults`, field by field.
 *
 * - Unknown properties are ignored — a newer PULSE may add some.
 * - An invalid or missing field takes the default's value, alone.
 * - A fixed scale with `max <= min` is reset to auto: it cannot be drawn.
 */
export function normalizeConfig(
  raw: unknown,
  defaults: VisualizationConfig = BASE_CONFIG,
): VisualizationConfig {
  const source = isPlainObject(raw) ? raw : {};
  const get = (key: string): Record<string, unknown> =>
    isPlainObject(source[key]) ? (source[key] as Record<string, unknown>) : {};

  const size = get('size');
  const line = get('line');
  const fill = get('fill');
  const colors = get('colors');
  const background = get('background');
  const frame = get('frame');
  const text = get('text');
  const axes = get('axes');
  const scale = get('scale');
  const display = get('display');
  const gauge = get('gauge');
  const d = defaults;

  const scaleMode = oneOf(SCALE_MODES, scale.mode, d.scale.mode);
  const scaleMin = nullableNumber(scale.min, d.scale.min);
  const scaleMax = nullableNumber(scale.max, d.scale.max);
  const fixedScaleValid = scaleMin !== null && scaleMax !== null && scaleMax > scaleMin;

  return {
    version: VISUALIZATION_CONFIG_VERSION,
    renderer: oneOf(RENDERER_KINDS, source.renderer, d.renderer),
    size: {
      preset: oneOf(SIZE_PRESETS, size.preset, d.size.preset),
      width: nullableNumber(size.width, d.size.width, LIMITS.width.min, LIMITS.width.max),
      height: nullableNumber(size.height, d.size.height, LIMITS.height.min, LIMITS.height.max),
    },
    line: {
      width: clampNumber(line.width, LIMITS.lineWidth.min, LIMITS.lineWidth.max, d.line.width),
      curve: oneOf(CURVE_STYLES, line.curve, d.line.curve),
      points: oneOf(POINT_STYLES, line.points, d.line.points),
    },
    fill: {
      mode: oneOf(FILL_MODES, fill.mode, d.fill.mode),
      opacity: clampNumber(fill.opacity, 0, 1, d.fill.opacity),
    },
    colors: {
      mode: oneOf(COLOR_MODES, colors.mode, d.colors.mode),
      primary: color(colors.primary, d.colors.primary),
      secondary: color(colors.secondary, d.colors.secondary),
      text: color(colors.text, d.colors.text),
      grid: color(colors.grid, d.colors.grid),
      background: color(colors.background, d.colors.background),
      border: color(colors.border, d.colors.border),
      fill:
        colors.fill === null
          ? null
          : isHexColor(colors.fill)
            ? colors.fill.toLowerCase()
            : d.colors.fill,
      gradientStart: color(colors.gradientStart, d.colors.gradientStart),
      gradientEnd: color(colors.gradientEnd, d.colors.gradientEnd),
      thresholds: thresholds(colors.thresholds, d.colors.thresholds),
      series: 'series' in colors ? seriesStyles(colors.series) : { ...d.colors.series },
    },
    background: {
      mode: oneOf(BACKGROUND_MODES, background.mode, d.background.mode),
      opacity: clampNumber(background.opacity, 0, 1, d.background.opacity),
    },
    frame: {
      border: oneOf(BORDER_STYLES, frame.border, d.frame.border),
      radius: clampNumber(frame.radius, LIMITS.radius.min, LIMITS.radius.max, d.frame.radius),
      shadow: oneOf(SHADOW_STYLES, frame.shadow, d.frame.shadow),
    },
    text: {
      scale: clampNumber(text.scale, LIMITS.textScale.min, LIMITS.textScale.max, d.text.scale),
      weight: oneOf(FONT_WEIGHTS, text.weight, d.text.weight),
      showLabel: bool(text.showLabel, d.text.showLabel),
      showUnit: bool(text.showUnit, d.text.showUnit),
      decimals:
        text.decimals === null
          ? null
          : typeof text.decimals === 'number' && Number.isInteger(text.decimals)
            ? Math.min(LIMITS.decimals.max, Math.max(LIMITS.decimals.min, text.decimals))
            : d.text.decimals,
    },
    axes: {
      x: bool(axes.x, d.axes.x),
      y: bool(axes.y, d.axes.y),
      grid: bool(axes.grid, d.axes.grid),
    },
    scale:
      scaleMode === 'fixed' && !fixedScaleValid
        ? { mode: 'auto', min: scaleMin, max: scaleMax }
        : { mode: scaleMode, min: scaleMin, max: scaleMax },
    smoothing: oneOf(SMOOTHING_LEVELS, source.smoothing, d.smoothing),
    display: {
      legend: bool(display.legend, d.display.legend),
      tooltip: bool(display.tooltip, d.display.tooltip),
      current: bool(display.current, d.display.current),
      min: bool(display.min, d.display.min),
      max: bool(display.max, d.display.max),
      average: bool(display.average, d.display.average),
      compact: bool(display.compact, d.display.compact),
    },
    gauge: {
      thickness: clampNumber(
        gauge.thickness,
        LIMITS.gaugeThickness.min,
        LIMITS.gaugeThickness.max,
        d.gauge.thickness,
      ),
      arc: oneOf(GAUGE_ARCS, gauge.arc, d.gauge.arc),
    },
  };
}

/** Why a fixed scale cannot be used, or `null` when it can. */
export function scaleError(min: number | null, max: number | null): string | null {
  if (min === null || max === null) return t('visualization.scaleErrors.both');
  if (!Number.isFinite(min) || !Number.isFinite(max)) return t('visualization.scaleErrors.numbers');
  if (max <= min) return t('visualization.scaleErrors.order');
  return null;
}

/**
 * The parts of a configuration that are *style*, i.e. meaningful on any
 * metric. A saved preset keeps only these: a scale, a precision, colour
 * thresholds and series names belong to the chart they were made for.
 */
export function styleOf(config: VisualizationConfig): DeepPartial<VisualizationConfig> {
  const { scale: _scale, version: _version, ...rest } = config;
  const { thresholds: _thresholds, series: _series, ...colors } = config.colors;
  const { decimals: _decimals, ...text } = config.text;
  return { ...rest, colors, text };
}

/** The pixel height a configuration asks for, given no external constraint. */
export function configuredHeight(config: VisualizationConfig): number {
  if (config.size.preset === 'custom') {
    return config.size.height ?? SIZE_HEIGHTS.medium;
  }
  return SIZE_HEIGHTS[config.size.preset];
}
