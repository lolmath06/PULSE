import type { Smoothing } from '@/visualization/config';
import type { VisualizationPoint, VisualizationSeries } from '@/visualization/types';

/**
 * Pure operations on series: gaps, visual smoothing, summaries, lookup.
 *
 * None of these ever changes a sample. Smoothing returns *new* y values for
 * drawing only; summaries and tooltips always read the points as recorded.
 */

/**
 * Splits points into continuous runs.
 *
 * A step longer than `gapThresholdMs` — PULSE was closed, the machine slept,
 * the metric was unavailable — starts a new run, so no line is ever drawn
 * across time nobody measured. A step that goes *backwards* (the wall clock
 * was set back) also breaks the line rather than folding it over itself.
 */
export function splitSegments(
  points: readonly VisualizationPoint[],
  gapThresholdMs: number,
): VisualizationPoint[][] {
  const segments: VisualizationPoint[][] = [];
  let current: VisualizationPoint[] = [];

  for (const point of points) {
    if (!Number.isFinite(point.t) || !Number.isFinite(point.v)) continue;
    const previous = current[current.length - 1];
    if (previous) {
      const step = point.t - previous.t;
      if (step <= 0 || step > gapThresholdMs) {
        segments.push(current);
        current = [];
      }
    }
    current.push(point);
  }
  if (current.length > 0) segments.push(current);
  return segments;
}

const SMOOTHING_WINDOW: Readonly<Record<Smoothing, number>> = {
  none: 1,
  light: 3,
  smooth: 7,
};

/**
 * Centred moving average over one continuous run, for drawing only.
 *
 * Never crosses a gap — it is applied per segment — and the window shrinks at
 * the ends rather than inventing values beyond them.
 */
export function smoothValues(segment: readonly VisualizationPoint[], level: Smoothing): number[] {
  const window = SMOOTHING_WINDOW[level];
  if (window <= 1 || segment.length < 3) return segment.map((point) => point.v);

  const half = Math.floor(window / 2);
  return segment.map((_, index) => {
    const from = Math.max(0, index - half);
    const to = Math.min(segment.length - 1, index + half);
    let sum = 0;
    for (let i = from; i <= to; i += 1) sum += segment[i]!.v;
    return sum / (to - from + 1);
  });
}

export interface SeriesSummary {
  /** The latest real sample, or the last point when none is known. */
  readonly current: number | null;
  readonly currentAt: number | null;
  /** Lowest reading in the window — a bucket's `min`, not its average. */
  readonly min: number | null;
  /** Highest reading in the window — a bucket's `max`, so peaks survive. */
  readonly max: number | null;
  /** Sample-weighted: a bucket of 24 samples counts 24 times. */
  readonly average: number | null;
  readonly samples: number;
}

/** Current, minimum, maximum and average over the visible points. */
export function summarize(series: VisualizationSeries): SeriesSummary {
  let min = Infinity;
  let max = -Infinity;
  let sum = 0;
  let weight = 0;

  for (const point of series.points) {
    if (!Number.isFinite(point.v)) continue;
    const n = point.n && point.n > 0 ? point.n : 1;
    min = Math.min(min, point.min ?? point.v);
    max = Math.max(max, point.max ?? point.v);
    sum += point.v * n;
    weight += n;
  }

  const last = series.points[series.points.length - 1];
  const current = series.latest ?? (last ? { t: last.t, v: last.v } : null);

  return {
    current: current ? current.v : null,
    currentAt: current ? current.t : null,
    min: weight > 0 ? min : null,
    max: weight > 0 ? max : null,
    average: weight > 0 ? sum / weight : null,
    samples: weight,
  };
}

/**
 * The point nearest to `t`, if one lies within `maxDistance` — so hovering
 * inside a gap shows nothing instead of a sample from an hour earlier.
 */
export function nearestPoint(
  points: readonly VisualizationPoint[],
  t: number,
  maxDistance: number,
): VisualizationPoint | null {
  if (points.length === 0) return null;
  let low = 0;
  let high = points.length - 1;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (points[middle]!.t < t) low = middle + 1;
    else high = middle;
  }
  const candidates = [points[low - 1], points[low]].filter(
    (point): point is VisualizationPoint => point !== undefined,
  );
  let best: VisualizationPoint | null = null;
  for (const candidate of candidates) {
    if (!best || Math.abs(candidate.t - t) < Math.abs(best.t - t)) best = candidate;
  }
  return best && Math.abs(best.t - t) <= maxDistance ? best : null;
}

/** The value extent of every series, bucket extremes included. */
export function valueExtent(
  series: readonly VisualizationSeries[],
): { min: number; max: number } | null {
  let min = Infinity;
  let max = -Infinity;
  for (const entry of series) {
    for (const point of entry.points) {
      if (!Number.isFinite(point.v)) continue;
      min = Math.min(min, point.min ?? point.v);
      max = Math.max(max, point.max ?? point.v);
    }
    if (entry.latest && Number.isFinite(entry.latest.v)) {
      min = Math.min(min, entry.latest.v);
      max = Math.max(max, entry.latest.v);
    }
  }
  return min <= max ? { min, max } : null;
}

/** The time extent of every series. */
export function timeExtent(
  series: readonly VisualizationSeries[],
): { min: number; max: number } | null {
  let min = Infinity;
  let max = -Infinity;
  for (const entry of series) {
    const first = entry.points[0];
    const last = entry.points[entry.points.length - 1];
    if (first) min = Math.min(min, first.t);
    if (last) max = Math.max(max, last.t);
  }
  return min <= max ? { min, max } : null;
}

/** How many points the richest series has. */
export function maxPointCount(series: readonly VisualizationSeries[]): number {
  return series.reduce((most, entry) => Math.max(most, entry.points.length), 0);
}
