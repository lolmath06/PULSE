import type { PointStyle, RendererKind, VisualizationConfig } from '@/visualization/config';
import { mergeConfig } from '@/visualization/config';
import { rendererInfo } from '@/visualization/registry';

/**
 * Responsive content density — one policy for every surface that draws a
 * metric: dashboard widgets, the Mini window, overlays and the Phase 10 panels.
 *
 * The decision uses the **actual box** the visualization gets, never the size
 * preset the user picked. It only changes what is *rendered*: the stored
 * configuration is never touched, so a widget that grows again shows every
 * detail it was configured with.
 *
 * | Density | Box                                  | What is drawn                                   |
 * | ------- | ------------------------------------ | ----------------------------------------------- |
 * | micro   | height < 56 or width < 110           | one line: `CPU 7 %`, with a sparkline beside it if it fits |
 * | compact | height < 130 or width < 200          | an inline `CPU 7 %` header over a chrome-less chart |
 * | normal  | otherwise                            | full header, legend, statistics, axes if they fit |
 * | large   | width ≥ 640 and height ≥ 260         | everything configured                           |
 *
 * In an exact box, parts are then dropped in order — statistics, legend,
 * full header → inline header → none — until the chart keeps its minimum
 * height. Nothing is ever drawn on top of something else.
 */
export type Density = 'micro' | 'compact' | 'normal' | 'large';

export function densityOf(width: number, height: number): Density {
  if (height < 56 || width < 110) return 'micro';
  if (height < 130 || width < 200) return 'compact';
  if (width >= 640 && height >= 260) return 'large';
  return 'normal';
}

/** The smallest body each renderer family is drawn in. */
export const MIN_BODY: Readonly<Record<'timeseries' | 'value' | 'bar' | 'gauge', number>> = {
  timeseries: 24,
  value: 12,
  bar: 12,
  gauge: 64,
};

/** Part heights at text scale 1, in px. */
export const PART_HEIGHT = { fullHeader: 34, inlineHeader: 18, statsRow: 18, legend: 18 } as const;

export interface Presentation {
  readonly density: Density;
  /** What is drawn. Differs from the stored renderer only as a fallback. */
  readonly renderer: RendererKind;
  readonly fallback: boolean;
  readonly header: 'none' | 'inline' | 'full';
  readonly stats: boolean;
  readonly legend: boolean;
  /** Micro time series: the value text and a sparkline side by side. */
  readonly strip: boolean;
  readonly axes: { readonly x: boolean; readonly y: boolean; readonly grid: boolean };
  readonly points: PointStyle;
  readonly tooltip: boolean;
  readonly heights: {
    readonly header: number;
    readonly legend: number;
    readonly stats: number;
    readonly body: number;
  };
}

export interface PresentationInput {
  readonly config: VisualizationConfig;
  readonly width: number;
  readonly height: number;
  readonly seriesCount: number;
  /** True when the whole visualization must fit `height` (a widget box). */
  readonly exact: boolean;
}

function minBodyFor(renderer: RendererKind): number {
  if (renderer === 'value') return MIN_BODY.value;
  if (renderer === 'bar') return MIN_BODY.bar;
  if (renderer === 'gauge') return MIN_BODY.gauge;
  return MIN_BODY.timeseries;
}

export function presentationFor(input: PresentationInput): Presentation {
  const { config, width, height, seriesCount, exact } = input;
  const density = densityOf(width, height);
  const scale = config.text.scale;

  // Renderer fallbacks — rendering only.
  let renderer = config.renderer;
  if (renderer === 'gauge' && Math.min(width, height) < MIN_BODY.gauge) renderer = 'value';
  if (renderer === 'bar' && width < 90) renderer = 'value';
  const fallback = renderer !== config.renderer;
  const timeseries = rendererInfo(renderer).family === 'timeseries';

  const roomy = density === 'normal' || density === 'large';
  const configuredCompact = config.display.compact || renderer === 'sparkline';
  const wantsText = config.text.showLabel || config.display.current;

  let header: Presentation['header'] = 'none';
  if (timeseries && wantsText && density !== 'micro') {
    header = roomy && !configuredCompact ? 'full' : 'inline';
  }
  // A statistics row needs its width: four figures (and, with several
  // series, the series name) on one line, never wrapped over the chart.
  const statsFit = width >= (seriesCount > 1 ? 420 : 300);
  const statsWanted =
    roomy &&
    statsFit &&
    !configuredCompact &&
    (config.display.current || config.display.min || config.display.max || config.display.average);
  let stats = statsWanted;
  let legend = roomy && !configuredCompact && config.display.legend && seriesCount > 1;

  const headerHeight = (kind: Presentation['header']) =>
    kind === 'full'
      ? PART_HEIGHT.fullHeader * scale
      : kind === 'inline'
        ? PART_HEIGHT.inlineHeader * scale
        : 0;
  const statsHeight = () => (stats ? PART_HEIGHT.statsRow * scale * Math.max(1, seriesCount) : 0);
  const legendHeight = () => (legend ? PART_HEIGHT.legend * scale : 0);

  let body = exact ? height - headerHeight(header) - statsHeight() - legendHeight() : height;
  const minimum = minBodyFor(renderer);
  if (exact) {
    // Drop parts until the chart keeps its minimum height.
    const steps: (() => void)[] = [
      () => (stats = false),
      () => (legend = false),
      () => {
        if (header === 'full') header = 'inline';
      },
      () => (header = 'none'),
    ];
    for (const step of steps) {
      if (body >= minimum) break;
      step();
      body = height - headerHeight(header) - statsHeight() - legendHeight();
    }
    body = Math.max(0, body);
  }

  const chromeless = !roomy || configuredCompact;
  return {
    density,
    renderer,
    fallback,
    header,
    stats,
    legend,
    // A micro time series becomes `CPU 7 %` + a sparkline beside it — except a
    // narrow sparkline, whose line *is* the primary content.
    strip:
      timeseries && density === 'micro' && wantsText && !(renderer === 'sparkline' && width < 110),
    axes: {
      x: !chromeless && config.axes.x && body >= 110,
      y: !chromeless && config.axes.y && width >= 220,
      grid: !chromeless && config.axes.grid,
    },
    points: roomy ? config.line.points : 'none',
    tooltip: config.display.tooltip && density !== 'micro',
    heights: { header: headerHeight(header), legend: legendHeight(), stats: statsHeight(), body },
  };
}

/**
 * The configuration actually drawn: the stored one with the presentation's
 * suppressions applied. A new object — the stored configuration is never
 * mutated.
 */
export function renderedConfig(
  config: VisualizationConfig,
  presentation: Presentation,
): VisualizationConfig {
  return mergeConfig(config, {
    renderer: presentation.renderer,
    axes: presentation.axes,
    line: { points: presentation.points },
    display: {
      tooltip: presentation.tooltip,
      compact: config.display.compact || presentation.density === 'micro',
    },
  });
}

/**
 * Fits one line of text — `label value unit` — in a box, dropping the label,
 * then the unit, before letting the font fall under a readable size.
 */
export function fitLine(
  width: number,
  height: number,
  parts: { readonly label?: string | null; readonly value: string; readonly unit?: string | null },
  scale = 1,
  maxFont = 40,
): {
  readonly fontPx: number;
  readonly label: boolean;
  readonly unit: boolean;
  readonly textWidth: number;
} {
  const base = Math.max(8, Math.min(maxFont, height * 0.62)) * scale;
  // Average advance of a digit or letter, in em — measured on the fallback
  // fonts PULSE meets (Cantarell, Segoe UI, DejaVu), so a label never ends in "…".
  const glyph = 0.62;
  const attempts: [boolean, boolean][] = [
    [Boolean(parts.label), Boolean(parts.unit)],
    [false, Boolean(parts.unit)],
    [false, false],
  ];
  for (const [label, unit] of attempts) {
    const chars =
      parts.value.length +
      (label ? (parts.label?.length ?? 0) + 1 : 0) +
      (unit ? (parts.unit?.length ?? 0) + 1 : 0);
    const fontPx = Math.min(base, width / (chars * glyph));
    if (fontPx >= 9 || (!label && !unit)) {
      const size = Math.max(7, fontPx);
      return { fontPx: size, label, unit, textWidth: Math.ceil(chars * glyph * size) };
    }
  }
  return { fontPx: 9, label: false, unit: false, textWidth: width };
}

/**
 * How a group of values fits its box.
 *
 * - Rows need 13 px each. When they do not fit, a group switches to one
 *   inline strip if each value keeps 44 px; otherwise only the rows that fit
 *   are shown (never squeezed on top of each other).
 * - Labels are dropped before values; tiny trends appear only with room.
 */
export interface GroupLayout {
  readonly orientation: 'rows' | 'inline';
  readonly visible: number;
  readonly fontPx: number;
  readonly labels: boolean;
  readonly sparklines: boolean;
}

export function groupLayout(
  count: number,
  width: number,
  height: number,
  orientation: 'rows' | 'inline',
  sparklines: boolean,
  scale = 1,
): GroupLayout {
  const n = Math.max(1, count);
  let mode = orientation;
  let visible = n;
  if (mode === 'rows' && height / n < 13) {
    if (width / n >= 44) mode = 'inline';
    else visible = Math.max(1, Math.floor(height / 13));
  }
  if (mode === 'inline') {
    const cell = width / n;
    return {
      orientation: 'inline',
      visible: n,
      fontPx: Math.max(8, Math.min(16, height * 0.42, cell / 5)) * scale,
      labels: cell >= 70,
      sparklines: sparklines && height >= 34 && cell >= 120,
    };
  }
  const row = height / visible;
  return {
    orientation: 'rows',
    visible,
    fontPx: Math.max(8, Math.min(16, row * 0.62)) * scale,
    labels: width >= 110,
    sparklines: sparklines && row >= 26 && width >= 200,
  };
}
