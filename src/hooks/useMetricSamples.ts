import { useCallback, useEffect, useState } from 'react';
import type { MetricRef, MetricSample } from '@/types/metrics';
import { metricRefId } from '@/types/wellknown';
import { sampleMetrics } from '@/services/metrics';

type SampleMap = ReadonlyMap<string, MetricSample>;

export interface MetricSamplesState {
  /** `loading` only before the first response; a refresh keeps the old data. */
  readonly status: 'loading' | 'ready' | 'error';
  /** Samples indexed by `key@sourceId`. Empty until the first response. */
  readonly samples: SampleMap;
  /** Set when the whole call failed, e.g. outside the Tauri runtime. */
  readonly message?: string;
  /** True while a refresh is in flight over already-displayed data. */
  readonly refreshing: boolean;
  /** Requests a new sample. */
  readonly refresh: () => void;
}

type FetchResult =
  | { readonly ok: true; readonly samples: SampleMap }
  | { readonly ok: false; readonly message: string };

/**
 * Performs one sampling request. Pure with respect to React — it holds no
 * state and touches no setter, so both the mount effect and the Refresh button
 * can use it without duplicating logic.
 */
async function fetchSamples(metrics: readonly MetricRef[]): Promise<FetchResult> {
  try {
    const response = await sampleMetrics(metrics);
    return {
      ok: true,
      samples: new Map(response.map((sample) => [metricRefId(sample.metric), sample])),
    };
  } catch (error: unknown) {
    return { ok: false, message: error instanceof Error ? error.message : String(error) };
  }
}

/**
 * Samples a fixed set of metrics on mount, and again on demand.
 *
 * **Deliberately not polling.** PULSE has no scheduler yet, and a hidden
 * interval here would be a scheduler in disguise — one that keeps running when
 * the window is hidden and that no other part of the app could coordinate
 * with. Refreshing stays the user's decision until the real sampler arrives.
 *
 * `metrics` must be a stable reference (a module-level constant, or memoised);
 * it is an effect dependency, so a fresh array literal on every render would
 * re-sample on every render.
 */
export function useMetricSamples(metrics: readonly MetricRef[]): MetricSamplesState {
  const [samples, setSamples] = useState<SampleMap>(new Map());
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [message, setMessage] = useState<string | undefined>(undefined);
  const [refreshing, setRefreshing] = useState(false);

  const apply = useCallback((result: FetchResult) => {
    if (result.ok) {
      setSamples(result.samples);
      setStatus('ready');
      setMessage(undefined);
    } else {
      setStatus('error');
      setMessage(result.message);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;

    void fetchSamples(metrics).then((result) => {
      if (!cancelled) apply(result);
    });

    return () => {
      cancelled = true;
    };
  }, [metrics, apply]);

  const refresh = useCallback(() => {
    setRefreshing(true);
    void fetchSamples(metrics).then((result) => {
      apply(result);
      setRefreshing(false);
    });
  }, [metrics, apply]);

  return { status, samples, message, refreshing, refresh };
}
