import type { MetricUnit } from '@/types/metrics';

/**
 * The data a visualization draws — deliberately ignorant of where it came
 * from. History, a live sample, a test fixture or a future overlay feed all
 * produce this shape; no renderer knows that SQLite or Tauri exist.
 */

/** One point. `min`/`max`/`n` are present when the point is a bucket. */
export interface VisualizationPoint {
  readonly t: number;
  readonly v: number;
  readonly min?: number;
  readonly max?: number;
  readonly n?: number;
}

export interface VisualizationSeries {
  /** Stable within the visualization, e.g. a metric ref id. */
  readonly id: string;
  readonly label: string;
  /** Ascending by `t`. Missing data is missing points, never zeros. */
  readonly points: readonly VisualizationPoint[];
  /** The most recent real sample: what *current* shows. */
  readonly latest?: { readonly t: number; readonly v: number } | null;
}

/** What the renderers need to know about the metric itself. */
export interface VisualizationMeta {
  /** e.g. `CPU`, `Memory`, `Network`. */
  readonly label: string;
  /** Canonical unit of every series. */
  readonly unit: MetricUnit;
  /**
   * The metric's natural bounds, when it has any — `0–100` for a percentage.
   * `null` for temperatures, throughput and counts: PULSE never invents one.
   */
  readonly bounds?: { readonly min: number; readonly max: number } | null;
  /** Default precision when the configuration does not set one. */
  readonly decimals?: number;
  /** A short line under the value, e.g. `11.8 GiB used`. */
  readonly secondary?: string;
}

export type VisualizationStatus = 'loading' | 'ready' | 'unavailable';

export interface VisualizationData {
  readonly status: VisualizationStatus;
  /** Why the data is unavailable, when it is. */
  readonly message?: string;
  readonly series: readonly VisualizationSeries[];
  /** Consecutive points further apart than this are drawn with a gap. */
  readonly gapThresholdMs: number;
  /** The time window the chart spans; the data extent when absent. */
  readonly window?: { readonly fromMs: number; readonly toMs: number } | null;
  /** True when points are buckets (they then carry `min`/`max`). */
  readonly aggregated?: boolean;
}

/** An empty, ready dataset. */
export const EMPTY_DATA: VisualizationData = {
  status: 'ready',
  series: [],
  gapThresholdMs: 15_000,
};
