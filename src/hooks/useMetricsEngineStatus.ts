import { useEffect, useState } from 'react';
import type { EngineStatus } from '@/types/metrics';
import { getMetricsEngineStatus } from '@/services/metrics';

export type MetricsEngineStatusState =
  | { status: 'loading' }
  | { status: 'ready'; engine: EngineStatus }
  | { status: 'error'; message: string };

/** Loads the metrics engine status once, on mount. */
export function useMetricsEngineStatus(): MetricsEngineStatusState {
  const [state, setState] = useState<MetricsEngineStatusState>({ status: 'loading' });

  useEffect(() => {
    let cancelled = false;

    getMetricsEngineStatus()
      .then((engine) => {
        if (!cancelled) setState({ status: 'ready', engine });
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setState({
          status: 'error',
          message: error instanceof Error ? error.message : String(error),
        });
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return state;
}
