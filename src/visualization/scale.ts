import type { MetricUnit } from '@/types/metrics';
import type { VisualizationConfig } from '@/visualization/config';
import { scaleError } from '@/visualization/config';
import { valueExtent } from '@/visualization/series';
import type { VisualizationMeta, VisualizationSeries } from '@/visualization/types';

export interface Domain {
  readonly min: number;
  readonly max: number;
}

/** Units whose readings cannot be negative, so an auto scale starts at 0. */
const ZERO_BASED: ReadonlySet<MetricUnit> = new Set<MetricUnit>([
  'percent',
  'bytes',
  'bytesPerSecond',
  'bitsPerSecond',
  'operationsPerSecond',
  'packetsPerSecond',
  'count',
  'rpm',
  'hertz',
  'watts',
]);

/**
 * The value range a time-series chart spans.
 *
 * - **Fixed** — exactly the configured bounds (validated: `max > min`).
 * - **Auto** — the data's extent, bucket extremes included, with a little
 *   padding. Rates and counts start at zero so a quiet link does not look
 *   busy; temperatures do not, because 40 → 45 °C matters and a 0 °C floor
 *   would flatten it. A metric with natural bounds (a percentage) is never
 *   padded past them.
 */
export function yDomain(
  series: readonly VisualizationSeries[],
  config: VisualizationConfig,
  meta: VisualizationMeta,
): Domain {
  if (config.scale.mode === 'fixed' && scaleError(config.scale.min, config.scale.max) === null) {
    return { min: config.scale.min!, max: config.scale.max! };
  }

  const extent = valueExtent(series);
  if (!extent) return meta.bounds ?? { min: 0, max: 1 };

  const span = extent.max - extent.min;
  const pad = span > 0 ? span * 0.08 : Math.max(Math.abs(extent.max) * 0.1, 1);
  let min = ZERO_BASED.has(meta.unit) && extent.min >= 0 ? 0 : extent.min - pad;
  let max = extent.max + pad;

  if (meta.bounds) {
    min = Math.max(min, meta.bounds.min);
    max = Math.min(max, meta.bounds.max);
    if (max <= min) max = min + 1;
  }
  return { min, max };
}

/** Round step for about `count` ticks across `span`. */
function niceStep(span: number, count: number): number {
  const raw = span / Math.max(1, count);
  const power = 10 ** Math.floor(Math.log10(raw));
  const fraction = raw / power;
  const nice =
    fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 2.5 ? 2.5 : fraction <= 5 ? 5 : 10;
  return nice * power;
}

/** Round values inside `[min, max]`, about `count` of them. */
export function niceTicks(min: number, max: number, count: number): number[] {
  if (!Number.isFinite(min) || !Number.isFinite(max) || max <= min || count < 1) return [];
  const step = niceStep(max - min, count);
  const ticks: number[] = [];
  for (let value = Math.ceil(min / step) * step; value <= max + step * 1e-9; value += step) {
    ticks.push(Number(value.toFixed(10)));
    if (ticks.length > 20) break;
  }
  return ticks;
}

const TIME_STEPS_MS = [
  5_000, 10_000, 15_000, 30_000, 60_000, 120_000, 300_000, 600_000, 900_000, 1_800_000, 3_600_000,
  7_200_000, 10_800_000, 21_600_000, 43_200_000, 86_400_000,
];

/**
 * Round clock times inside `[from, to]`, about `count` of them.
 *
 * Aligned in **local** time, so a label reads `14:00` rather than `13:57`.
 */
export function timeTicks(from: number, to: number, count: number): number[] {
  if (!Number.isFinite(from) || !Number.isFinite(to) || to <= from || count < 1) return [];
  const raw = (to - from) / count;
  const step = TIME_STEPS_MS.find((candidate) => candidate >= raw) ?? TIME_STEPS_MS.at(-1)!;
  const offset = new Date(from).getTimezoneOffset() * 60_000;
  const first = Math.ceil((from - offset) / step) * step + offset;
  const ticks: number[] = [];
  for (let t = first; t <= to; t += step) {
    ticks.push(t);
    if (ticks.length > 24) break;
  }
  return ticks;
}

/** Maps `value` in `domain` onto `[from, to]` pixels. */
export function project(value: number, domain: Domain, from: number, to: number): number {
  const span = domain.max - domain.min;
  if (span <= 0) return (from + to) / 2;
  return from + ((value - domain.min) / span) * (to - from);
}
