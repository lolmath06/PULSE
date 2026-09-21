import type { Availability, MetricDefinition, MetricRef, MetricSample } from '@/types/metrics';
import {
  NETWORK_LINK_RECEIVE_SPEED_KEY,
  NETWORK_PER_INTERFACE_KEYS,
  NETWORK_PER_WIFI_KEYS,
  NETWORK_TRAFFIC_KEYS,
  isNetworkInterfaceSource,
  metricRefId,
} from '@/types/wellknown';

/**
 * Deriving the network inventory from the metric catalog.
 *
 * **Nothing here knows how many interfaces a machine has, what they are, or
 * which of them is the one the user cares about.** The backend publishes
 * eleven metrics per interface and four more per Wi-Fi radio, and this module
 * reads the catalog back to discover which exist. A laptop with one Wi-Fi
 * adapter, a workstation with two NICs and a VPN, and a developer machine with
 * seven Docker bridges all go through the same code.
 *
 * Equally, nothing here parses a `SourceId`. Whether it encodes a permanent
 * MAC, a Windows interface GUID or an interface name is the backend's
 * business — the interface treats it as an opaque, stable key.
 *
 * Pure and free of React, so the grouping and ordering rules are tested
 * directly rather than through a rendered component.
 */

/** How an interface is grouped and labelled in the card. */
export type NetworkInterfaceKind =
  'Ethernet' | 'Wi-Fi' | 'Bridge' | 'Tunnel' | 'Virtual' | 'Network';

/** One interface discovered in the catalog. */
export interface NetworkInterface {
  /** Its stable source identifier. Opaque to the interface. */
  readonly sourceId: string;
  /** The name the backend gave it, e.g. `wlp59s0f0`. */
  readonly label: string;
  /** The kind the backend classified it as, from the source label's suffix. */
  readonly kind: NetworkInterfaceKind;
  /** True when the catalog declares Wi-Fi metrics for it. */
  readonly wireless: boolean;
}

/**
 * The separator the backend puts between an interface's name and its kind in
 * `sourceLabel` — `wlp59s0f0 · Wi-Fi`.
 *
 * Splitting on it is the one structural assumption the interface makes about a
 * label, it is one the backend builds in a single place and tests, and it
 * fails soft: a label without the separator keeps its whole text as the name
 * and falls back to the neutral kind.
 */
const LABEL_SEPARATOR = ' · ';

const KINDS: readonly NetworkInterfaceKind[] = [
  'Ethernet',
  'Wi-Fi',
  'Bridge',
  'Tunnel',
  'Virtual',
  'Network',
];

/** Splits a backend source label into its name and its kind. */
export function splitLabel(sourceLabel: string): {
  readonly name: string;
  readonly kind: NetworkInterfaceKind;
} {
  const index = sourceLabel.lastIndexOf(LABEL_SEPARATOR);
  if (index < 0) return { name: sourceLabel, kind: 'Network' };

  const name = sourceLabel.slice(0, index);
  const suffix = sourceLabel.slice(index + LABEL_SEPARATOR.length);
  const kind = KINDS.find((candidate) => candidate === suffix);

  return kind ? { name, kind } : { name: sourceLabel, kind: 'Network' };
}

/** Whether this kind is hardware a user thinks of as "my connection". */
export function isPrimaryKind(kind: NetworkInterfaceKind): boolean {
  return kind === 'Ethernet' || kind === 'Wi-Fi';
}

/**
 * Finds every interface the catalog describes.
 *
 * An interface is listed as soon as it appears in *any* per-interface metric,
 * so an adapter whose counters are entirely unavailable still gets a row —
 * which is the point: a recognised NIC with no reachable telemetry must be
 * visible, not hidden.
 */
export function discoverNetworkInterfaces(
  catalog: readonly MetricDefinition[],
): NetworkInterface[] {
  const found = new Map<string, { name: string; kind: NetworkInterfaceKind }>();
  const wireless = new Set<string>();

  for (const definition of catalog) {
    const { key, sourceId } = definition.metric;
    if (!isNetworkInterfaceSource(sourceId)) continue;

    if (NETWORK_PER_WIFI_KEYS.includes(key)) {
      wireless.add(sourceId);
      continue;
    }
    if (!NETWORK_PER_INTERFACE_KEYS.includes(key) || found.has(sourceId)) continue;

    found.set(sourceId, splitLabel(definition.sourceLabel));
  }

  return [...found.entries()].map(([sourceId, { name, kind }]) => ({
    sourceId,
    label: name,
    kind,
    wireless: wireless.has(sourceId),
  }));
}

/**
 * Orders interfaces the way a user scans them.
 *
 * Connected hardware first, because that is the one carrying their traffic;
 * then connected tunnels, which are the next thing they look for; then
 * disconnected hardware, which is still worth seeing; then everything virtual.
 * Within each group, by label, so the order is deterministic across refreshes
 * rather than following whatever order the OS enumerated.
 */
export function orderInterfaces(
  interfaces: readonly NetworkInterface[],
  connected: (sourceId: string) => boolean,
): NetworkInterface[] {
  const rank = (entry: NetworkInterface): number => {
    const up = connected(entry.sourceId);

    if (isPrimaryKind(entry.kind)) return up ? 0 : 2;
    if (entry.kind === 'Tunnel') return up ? 1 : 3;
    return 4;
  };

  return [...interfaces].sort(
    (left, right) =>
      rank(left) - rank(right) ||
      left.label.localeCompare(right.label) ||
      left.sourceId.localeCompare(right.sourceId),
  );
}

/**
 * Splits the ordered list into the interfaces shown by default and the rest.
 *
 * A machine running containers can have thirty `veth` pairs and two real
 * adapters. Showing thirty full cards by default would bury the two that
 * matter, so the virtual ones go behind a *Show all* control — **collapsed,
 * never hidden**, and never removed from the catalog.
 */
export function partitionInterfaces(interfaces: readonly NetworkInterface[]): {
  readonly primary: NetworkInterface[];
  readonly other: NetworkInterface[];
} {
  const primary: NetworkInterface[] = [];
  const other: NetworkInterface[] = [];

  for (const entry of interfaces) {
    if (isPrimaryKind(entry.kind) || entry.kind === 'Tunnel') {
      primary.push(entry);
    } else {
      other.push(entry);
    }
  }

  return { primary, other };
}

/**
 * The metrics to request for a set of interfaces.
 *
 * Eleven references per interface, plus four more for each Wi-Fi radio.
 */
export function networkMetrics(interfaces: readonly NetworkInterface[]): MetricRef[] {
  return interfaces.flatMap((entry) => [
    ...NETWORK_PER_INTERFACE_KEYS.map((key) => ({ key, sourceId: entry.sourceId })),
    ...(entry.wireless
      ? NETWORK_PER_WIFI_KEYS.map((key) => ({ key, sourceId: entry.sourceId }))
      : []),
  ]);
}

/** Builds a reference for one interface. */
export function networkMetric(sourceId: string, key: string): MetricRef {
  return { key, sourceId };
}

// --- explaining an interface with no traffic -------------------------------

/**
 * What one interface's telemetry is currently doing.
 *
 * The distinction exists because the two halves fail for different reasons and
 * need different sentences. An adapter waiting for a second sample is not an
 * adapter with a problem, and an unplugged cable is not a broken driver.
 */
export interface NetworkTelemetryState {
  /** True as soon as one traffic metric produced a value. */
  readonly trafficAvailable: boolean;
  /**
   * True when the traffic metrics are absent only because no baseline exists
   * yet — the state a fresh launch is in, which resolves on the next refresh
   * and is not a failure.
   */
  readonly awaitingBaseline: boolean;
  /** True when the interface reports a link speed. */
  readonly linkSpeedAvailable: boolean;
}

/** Summarises one interface's telemetry from the samples that came back. */
export function networkTelemetryState(
  samples: ReadonlyMap<string, MetricSample>,
  sourceId: string,
): NetworkTelemetryState {
  const sampleOf = (key: string): MetricSample | undefined =>
    samples.get(metricRefId(networkMetric(sourceId, key)));

  const hasValue = (key: string): boolean => sampleOf(key)?.value?.type === 'number';

  const trafficAvailable = NETWORK_TRAFFIC_KEYS.some(hasValue);

  // "Waiting for a second sample" is `temporarilyUnavailable` on every traffic
  // metric at once, which is exactly the shape a fresh launch produces.
  const awaitingBaseline =
    !trafficAvailable &&
    NETWORK_TRAFFIC_KEYS.every(
      (key) => sampleOf(key)?.availability.status === 'temporarilyUnavailable',
    );

  return {
    trafficAvailable,
    awaitingBaseline,
    linkSpeedAvailable: hasValue(NETWORK_LINK_RECEIVE_SPEED_KEY),
  };
}

/**
 * How specific an availability reason is, for choosing which to surface.
 *
 * The one worth showing is the one the user can act on, so a permission
 * problem outranks an unsupported interface, and a transient state ranks last
 * because it will probably be gone next refresh.
 */
function reasonRank(availability: Availability): number {
  switch (availability.status) {
    case 'permissionDenied':
      return 0;
    case 'unsupported':
      return 1;
    case 'notDetected':
      return 2;
    case 'providerError':
      return 3;
    case 'temporarilyUnavailable':
      return 4;
    default:
      return 5;
  }
}

function reasonOf(availability: Availability): string | null {
  if (availability.status === 'available') return null;
  if (availability.status === 'providerError') return availability.error.message;

  return availability.reason;
}

/**
 * The backend's own explanation for a group of missing metrics.
 *
 * Never invented here: it is the reason the provider attached to the sample.
 * `null` when at least one of them produced a value.
 */
export function explainMissing(
  samples: ReadonlyMap<string, MetricSample>,
  sourceId: string,
  keys: readonly string[],
): string | null {
  let best: { rank: number; reason: string } | null = null;

  for (const key of keys) {
    const sample = samples.get(metricRefId(networkMetric(sourceId, key)));
    if (!sample) continue;
    if (sample.value?.type === 'number') return null;

    const reason = reasonOf(sample.availability);
    if (reason === null) continue;

    const rank = reasonRank(sample.availability);
    if (best === null || rank < best.rank) best = { rank, reason };
  }

  return best?.reason ?? null;
}
