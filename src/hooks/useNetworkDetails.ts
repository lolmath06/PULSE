import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { MetricRef, MetricSample } from '@/types/metrics';
import {
  NETWORK_INTERFACE_COUNT,
  NETWORK_INTERFACE_UP_COUNT,
  metricRefId,
} from '@/types/wellknown';
import { getMetricCatalog, sampleMetrics } from '@/services/metrics';
import { discoverNetworkInterfaces, networkMetrics } from '@/utils/network';
import type { NetworkInterface } from '@/utils/network';

type SampleMap = ReadonlyMap<string, MetricSample>;

export interface NetworkDetailsState {
  /** `loading` only before the first response; a refresh keeps the old data. */
  readonly status: 'loading' | 'ready' | 'error';
  /** The interfaces this machine has, as discovered from the catalog. */
  readonly interfaces: readonly NetworkInterface[];
  /** Samples indexed by `key@sourceId`. */
  readonly samples: SampleMap;
  /** Set when the whole call failed, e.g. outside the Tauri runtime. */
  readonly message?: string;
  /** True while a refresh is in flight over already-displayed data. */
  readonly refreshing: boolean;
  /** Re-samples traffic, link state and Wi-Fi quality. */
  readonly refresh: () => void;
}

/**
 * Loads the network inventory and samples it on demand.
 *
 * # Two phases, for two kinds of data
 *
 * The **catalog** says which interfaces exist, what they are called and what
 * can be measured on each. None of that changes while the window is open, so
 * it is discovered once on mount — a refresh does not re-enumerate interfaces,
 * reopen a netlink socket for the inventory or re-resolve the `nl80211`
 * family.
 *
 * The **samples** are what move: throughput, packet rates, errors, drops, link
 * state and Wi-Fi quality, re-read on every refresh.
 *
 * An interface that appears *after* mount — a USB adapter plugged in, a VPN
 * connecting — is picked up on the next launch rather than the next refresh.
 * That is the backend's documented strategy and the card says nothing
 * misleading in the meantime.
 *
 * # The first sample has no baseline, and says so
 *
 * Throughput and packet rates only exist *between* two samples. On mount the
 * backend has one, so it reports those eight metrics as waiting rather than as
 * `0 B/s` — and this hook does **not** quietly fire a second request to paper
 * over it. A hidden refresh would be a polling loop with one iteration, and it
 * would hide the very thing that makes the numbers trustworthy: that a rate is
 * measured, not read.
 *
 * # Deliberately not polling
 *
 * There is exactly one sample on mount and one per click of Refresh. A test
 * advances timers and asserts no further request is made.
 */
export function useNetworkDetails(): NetworkDetailsState {
  const [interfaces, setInterfaces] = useState<readonly NetworkInterface[]>([]);
  const [samples, setSamples] = useState<SampleMap>(new Map());
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [message, setMessage] = useState<string | undefined>(undefined);
  const [refreshing, setRefreshing] = useState(false);

  // Read by `refresh`, which must not be re-created when the inventory
  // changes — a changing callback identity would re-run the mount effect and
  // turn a one-shot load into a loop.
  const interfacesRef = useRef<readonly NetworkInterface[]>([]);

  const requestFor = useCallback(
    (discovered: readonly NetworkInterface[]): MetricRef[] => [
      NETWORK_INTERFACE_COUNT,
      NETWORK_INTERFACE_UP_COUNT,
      ...networkMetrics(discovered),
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
      const discovered = discoverNetworkInterfaces(catalog);
      if (cancelled) return;

      interfacesRef.current = discovered;
      setInterfaces(discovered);

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

    void sampleMetrics(requestFor(interfacesRef.current))
      .then(applySamples)
      .catch(fail)
      .finally(() => setRefreshing(false));
  }, [requestFor, applySamples, fail]);

  return useMemo(
    () => ({ status, interfaces, samples, message, refreshing, refresh }),
    [status, interfaces, samples, message, refreshing, refresh],
  );
}
