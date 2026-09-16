import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { MetricRef, MetricSample } from '@/types/metrics';
import { GPU_COUNT, metricRefId } from '@/types/wellknown';
import { getMetricCatalog, sampleMetrics } from '@/services/metrics';
import { discoverGpus, gpuMetrics } from '@/utils/gpu';
import type { GpuDevice } from '@/utils/gpu';

type SampleMap = ReadonlyMap<string, MetricSample>;

export interface GpuDetailsState {
  /** `loading` only before the first response; a refresh keeps the old data. */
  readonly status: 'loading' | 'ready' | 'error';
  /** The GPUs this machine has, in catalog order. */
  readonly devices: readonly GpuDevice[];
  /** Samples indexed by `key@sourceId`. */
  readonly samples: SampleMap;
  /** Set when the whole call failed, e.g. outside the Tauri runtime. */
  readonly message?: string;
  /** True while a refresh is in flight over already-displayed data. */
  readonly refreshing: boolean;
  /** Re-samples usage, VRAM and clocks. */
  readonly refresh: () => void;
}

/**
 * Loads the GPU inventory and samples it on demand.
 *
 * # Two phases, for two kinds of data
 *
 * The **catalog** says which GPUs exist, what they are called and what can be
 * measured on them. None of that changes while the window is open, so it is
 * discovered once on mount — a refresh does not re-enumerate PCI, reload NVML
 * or re-resolve any symbol.
 *
 * The **samples** are what move: utilisation, VRAM and clocks, re-read on every
 * refresh.
 *
 * # Deliberately not polling
 *
 * PULSE has no scheduler yet, and a hidden interval here would be one in
 * disguise. There is exactly one sample on mount and one per click of Refresh;
 * a test advances timers and asserts no further request is made.
 */
export function useGpuDetails(): GpuDetailsState {
  const [devices, setDevices] = useState<readonly GpuDevice[]>([]);
  const [samples, setSamples] = useState<SampleMap>(new Map());
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [message, setMessage] = useState<string | undefined>(undefined);
  const [refreshing, setRefreshing] = useState(false);

  // Read by `refresh`, which must not be re-created when the device list
  // changes — a changing callback identity would re-run the mount effect and
  // turn a one-shot load into a loop.
  const devicesRef = useRef<readonly GpuDevice[]>([]);

  const requestFor = useCallback((discovered: readonly GpuDevice[]): MetricRef[] => {
    return [GPU_COUNT, ...gpuMetrics(discovered)];
  }, []);

  const applySamples = useCallback((response: readonly MetricSample[]) => {
    setSamples(new Map(response.map((sample) => [metricRefId(sample.metric), sample])));
    setStatus('ready');
    setMessage(undefined);
  }, []);

  const fail = useCallback((error: unknown) => {
    setStatus('error');
    setMessage(error instanceof Error ? error.message : String(error));
  }, []);

  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      const catalog = await getMetricCatalog();
      const discovered = discoverGpus(catalog);
      if (cancelled) return;

      devicesRef.current = discovered;
      setDevices(discovered);

      const response = await sampleMetrics(requestFor(discovered));
      if (cancelled) return;

      applySamples(response);
    };

    void load().catch((error: unknown) => {
      if (!cancelled) fail(error);
    });

    return () => {
      cancelled = true;
    };
  }, [requestFor, applySamples, fail]);

  const refresh = useCallback(() => {
    setRefreshing(true);

    void sampleMetrics(requestFor(devicesRef.current))
      .then(applySamples)
      .catch(fail)
      .finally(() => setRefreshing(false));
  }, [requestFor, applySamples, fail]);

  return useMemo(
    () => ({ status, devices, samples, message, refreshing, refresh }),
    [status, devices, samples, message, refreshing, refresh],
  );
}
