import { describe, expect, it } from 'vitest';
import type { Availability, MetricDefinition, MetricSample } from '@/types/metrics';
import {
  NETWORK_LINK_RECEIVE_SPEED_KEY,
  NETWORK_PER_INTERFACE_KEYS,
  NETWORK_PER_WIFI_KEYS,
  NETWORK_RECEIVE_BYTES_KEY,
  NETWORK_TRAFFIC_KEYS,
  NETWORK_WIFI_SIGNAL_QUALITY_KEY,
  NETWORK_WIFI_SIGNAL_RSSI_KEY,
  metricRefId,
} from '@/types/wellknown';
import {
  discoverNetworkInterfaces,
  explainMissing,
  isPrimaryKind,
  networkMetric,
  networkMetrics,
  networkTelemetryState,
  orderInterfaces,
  partitionInterfaces,
  splitLabel,
} from '@/utils/network';
import type { NetworkInterface } from '@/utils/network';

const AVAILABLE: Availability = { status: 'available' };

const WIFI = 'network:mac-9009df3e97f2';
const ETH = 'network:mac-d843ae328ac9';
const DOCKER = 'network:mac-02427057015e';

function definition(key: string, sourceId: string, sourceLabel: string): MetricDefinition {
  return {
    metric: { key, sourceId },
    sourceLabel,
    displayName: key,
    category: 'network',
    description: '',
    unit: 'bytesPerSecond',
    valueType: 'number',
    kind: 'gauge',
    availability: AVAILABLE,
    providerId: 'linux.network',
  };
}

function entries(sourceId: string, label: string, wireless = false): MetricDefinition[] {
  const keys = wireless
    ? [...NETWORK_PER_INTERFACE_KEYS, ...NETWORK_PER_WIFI_KEYS]
    : NETWORK_PER_INTERFACE_KEYS;

  return keys.map((key) => definition(key, sourceId, label));
}

function sample(sourceId: string, key: string, value: number | Availability): MetricSample {
  const metric = networkMetric(sourceId, key);

  return typeof value === 'number'
    ? { metric, timestamp: 1, value: { type: 'number', value }, availability: AVAILABLE }
    : { metric, timestamp: 1, value: null, availability: value };
}

function sampleMap(samples: readonly MetricSample[]): ReadonlyMap<string, MetricSample> {
  return new Map(samples.map((entry) => [metricRefId(entry.metric), entry]));
}

function iface(
  sourceId: string,
  label: string,
  kind: NetworkInterface['kind'],
  wireless = false,
): NetworkInterface {
  return { sourceId, label, kind, wireless };
}

// --- labels ----------------------------------------------------------------

describe('splitLabel', () => {
  it('splits the name from the kind the backend appended', () => {
    expect(splitLabel('wlp59s0f0 · Wi-Fi')).toEqual({ name: 'wlp59s0f0', kind: 'Wi-Fi' });
    expect(splitLabel('enp58s0 · Ethernet')).toEqual({ name: 'enp58s0', kind: 'Ethernet' });
    expect(splitLabel('docker0 · Bridge')).toEqual({ name: 'docker0', kind: 'Bridge' });
  });

  it('keeps a name that itself contains the separator', () => {
    // A Windows description can contain almost anything; the *last* separator
    // is the one the backend added.
    expect(splitLabel('Intel(R) Wi-Fi 6E · AX211 · Wi-Fi')).toEqual({
      name: 'Intel(R) Wi-Fi 6E · AX211',
      kind: 'Wi-Fi',
    });
  });

  it('fails soft on a label with no kind at all', () => {
    expect(splitLabel('something odd')).toEqual({ name: 'something odd', kind: 'Network' });
    expect(splitLabel('name · NotAKind')).toEqual({ name: 'name · NotAKind', kind: 'Network' });
    expect(splitLabel('')).toEqual({ name: '', kind: 'Network' });
  });
});

describe('isPrimaryKind', () => {
  it('counts only real adapters as primary', () => {
    expect(isPrimaryKind('Ethernet')).toBe(true);
    expect(isPrimaryKind('Wi-Fi')).toBe(true);
    expect(isPrimaryKind('Bridge')).toBe(false);
    expect(isPrimaryKind('Tunnel')).toBe(false);
    expect(isPrimaryKind('Virtual')).toBe(false);
  });
});

// --- discovery -------------------------------------------------------------

describe('discoverNetworkInterfaces', () => {
  it('finds nothing in an empty catalog', () => {
    expect(discoverNetworkInterfaces([])).toEqual([]);
  });

  it('finds one interface from its per-interface metrics', () => {
    const found = discoverNetworkInterfaces(entries(ETH, 'enp58s0 · Ethernet'));

    expect(found).toEqual([{ sourceId: ETH, label: 'enp58s0', kind: 'Ethernet', wireless: false }]);
  });

  it('marks an interface wireless when the catalog declares Wi-Fi metrics for it', () => {
    const found = discoverNetworkInterfaces(entries(WIFI, 'wlp59s0f0 · Wi-Fi', true));

    expect(found[0]?.wireless).toBe(true);
    expect(found[0]?.kind).toBe('Wi-Fi');
  });

  it('does not mark a wired interface wireless', () => {
    const found = discoverNetworkInterfaces(entries(ETH, 'enp58s0 · Ethernet'));

    expect(found[0]?.wireless).toBe(false);
  });

  it('lists an interface whose telemetry is entirely unavailable', () => {
    // A recognised adapter with no reachable counters must be visible, not
    // hidden.
    const catalog = entries(ETH, 'enp58s0 · Ethernet').map((definition) => ({
      ...definition,
      availability: { status: 'notDetected', reason: 'no counters' } as Availability,
    }));

    expect(discoverNetworkInterfaces(catalog)).toHaveLength(1);
  });

  it('never mistakes the machine-wide source for an interface', () => {
    const catalog = [
      definition('network.interface.count', 'network:system', 'Network'),
      definition('network.interface.up_count', 'network:system', 'Network'),
      ...entries(ETH, 'enp58s0 · Ethernet'),
    ];

    expect(discoverNetworkInterfaces(catalog).map((entry) => entry.sourceId)).toEqual([ETH]);
  });

  it('ignores metrics from other families', () => {
    const catalog = [
      definition('cpu.usage.total', 'cpu:system', 'Processor'),
      definition('storage.capacity.total', 'storage:wwid-abc', 'Disk'),
    ];

    expect(discoverNetworkInterfaces(catalog)).toEqual([]);
  });

  it('finds a mix of many interfaces', () => {
    const catalog = [
      ...entries(WIFI, 'wlp59s0f0 · Wi-Fi', true),
      ...entries(ETH, 'enp58s0 · Ethernet'),
      ...entries(DOCKER, 'docker0 · Bridge'),
    ];

    const found = discoverNetworkInterfaces(catalog);

    expect(found).toHaveLength(3);
    expect(found.filter((entry) => entry.wireless)).toHaveLength(1);
  });
});

// --- requests --------------------------------------------------------------

describe('networkMetrics', () => {
  it('requests nothing for an empty inventory', () => {
    expect(networkMetrics([])).toEqual([]);
  });

  it('requests the Wi-Fi keys only for a wireless interface', () => {
    const request = networkMetrics([
      iface(WIFI, 'wlp59s0f0', 'Wi-Fi', true),
      iface(ETH, 'enp58s0', 'Ethernet'),
    ]);

    expect(request).toHaveLength(
      NETWORK_PER_INTERFACE_KEYS.length * 2 + NETWORK_PER_WIFI_KEYS.length,
    );
    expect(new Set(request.map(metricRefId)).size).toBe(request.length);

    const wifiKeys = request.filter((metric) => NETWORK_PER_WIFI_KEYS.includes(metric.key));
    expect(wifiKeys.every((metric) => metric.sourceId === WIFI)).toBe(true);
  });
});

// --- ordering --------------------------------------------------------------

describe('orderInterfaces', () => {
  const inventory = [
    iface(DOCKER, 'docker0', 'Bridge'),
    iface(ETH, 'enp58s0', 'Ethernet'),
    iface('network:if-wg0', 'wg0', 'Tunnel'),
    iface(WIFI, 'wlp59s0f0', 'Wi-Fi', true),
  ];

  it('puts connected hardware first, then connected tunnels', () => {
    const connected = (sourceId: string) => sourceId === WIFI || sourceId === 'network:if-wg0';

    expect(orderInterfaces(inventory, connected).map((entry) => entry.label)).toEqual([
      'wlp59s0f0',
      'wg0',
      'enp58s0',
      'docker0',
    ]);
  });

  it('keeps a disconnected adapter visible, below the connected ones', () => {
    // An unplugged Ethernet port is still worth seeing.
    const connected = (sourceId: string) => sourceId === WIFI;
    const ordered = orderInterfaces(inventory, connected);

    expect(ordered.map((entry) => entry.label)).toContain('enp58s0');
    expect(ordered.findIndex((entry) => entry.label === 'wlp59s0f0')).toBeLessThan(
      ordered.findIndex((entry) => entry.label === 'enp58s0'),
    );
  });

  it('is deterministic regardless of the order it was handed', () => {
    const connected = () => false;
    const forward = orderInterfaces(inventory, connected).map((entry) => entry.sourceId);
    const backward = orderInterfaces([...inventory].reverse(), connected).map(
      (entry) => entry.sourceId,
    );

    expect(forward).toEqual(backward);
  });

  it('does not mutate the list it was given', () => {
    const original = [...inventory];
    orderInterfaces(inventory, () => true);

    expect(inventory).toEqual(original);
  });
});

describe('partitionInterfaces', () => {
  it('shows hardware and tunnels, and collapses the rest', () => {
    const { primary, other } = partitionInterfaces([
      iface(WIFI, 'wlp59s0f0', 'Wi-Fi', true),
      iface(ETH, 'enp58s0', 'Ethernet'),
      iface('network:if-wg0', 'wg0', 'Tunnel'),
      iface(DOCKER, 'docker0', 'Bridge'),
      iface('network:mac-4a45a3b9336f', 'vethe0ffe82', 'Virtual'),
    ]);

    expect(primary.map((entry) => entry.label)).toEqual(['wlp59s0f0', 'enp58s0', 'wg0']);
    expect(other.map((entry) => entry.label)).toEqual(['docker0', 'vethe0ffe82']);
  });

  it('keeps thirty virtual interfaces out of the way without dropping any', () => {
    // The container case. Collapsed, never hidden.
    const many = Array.from({ length: 30 }, (_, index) =>
      iface(`network:if-veth${index}`, `veth${index}`, 'Virtual'),
    );

    const { primary, other } = partitionInterfaces([iface(WIFI, 'wlan0', 'Wi-Fi', true), ...many]);

    expect(primary).toHaveLength(1);
    expect(other).toHaveLength(30);
  });

  it('handles an empty inventory', () => {
    expect(partitionInterfaces([])).toEqual({ primary: [], other: [] });
  });
});

// --- telemetry state -------------------------------------------------------

describe('networkTelemetryState', () => {
  it('reports traffic available on an active interface', () => {
    const samples = sampleMap([
      ...NETWORK_TRAFFIC_KEYS.map((key) => sample(WIFI, key, 1)),
      sample(WIFI, NETWORK_LINK_RECEIVE_SPEED_KEY, 1_000_000_000),
    ]);

    const state = networkTelemetryState(samples, WIFI);

    expect(state.trafficAvailable).toBe(true);
    expect(state.awaitingBaseline).toBe(false);
    expect(state.linkSpeedAvailable).toBe(true);
  });

  it('recognises a first sample with no baseline yet', () => {
    // Every traffic metric temporarily unavailable at once is exactly the
    // shape a fresh launch produces, and it is not a failure.
    const waiting: Availability = {
      status: 'temporarilyUnavailable',
      reason: 'network traffic is measured between two samples; waiting for the next one',
    };
    const samples = sampleMap(NETWORK_TRAFFIC_KEYS.map((key) => sample(WIFI, key, waiting)));

    const state = networkTelemetryState(samples, WIFI);

    expect(state.trafficAvailable).toBe(false);
    expect(state.awaitingBaseline).toBe(true);
  });

  it('does not call an interface with no counters a missing baseline', () => {
    // An interface the kernel keeps no statistics for is a different
    // situation, and the "refresh to measure it" hint would be a lie.
    const absent: Availability = { status: 'notDetected', reason: 'no statistics block' };
    const samples = sampleMap(NETWORK_TRAFFIC_KEYS.map((key) => sample(WIFI, key, absent)));

    const state = networkTelemetryState(samples, WIFI);

    expect(state.trafficAvailable).toBe(false);
    expect(state.awaitingBaseline).toBe(false);
  });

  it('counts an idle interface as measured, not as waiting', () => {
    // Zero over a real interval is a measurement.
    const samples = sampleMap(NETWORK_TRAFFIC_KEYS.map((key) => sample(WIFI, key, 0)));

    const state = networkTelemetryState(samples, WIFI);

    expect(state.trafficAvailable).toBe(true);
    expect(state.awaitingBaseline).toBe(false);
  });

  it('reports nothing at all when the samples never arrived', () => {
    const state = networkTelemetryState(new Map(), WIFI);

    expect(state.trafficAvailable).toBe(false);
    expect(state.awaitingBaseline).toBe(false);
    expect(state.linkSpeedAvailable).toBe(false);
  });
});

// --- explaining absence ----------------------------------------------------

describe('explainMissing', () => {
  it('returns nothing when at least one metric produced a value', () => {
    // Fedora's case: a real RSSI and no quality percentage is still a working
    // Wi-Fi link.
    const samples = sampleMap([
      sample(WIFI, NETWORK_WIFI_SIGNAL_RSSI_KEY, -68),
      sample(WIFI, NETWORK_WIFI_SIGNAL_QUALITY_KEY, {
        status: 'unsupported',
        reason: 'this platform reports no quality percentage',
      }),
    ]);

    expect(explainMissing(samples, WIFI, NETWORK_PER_WIFI_KEYS)).toBeNull();
  });

  it('surfaces the most actionable reason when none produced a value', () => {
    const samples = sampleMap([
      sample(WIFI, NETWORK_WIFI_SIGNAL_RSSI_KEY, {
        status: 'temporarilyUnavailable',
        reason: 'not associated with a network',
      }),
      sample(WIFI, NETWORK_WIFI_SIGNAL_QUALITY_KEY, {
        status: 'permissionDenied',
        reason: 'the kernel refused the station dump',
      }),
    ]);

    expect(explainMissing(samples, WIFI, NETWORK_PER_WIFI_KEYS)).toBe(
      'the kernel refused the station dump',
    );
  });

  it('reads the message out of a structured provider error', () => {
    const samples = sampleMap([
      sample(WIFI, NETWORK_RECEIVE_BYTES_KEY, {
        status: 'providerError',
        error: { code: 'parse', message: 'malformed netlink attribute', recoverable: false },
      }),
    ]);

    expect(explainMissing(samples, WIFI, [NETWORK_RECEIVE_BYTES_KEY])).toBe(
      'malformed netlink attribute',
    );
  });

  it('returns nothing when the samples never arrived', () => {
    expect(explainMissing(new Map(), WIFI, NETWORK_PER_WIFI_KEYS)).toBeNull();
  });
});
