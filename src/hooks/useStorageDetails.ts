import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { MetricRef, MetricSample } from '@/types/metrics';
import { STORAGE_DEVICE_COUNT, STORAGE_VOLUME_COUNT, metricRefId } from '@/types/wellknown';
import { getMetricCatalog, sampleMetrics } from '@/services/metrics';
import { discoverStorageDevices, discoverStorageVolumes, storageMetrics } from '@/utils/storage';
import type { StorageDevice, StorageVolume } from '@/utils/storage';

type SampleMap = ReadonlyMap<string, MetricSample>;

export interface StorageDetailsState {
  /** `loading` only before the first response; a refresh keeps the old data. */
  readonly status: 'loading' | 'ready' | 'error';
  /** The physical devices this machine has, in catalog order. */
  readonly devices: readonly StorageDevice[];
  /** The mounted filesystems, ordered by the path a user recognises. */
  readonly volumes: readonly StorageVolume[];
  /** Samples indexed by `key@sourceId`. */
  readonly samples: SampleMap;
  /** Set when the whole call failed, e.g. outside the Tauri runtime. */
  readonly message?: string;
  /** True while a refresh is in flight over already-displayed data. */
  readonly refreshing: boolean;
  /** Re-samples activity, volume usage and health. */
  readonly refresh: () => void;
}

/**
 * Loads the storage inventory and samples it on demand.
 *
 * # Two phases, for two kinds of data
 *
 * The **catalog** says which disks and filesystems exist, what they are called
 * and what can be measured on each. None of that changes while the window is
 * open, so it is discovered once on mount — a refresh does not re-enumerate
 * `/sys/class/block`, re-read the mount table or reopen any device.
 *
 * The **samples** are what move: throughput, IOPS, latency, volume usage and
 * health, re-read on every refresh.
 *
 * # The first sample has no baseline, and says so
 *
 * Throughput, IOPS and latency only exist *between* two samples. On mount the
 * backend has one, so it reports those six metrics as waiting rather than as
 * `0 B/s` — and this hook does **not** quietly fire a second request to paper
 * over it. A hidden refresh would be a polling loop with one iteration, and it
 * would hide from the user the very thing that makes the numbers trustworthy:
 * that a rate is measured, not read.
 *
 * # Deliberately not polling
 *
 * There is exactly one sample on mount and one per click of Refresh. A test
 * advances timers and asserts no further request is made.
 */
export function useStorageDetails(): StorageDetailsState {
  const [devices, setDevices] = useState<readonly StorageDevice[]>([]);
  const [volumes, setVolumes] = useState<readonly StorageVolume[]>([]);
  const [samples, setSamples] = useState<SampleMap>(new Map());
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [message, setMessage] = useState<string | undefined>(undefined);
  const [refreshing, setRefreshing] = useState(false);

  // Read by `refresh`, which must not be re-created when the inventory
  // changes — a changing callback identity would re-run the mount effect and
  // turn a one-shot load into a loop.
  const inventoryRef = useRef<{
    devices: readonly StorageDevice[];
    volumes: readonly StorageVolume[];
  }>({ devices: [], volumes: [] });

  const requestFor = useCallback(
    (
      discoveredDevices: readonly StorageDevice[],
      discoveredVolumes: readonly StorageVolume[],
    ): MetricRef[] => [
      STORAGE_DEVICE_COUNT,
      STORAGE_VOLUME_COUNT,
      ...storageMetrics(discoveredDevices, discoveredVolumes),
    ],
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
      const discoveredDevices = discoverStorageDevices(catalog);
      const discoveredVolumes = discoverStorageVolumes(catalog);
      if (cancelled) return;

      inventoryRef.current = { devices: discoveredDevices, volumes: discoveredVolumes };
      setDevices(discoveredDevices);
      setVolumes(discoveredVolumes);

      const response = await sampleMetrics(requestFor(discoveredDevices, discoveredVolumes));
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

    const { devices: known, volumes: mounted } = inventoryRef.current;

    void sampleMetrics(requestFor(known, mounted))
      .then(applySamples)
      .catch(fail)
      .finally(() => setRefreshing(false));
  }, [requestFor, applySamples, fail]);

  return useMemo(
    () => ({ status, devices, volumes, samples, message, refreshing, refresh }),
    [status, devices, volumes, samples, message, refreshing, refresh],
  );
}
