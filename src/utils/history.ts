import type { MetricRef } from '@/types/metrics';
import type { HistoryResponse } from '@/types/history';
import { metricRefId } from '@/types/wellknown';
import type { VisualizationData, VisualizationSeries } from '@/visualization/types';

/** One plotted series of a history panel. */
export interface HistorySeriesSpec {
  readonly ref: MetricRef;
  readonly label: string;
}

/**
 * Turns a history answer into what the visualization engine draws.
 *
 * The one place that knows both sides: the engine never sees a
 * `HistoryResponse`, and history never sees a renderer. Series are matched by
 * reference, not by position, and a requested series the backend has no data
 * for becomes an **empty** series — drawn as nothing — never a line at zero.
 */
export function toVisualizationData(
  response: HistoryResponse | null,
  specs: readonly HistorySeriesSpec[],
  status: VisualizationData['status'],
  message?: string,
): VisualizationData {
  const byId = new Map(
    (response?.series ?? []).map((series) => [metricRefId(series.metric), series]),
  );

  const series: VisualizationSeries[] = specs.map((spec) => {
    const id = metricRefId(spec.ref);
    const found = byId.get(id);
    return {
      id,
      label: spec.label,
      points: found?.points ?? [],
      latest: found?.latest ?? null,
    };
  });

  return {
    status,
    message,
    series,
    gapThresholdMs: response?.gapThresholdMs ?? 15_000,
    window: response ? { fromMs: response.fromMs, toMs: response.toMs } : null,
    aggregated: response ? !response.raw : false,
  };
}
