import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { MetricRef, MetricSample } from '@/types/metrics';
import { CPU_TOPOLOGY_METRICS, metricRefId } from '@/types/wellknown';
import { getMetricCatalog, sampleMetrics } from '@/services/metrics';
import {
  cpuPackageMetrics,
  discoverCpuPackages,
  discoverLogicalProcessors,
  logicalProcessorMetrics,
} from '@/utils/cpu';
import type { CpuPackage, LogicalProcessor } from '@/utils/cpu';

type SampleMap = ReadonlyMap<string, MetricSample>;

export interface CpuDetailsState {
  /** `loading` only before the first response; a refresh keeps the old data. */
  readonly status: 'loading' | 'ready' | 'error';
  /** The logical processors this machine has, in numeric order. */
  readonly processors: readonly LogicalProcessor[];
  /** The processor packages this machine has, in numeric order. */
  readonly packages: readonly CpuPackage[];
  /** Samples indexed by `key@sourceId`. */
  readonly samples: SampleMap;
  /** Set when the whole call failed, e.g. outside the Tauri runtime. */
  readonly message?: string;
  /** True while a refresh is in flight over already-displayed data. */
  readonly refreshing: boolean;
  /** Takes a fresh sample of usage and frequency. */
  readonly refresh: () => void;
}

/**
 * Loads the CPU layout and samples it on demand.
 *
 * # Two phases, for two kinds of data
 *
 * The **catalog** says which logical processors and packages exist. That is discovered once
 * on mount: processors do not appear and vanish while the window is open, and
 * re-deriving the table's shape on every refresh would make rows jump around
 * under the pointer for no gain.
 *
 * The **samples** are what move, and they are re-read on every refresh.
 *
 * # Deliberately not polling
 *
 * PULSE has no scheduler yet, and a hidden interval here would be a scheduler
 * in disguise — one that keeps running when the window is hidden and that no
 * other part of the app could coordinate with. There is exactly one sample on
 * mount and one per click of Refresh; a test asserts that no further request
 * is made while time passes.
 */
export function useCpuDetails(): CpuDetailsState {
  const [processors, setProcessors] = useState<readonly LogicalProcessor[]>([]);
  const [packages, setPackages] = useState<readonly CpuPackage[]>([]);
  const [samples, setSamples] = useState<SampleMap>(new Map());
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [message, setMessage] = useState<string | undefined>(undefined);
  const [refreshing, setRefreshing] = useState(false);

  // Read by `refresh`, which must not be re-created whenever the processor
  // list changes — a changing callback identity would re-run the mount effect
  // and turn a one-shot load into a loop.
  const processorsRef = useRef<readonly LogicalProcessor[]>([]);
  const packagesRef = useRef<readonly CpuPackage[]>([]);

  const requestFor = useCallback(
    (discovered: readonly LogicalProcessor[], sockets: readonly CpuPackage[]): MetricRef[] => {
      return [
        ...CPU_TOPOLOGY_METRICS,
        ...cpuPackageMetrics(sockets),
        ...logicalProcessorMetrics(discovered),
      ];
    },
    [],
  );

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
      const discovered = discoverLogicalProcessors(catalog);
      const sockets = discoverCpuPackages(catalog);
      if (cancelled) return;

      processorsRef.current = discovered;
      packagesRef.current = sockets;
      setProcessors(discovered);
      setPackages(sockets);

      const response = await sampleMetrics(requestFor(discovered, sockets));
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

    void sampleMetrics(requestFor(processorsRef.current, packagesRef.current))
      .then(applySamples)
      .catch(fail)
      .finally(() => setRefreshing(false));
  }, [requestFor, applySamples, fail]);

  return useMemo(
    () => ({ status, processors, packages, samples, message, refreshing, refresh }),
    [status, processors, packages, samples, message, refreshing, refresh],
  );
}
