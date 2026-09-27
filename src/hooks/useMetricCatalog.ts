import { useEffect, useState } from 'react';
import type { MetricDefinition } from '@/types/metrics';
import { getMetricCatalog } from '@/services/metrics';

export interface MetricCatalogState {
  readonly status: 'loading' | 'ready' | 'error';
  readonly catalog: readonly MetricDefinition[];
  readonly message?: string;
}

let shared: Promise<MetricDefinition[]> | null = null;

/**
 * The metric catalog, fetched **once** for every history panel.
 *
 * The catalog is built at startup and does not change while PULSE runs, so
 * seven panels asking for it share a single request. A failed request is not
 * cached, so a later mount can try again.
 */
export function useMetricCatalog(): MetricCatalogState {
  const [state, setState] = useState<MetricCatalogState>({ status: 'loading', catalog: [] });

  useEffect(() => {
    let cancelled = false;
    shared ??= getMetricCatalog();
    const request = shared;
    request
      .then((catalog) => {
        if (!cancelled) setState({ status: 'ready', catalog });
      })
      .catch((error: unknown) => {
        if (shared === request) shared = null;
        if (!cancelled) {
          setState({
            status: 'error',
            catalog: [],
            message: error instanceof Error ? error.message : String(error),
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return state;
}

/** Forgets the shared catalog. For tests. */
export function resetMetricCatalogForTesting() {
  shared = null;
}
