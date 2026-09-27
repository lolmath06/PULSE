import { useEffect, useState } from 'react';
import type { MetricRef } from '@/types/metrics';
import type { HistoryRange, HistoryResponse } from '@/types/history';
import { getMetricHistory, onHistorySample } from '@/services/history';

export interface MetricHistoryState {
  /** `loading` only until the first answer; later reloads keep the data. */
  readonly status: 'loading' | 'ready' | 'unavailable';
  readonly response: HistoryResponse | null;
  /** Why history is unavailable, when it is. */
  readonly reason?: string;
}

/**
 * The history of `metrics` over `range`, kept current by the backend.
 *
 * # No timer
 *
 * This hook never polls. It loads once, then reloads when the backend's one
 * scheduler announces a new batch (`history-sample-recorded`) — so every chart
 * on screen moves in step with what was actually recorded, and a hidden or
 * off-screen chart (`enabled: false`) costs nothing at all.
 *
 * A batch that arrives while a load is in flight is coalesced into one
 * follow-up load, never queued once per event.
 */
export function useMetricHistory(
  metrics: readonly MetricRef[],
  range: HistoryRange,
  enabled = true,
): MetricHistoryState {
  const [state, setState] = useState<MetricHistoryState>({ status: 'loading', response: null });
  // The request is keyed on the references' *content*, so a caller passing a
  // fresh array with the same metrics does not reload.
  const key = JSON.stringify(metrics.map((metric) => [metric.key, metric.sourceId]));

  useEffect(() => {
    const requested: MetricRef[] = (JSON.parse(key) as [string, string][]).map(
      ([metricKey, sourceId]) => ({ key: metricKey, sourceId }),
    );
    if (!enabled || requested.length === 0) return;

    let cancelled = false;
    let inFlight = false;
    let pending = false;

    const load = () => {
      if (inFlight) {
        pending = true;
        return;
      }
      inFlight = true;
      getMetricHistory(requested, range)
        .then((result) => {
          if (cancelled) return;
          if (result.status === 'ok') {
            setState({ status: 'ready', response: result });
          } else {
            setState({ status: 'unavailable', response: null, reason: result.reason });
          }
        })
        .catch((error: unknown) => {
          if (!cancelled) {
            setState({
              status: 'unavailable',
              response: null,
              reason: error instanceof Error ? error.message : String(error),
            });
          }
        })
        .finally(() => {
          inFlight = false;
          if (pending && !cancelled) {
            pending = false;
            load();
          }
        });
    };

    load();
    const unsubscribe = onHistorySample(load);
    return () => {
      cancelled = true;
      unsubscribe();
    };
  }, [key, range, enabled]);

  return state;
}
