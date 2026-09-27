import type { HistoryPoint, HistoryResponse } from '@/types/history';
import type { MetricRef } from '@/types/metrics';
import { mergeConfig, BASE_CONFIG } from '@/visualization/config';
import type { DeepPartial, VisualizationConfig } from '@/visualization/config';
import type {
  VisualizationData,
  VisualizationMeta,
  VisualizationPoint,
  VisualizationSeries,
} from '@/visualization/types';

/** Shared fixtures for the visualization and history tests. */

export const T0 = Date.UTC(2026, 8, 27, 12, 0, 0);

export const PERCENT_META: VisualizationMeta = {
  label: 'CPU',
  unit: 'percent',
  bounds: { min: 0, max: 100 },
  decimals: 1,
};

export const CELSIUS_META: VisualizationMeta = {
  label: 'Temperature',
  unit: 'celsius',
  bounds: null,
  decimals: 0,
};

export const RATE_META: VisualizationMeta = {
  label: 'Network',
  unit: 'bytesPerSecond',
  bounds: null,
  decimals: 1,
};

/** `count` points every `stepMs` from `T0`, values from `valueAt`. */
export function points(
  count: number,
  valueAt: (index: number) => number = (index) => 20 + (index % 10),
  stepMs = 5_000,
): VisualizationPoint[] {
  return [...Array(count).keys()].map((index) => ({ t: T0 + index * stepMs, v: valueAt(index) }));
}

export function series(id: string, label: string, data: VisualizationPoint[]): VisualizationSeries {
  const last = data[data.length - 1];
  return { id, label, points: data, latest: last ? { t: last.t, v: last.v } : null };
}

export function dataOf(
  list: VisualizationSeries[],
  extra: Partial<VisualizationData> = {},
): VisualizationData {
  return { status: 'ready', series: list, gapThresholdMs: 15_000, ...extra };
}

export function configWith(patch: DeepPartial<VisualizationConfig> = {}): VisualizationConfig {
  return mergeConfig(BASE_CONFIG, patch);
}

export function historyResponse(
  entries: { metric: MetricRef; points: HistoryPoint[] }[],
  extra: Partial<HistoryResponse> = {},
): { status: 'ok' } & HistoryResponse {
  return {
    status: 'ok',
    range: '15m',
    fromMs: T0 - 15 * 60_000,
    toMs: T0 + 60_000,
    bucketMs: 5_000,
    raw: true,
    gapThresholdMs: 15_000,
    series: entries.map((entry) => {
      const last = entry.points[entry.points.length - 1];
      return {
        metric: entry.metric,
        points: entry.points,
        latest: last ? { t: last.t, v: last.v } : null,
      };
    }),
    ...extra,
  };
}
