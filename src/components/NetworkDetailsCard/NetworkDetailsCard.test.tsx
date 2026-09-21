import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Availability, MetricDefinition, MetricRef, MetricSample } from '@/types/metrics';
import {
  NETWORK_INTERFACE_COUNT_KEY,
  NETWORK_INTERFACE_UP_COUNT_KEY,
  NETWORK_LINK_RECEIVE_SPEED_KEY,
  NETWORK_LINK_TRANSMIT_SPEED_KEY,
  NETWORK_MTU_KEY,
  NETWORK_PER_INTERFACE_KEYS,
  NETWORK_PER_WIFI_KEYS,
  NETWORK_RECEIVE_BYTES_KEY,
  NETWORK_RECEIVE_DROPPED_KEY,
  NETWORK_RECEIVE_ERRORS_KEY,
  NETWORK_RECEIVE_PACKETS_KEY,
  NETWORK_TRAFFIC_KEYS,
  NETWORK_TRANSMIT_BYTES_KEY,
  NETWORK_TRANSMIT_DROPPED_KEY,
  NETWORK_TRANSMIT_ERRORS_KEY,
  NETWORK_TRANSMIT_PACKETS_KEY,
  NETWORK_WIFI_LINK_RECEIVE_RATE_KEY,
  NETWORK_WIFI_LINK_TRANSMIT_RATE_KEY,
  NETWORK_WIFI_SIGNAL_QUALITY_KEY,
  NETWORK_WIFI_SIGNAL_RSSI_KEY,
} from '@/types/wellknown';
import { NetworkDetailsCard } from '@/components/NetworkDetailsCard/NetworkDetailsCard';
import * as metricsService from '@/services/metrics';

const AVAILABLE: Availability = { status: 'available' };

const WIFI = 'network:mac-9009df3e97f2';
const ETH = 'network:mac-d843ae328ac9';
const DOCKER = 'network:mac-02427057015e';
const VPN = 'network:if-wg0';

interface FakeInterface {
  readonly sourceId: string;
  readonly label: string;
  readonly wireless?: boolean;
}

function definition(key: string, sourceId: string, sourceLabel: string): MetricDefinition {
  return {
    metric: { key, sourceId },
    sourceLabel,
    displayName: key,
    description: '',
    category: 'network',
    unit: 'bytesPerSecond',
    valueType: 'number',
    kind: 'gauge',
    availability: AVAILABLE,
    providerId: 'linux.network',
  };
}

function catalogFor(interfaces: readonly FakeInterface[]): MetricDefinition[] {
  const entries = interfaces.flatMap((entry) => {
    const keys = entry.wireless
      ? [...NETWORK_PER_INTERFACE_KEYS, ...NETWORK_PER_WIFI_KEYS]
      : NETWORK_PER_INTERFACE_KEYS;

    return keys.map((key) => definition(key, entry.sourceId, entry.label));
  });

  entries.push(definition(NETWORK_INTERFACE_COUNT_KEY, 'network:system', 'Network'));
  entries.push(definition(NETWORK_INTERFACE_UP_COUNT_KEY, 'network:system', 'Network'));

  return entries.sort((left, right) =>
    `${left.metric.key}@${left.metric.sourceId}`.localeCompare(
      `${right.metric.key}@${right.metric.sourceId}`,
    ),
  );
}

function respond(
  requested: readonly MetricRef[],
  value: (metric: MetricRef) => number | Availability,
): MetricSample[] {
  return requested.map((metric) => {
    const result = value(metric);

    return typeof result === 'number'
      ? {
          metric,
          timestamp: 1_700_000_000_000,
          value: { type: 'number', value: result },
          availability: AVAILABLE,
        }
      : { metric, timestamp: 1_700_000_000_000, value: null, availability: result };
  });
}

/** A fully measured, connected interface. */
function active(metric: MetricRef, counts: { total: number; up: number }): number | Availability {
  switch (metric.key) {
    case NETWORK_INTERFACE_COUNT_KEY:
      return counts.total;
    case NETWORK_INTERFACE_UP_COUNT_KEY:
      return counts.up;
    case NETWORK_RECEIVE_BYTES_KEY:
      return 13_002_342;
    case NETWORK_TRANSMIT_BYTES_KEY:
      return 1_258_291;
    case NETWORK_RECEIVE_PACKETS_KEY:
      return 8400;
    case NETWORK_TRANSMIT_PACKETS_KEY:
      return 2100;
    case NETWORK_RECEIVE_ERRORS_KEY:
    case NETWORK_TRANSMIT_ERRORS_KEY:
    case NETWORK_RECEIVE_DROPPED_KEY:
    case NETWORK_TRANSMIT_DROPPED_KEY:
      return 0;
    case NETWORK_LINK_RECEIVE_SPEED_KEY:
    case NETWORK_LINK_TRANSMIT_SPEED_KEY:
      return 1_000_000_000;
    case NETWORK_MTU_KEY:
      return 1500;
    case NETWORK_WIFI_SIGNAL_RSSI_KEY:
      return -68;
    case NETWORK_WIFI_SIGNAL_QUALITY_KEY:
      return 92;
    case NETWORK_WIFI_LINK_RECEIVE_RATE_KEY:
      return 175_500_000;
    case NETWORK_WIFI_LINK_TRANSMIT_RATE_KEY:
      return 390_000_000;
    default:
      return { status: 'notRegistered', reason: 'unexpected' };
  }
}

function mockBackend(
  interfaces: readonly FakeInterface[],
  value: (metric: MetricRef) => number | Availability = (metric) =>
    active(metric, { total: interfaces.length, up: interfaces.length }),
) {
  const catalog = vi
    .spyOn(metricsService, 'getMetricCatalog')
    .mockResolvedValue(catalogFor(interfaces));
  const sample = vi
    .spyOn(metricsService, 'sampleMetrics')
    .mockImplementation((requested) => Promise.resolve(respond(requested, value)));

  return { catalog, sample };
}

const WIFI_IF: FakeInterface = { sourceId: WIFI, label: 'wlp59s0f0 · Wi-Fi', wireless: true };
const ETH_IF: FakeInterface = { sourceId: ETH, label: 'enp58s0 · Ethernet' };
const DOCKER_IF: FakeInterface = { sourceId: DOCKER, label: 'docker0 · Bridge' };
const VPN_IF: FakeInterface = { sourceId: VPN, label: 'wg0 · Tunnel' };

async function card() {
  render(<NetworkDetailsCard />);
  return screen.findByLabelText('Network details');
}

/**
 * The value shown beside one key/value label.
 *
 * Looked up through the `<dt>` rather than by searching for the value's text:
 * `0/s` and `1500 B` appear on several interfaces at once, and asserting on
 * the bare string would silently test whichever one the query found first.
 */
function rowValue(scope: HTMLElement, label: string): string {
  const term = within(scope)
    .getAllByText(label)
    .find((node) => node.tagName === 'DT');

  return term?.nextElementSibling?.textContent?.trim() ?? '';
}

/** The card block belonging to one interface. */
function entryFor(region: HTMLElement, name: string): HTMLElement {
  const heading = within(region)
    .getAllByText(name)
    .find((node) => node.tagName === 'H4');

  const item = heading?.closest('li');
  if (!item) throw new Error(`no interface block for '${name}'`);

  return item as HTMLElement;
}

afterEach(() => {
  vi.restoreAllMocks();
});

// --- inventory -------------------------------------------------------------

describe('NetworkDetailsCard inventory', () => {
  it('reports a machine with no interfaces without pretending it failed', async () => {
    mockBackend([], (metric) => active(metric, { total: 0, up: 0 }));

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText(/No network interface was inventoried/)).toBeVisible();
    });
    expect(rowValue(region, 'Interfaces')).toBe('0');
  });

  it('shows one Ethernet interface', async () => {
    mockBackend([ETH_IF]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('enp58s0')).toBeVisible();
    });
    expect(entryFor(region, 'enp58s0').textContent).toContain('Ethernet');
  });

  it('shows one Wi-Fi interface with its radio figures', async () => {
    mockBackend([WIFI_IF]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('wlp59s0f0')).toBeVisible();
    });

    const entry = entryFor(region, 'wlp59s0f0');
    expect(rowValue(entry, 'Signal')).toBe('-68 dBm');
    expect(rowValue(entry, 'Quality')).toBe('92 %');
  });

  it('shows Ethernet and Wi-Fi together', async () => {
    mockBackend([ETH_IF, WIFI_IF]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('wlp59s0f0')).toBeVisible();
    });
    expect(within(region).getByText('enp58s0')).toBeVisible();
  });

  it('shows a VPN tunnel among the primary interfaces', async () => {
    mockBackend([WIFI_IF, VPN_IF]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('wg0')).toBeVisible();
    });
    // Not tucked away with the bridges.
    expect(within(region).queryByLabelText('Virtual interfaces')).toBeNull();
  });

  it('never shows the loopback interface', async () => {
    // The backend excludes it from the catalog entirely, so it cannot reach
    // the card even if something asked for it.
    mockBackend([WIFI_IF]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('wlp59s0f0')).toBeVisible();
    });
    expect(within(region).queryByText('lo')).toBeNull();
    expect(region.textContent).not.toContain('Loopback');
  });

  it('shows a long adapter description without losing it', async () => {
    const verbose: FakeInterface = {
      sourceId: 'network:mac-001122334455',
      label: 'Intel(R) Wi-Fi 6E AX211 160MHz Wireless Network Adapter · Wi-Fi',
      wireless: true,
    };
    mockBackend([verbose]);

    const region = await card();

    await waitFor(() => {
      expect(
        within(region).getByText('Intel(R) Wi-Fi 6E AX211 160MHz Wireless Network Adapter'),
      ).toBeVisible();
    });
  });

  it('shows the stable identity in a tooltip rather than in the text', async () => {
    mockBackend([WIFI_IF]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('wlp59s0f0')).toHaveAttribute('title', WIFI);
    });
    // The MAC-derived identity is not rendered as visible text.
    expect(region.textContent).not.toContain('9009df3e97f2');
  });
});

// --- many virtual interfaces -----------------------------------------------

describe('NetworkDetailsCard virtual interfaces', () => {
  it('collapses virtual interfaces behind a control rather than burying the real ones', async () => {
    const many: FakeInterface[] = Array.from({ length: 10 }, (_, index) => ({
      sourceId: `network:if-veth${index}`,
      label: `veth${index} · Virtual`,
    }));
    mockBackend([WIFI_IF, DOCKER_IF, ...many]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('wlp59s0f0')).toBeVisible();
    });

    const section = within(region).getByLabelText('Virtual interfaces');
    expect(within(section).getByText(/Virtual interfaces \(11\)/)).toBeVisible();
    // Collapsed: the list is not rendered yet.
    expect(within(region).queryByLabelText('Virtual interface list')).toBeNull();
    expect(within(region).queryByText('veth0')).toBeNull();
  });

  it('reveals them on request, and hides them again', async () => {
    const user = userEvent.setup();
    mockBackend([WIFI_IF, DOCKER_IF]);

    const region = await card();
    await waitFor(() => {
      expect(within(region).getByText('wlp59s0f0')).toBeVisible();
    });

    await user.click(within(region).getByRole('button', { name: 'Show all' }));
    expect(within(region).getByText('docker0')).toBeVisible();

    await user.click(within(region).getByRole('button', { name: 'Hide' }));
    expect(within(region).queryByText('docker0')).toBeNull();
  });

  it('shows no virtual section when there is nothing to collapse', async () => {
    mockBackend([WIFI_IF, ETH_IF]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('wlp59s0f0')).toBeVisible();
    });
    expect(within(region).queryByLabelText('Virtual interfaces')).toBeNull();
  });
});

// --- traffic ---------------------------------------------------------------

describe('NetworkDetailsCard traffic', () => {
  it('shows download, upload and packet rates for an active interface', async () => {
    mockBackend([WIFI_IF]);

    const region = await card();
    await waitFor(() => {
      expect(within(region).getByText('wlp59s0f0')).toBeVisible();
    });

    const entry = entryFor(region, 'wlp59s0f0');
    expect(rowValue(entry, 'Download')).toBe('12.4 MiB/s');
    expect(rowValue(entry, 'Upload')).toBe('1.2 MiB/s');
    expect(rowValue(entry, 'RX packets')).toBe('8.4k/s');
    expect(rowValue(entry, 'TX packets')).toBe('2.1k/s');
  });

  it('says it is waiting when the first sample has no baseline', async () => {
    // The state a fresh launch is in. It must be explained in the card, not
    // only in a tooltip nobody will hover.
    const waiting: Availability = {
      status: 'temporarilyUnavailable',
      reason: 'network traffic is measured between two samples; waiting for the next one',
    };
    mockBackend([WIFI_IF], (metric) =>
      NETWORK_TRAFFIC_KEYS.includes(metric.key) ? waiting : active(metric, { total: 1, up: 1 }),
    );

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('Waiting for another sample')).toBeVisible();
    });
    expect(within(region).getByText(/Refresh to measure it/)).toBeVisible();

    // And no fabricated zeros.
    const entry = entryFor(region, 'wlp59s0f0');
    expect(rowValue(entry, 'Download')).toBe('—');
    expect(rowValue(entry, 'Download')).not.toBe('0 B/s');
  });

  it('shows zero traffic for an idle interface, without the waiting notice', async () => {
    // Over a real interval these are measurements, not absences.
    mockBackend([ETH_IF], (metric) =>
      NETWORK_TRAFFIC_KEYS.includes(metric.key) ? 0 : active(metric, { total: 1, up: 1 }),
    );

    const region = await card();
    await waitFor(() => {
      expect(within(region).getByText('enp58s0')).toBeVisible();
    });

    const entry = entryFor(region, 'enp58s0');
    expect(rowValue(entry, 'Download')).toBe('0 B/s');
    expect(rowValue(entry, 'RX packets')).toBe('0/s');
    expect(within(region).queryByText('Waiting for another sample')).toBeNull();
  });

  it('shows a clean interface: errors and drops as zero pairs', async () => {
    mockBackend([ETH_IF]);

    const region = await card();
    await waitFor(() => {
      expect(within(region).getByText('enp58s0')).toBeVisible();
    });

    const entry = entryFor(region, 'enp58s0');
    expect(rowValue(entry, 'Errors')).toBe('0/s RX · 0/s TX');
    expect(rowValue(entry, 'Dropped')).toBe('0/s RX · 0/s TX');
  });

  it('marks a non-zero error rate without turning it into a score', async () => {
    mockBackend([ETH_IF], (metric) => {
      if (metric.key === NETWORK_RECEIVE_ERRORS_KEY) return 17;
      return active(metric, { total: 1, up: 1 });
    });

    const region = await card();
    await waitFor(() => {
      expect(within(region).getByText('enp58s0')).toBeVisible();
    });

    const entry = entryFor(region, 'enp58s0');
    expect(rowValue(entry, 'Errors')).toBe('17/s RX · 0/s TX');
    // No verdict anywhere.
    expect(region.textContent).not.toMatch(/\b(health|score|\d+\/100)\b/i);
  });

  it('keeps errors and drops as separate rows', async () => {
    // Two different counters: a drop is an intact frame nobody wanted, an
    // error is a frame that arrived broken.
    mockBackend([ETH_IF], (metric) => {
      if (metric.key === NETWORK_RECEIVE_ERRORS_KEY) return 3;
      if (metric.key === NETWORK_RECEIVE_DROPPED_KEY) return 42;
      return active(metric, { total: 1, up: 1 });
    });

    const region = await card();
    await waitFor(() => {
      expect(within(region).getByText('enp58s0')).toBeVisible();
    });

    const entry = entryFor(region, 'enp58s0');
    expect(rowValue(entry, 'Errors')).toContain('3/s RX');
    expect(rowValue(entry, 'Dropped')).toContain('42/s RX');
  });
});

// --- link speed and state --------------------------------------------------

describe('NetworkDetailsCard link', () => {
  it('shows a known link speed in bits, beside traffic in bytes', async () => {
    // The factor-of-eight trap, rendered correctly.
    mockBackend([ETH_IF]);

    const region = await card();
    await waitFor(() => {
      expect(within(region).getByText('enp58s0')).toBeVisible();
    });

    const entry = entryFor(region, 'enp58s0');
    expect(rowValue(entry, 'Link RX')).toBe('1.0 Gbit/s');
    expect(rowValue(entry, 'Download')).toContain('iB/s');
  });

  it('shows a dash when the link speed is unknown', async () => {
    const unknown: Availability = {
      status: 'temporarilyUnavailable',
      reason: 'this interface negotiates no link speed while it has no carrier',
    };
    mockBackend([ETH_IF], (metric) =>
      metric.key === NETWORK_LINK_RECEIVE_SPEED_KEY ||
      metric.key === NETWORK_LINK_TRANSMIT_SPEED_KEY
        ? unknown
        : active(metric, { total: 1, up: 0 }),
    );

    const region = await card();
    await waitFor(() => {
      expect(within(region).getByText('enp58s0')).toBeVisible();
    });

    const entry = entryFor(region, 'enp58s0');
    expect(rowValue(entry, 'Link RX')).toBe('—');
    expect(rowValue(entry, 'Link RX')).not.toBe('0 bit/s');
  });

  it('shows a disconnected adapter rather than removing it', async () => {
    // An unplugged Ethernet port is still worth seeing.
    const down: Availability = { status: 'temporarilyUnavailable', reason: 'no carrier' };
    mockBackend([WIFI_IF, ETH_IF], (metric) => {
      if (metric.sourceId === ETH) {
        if (metric.key === NETWORK_MTU_KEY) return 1500;
        return down;
      }
      return active(metric, { total: 2, up: 1 });
    });

    const region = await card();
    await waitFor(() => {
      expect(within(region).getByText('enp58s0')).toBeVisible();
    });

    expect(entryFor(region, 'enp58s0').textContent).toContain('Disconnected');
    expect(entryFor(region, 'wlp59s0f0').textContent).toContain('Connected');
    expect(rowValue(region, 'Connected')).toBe('1');
  });

  it('shows the MTU beside the interface kind', async () => {
    mockBackend([ETH_IF]);

    const region = await card();
    await waitFor(() => {
      expect(within(region).getByText('enp58s0')).toBeVisible();
    });

    expect(entryFor(region, 'enp58s0').textContent).toContain('MTU');
  });
});

// --- Wi-Fi -----------------------------------------------------------------

describe('NetworkDetailsCard Wi-Fi', () => {
  it('shows signal, quality and both negotiated rates when all are available', async () => {
    mockBackend([WIFI_IF]);

    const region = await card();
    await waitFor(() => {
      expect(within(region).getByText('wlp59s0f0')).toBeVisible();
    });

    const entry = entryFor(region, 'wlp59s0f0');
    expect(rowValue(entry, 'Signal')).toBe('-68 dBm');
    expect(rowValue(entry, 'Quality')).toBe('92 %');
    expect(rowValue(entry, 'Wi-Fi RX')).toBe('175.5 Mbit/s');
    expect(rowValue(entry, 'Wi-Fi TX')).toBe('390.0 Mbit/s');
  });

  it('shows the RSSI with a dash for quality on a platform that reports none', async () => {
    // Fedora's case. The signal is real; the quality percentage is honestly
    // absent rather than invented from it.
    const unsupported: Availability = {
      status: 'unsupported',
      reason: 'this platform reports signal strength in dBm and no quality percentage',
    };
    mockBackend([WIFI_IF], (metric) =>
      metric.key === NETWORK_WIFI_SIGNAL_QUALITY_KEY
        ? unsupported
        : active(metric, { total: 1, up: 1 }),
    );

    const region = await card();
    await waitFor(() => {
      expect(within(region).getByText('wlp59s0f0')).toBeVisible();
    });

    const entry = entryFor(region, 'wlp59s0f0');
    expect(rowValue(entry, 'Signal')).toBe('-68 dBm');
    expect(rowValue(entry, 'Quality')).toBe('—');
    // The one thing that must never appear: a percentage derived from dBm.
    expect(rowValue(entry, 'Quality')).not.toMatch(/\d+ %/);
  });

  it('explains a disconnected radio instead of showing four dashes', async () => {
    const notConnected: Availability = {
      status: 'temporarilyUnavailable',
      reason: 'this adapter is not associated with a network, so it has no link to measure',
    };
    mockBackend([WIFI_IF], (metric) =>
      NETWORK_PER_WIFI_KEYS.includes(metric.key)
        ? notConnected
        : active(metric, { total: 1, up: 1 }),
    );

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('Wi-Fi link details unavailable')).toBeVisible();
    });
    expect(within(region).getByText(/not associated with a network/)).toBeVisible();
    // The interface itself is still fully present.
    expect(rowValue(entryFor(region, 'wlp59s0f0'), 'Download')).toBe('12.4 MiB/s');
  });

  it('shows no Wi-Fi rows at all on a wired interface', async () => {
    // Not four permanently-empty ones: there is no radio.
    mockBackend([ETH_IF]);

    const region = await card();
    await waitFor(() => {
      expect(within(region).getByText('enp58s0')).toBeVisible();
    });

    const entry = entryFor(region, 'enp58s0');
    expect(within(entry).queryByText('Signal')).toBeNull();
    expect(within(entry).queryByText('Quality')).toBeNull();
  });
});

// --- refresh ---------------------------------------------------------------

describe('NetworkDetailsCard refresh', () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('samples once on mount and once per refresh, never on a timer', async () => {
    const { catalog, sample } = mockBackend([WIFI_IF]);
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });

    const region = await card();
    await waitFor(() => {
      expect(sample).toHaveBeenCalledTimes(1);
    });

    // No hidden polling: a whole minute passes and nothing is requested.
    await vi.advanceTimersByTimeAsync(60_000);
    expect(sample).toHaveBeenCalledTimes(1);

    await user.click(within(region).getByRole('button', { name: 'Refresh network details' }));
    await waitFor(() => {
      expect(sample).toHaveBeenCalledTimes(2);
    });

    // The inventory is discovered once and never re-enumerated.
    expect(catalog).toHaveBeenCalledTimes(1);
  });

  it('shows when the displayed figures were read', async () => {
    mockBackend([WIFI_IF]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText(/^Updated /)).toBeVisible();
    });
  });

  it('reports a backend failure without claiming the machine has no interfaces', async () => {
    vi.spyOn(metricsService, 'getMetricCatalog').mockRejectedValue(new Error('no runtime'));

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText(/Backend unavailable/)).toBeVisible();
    });
    expect(within(region).queryByText(/No network interface was inventoried/)).toBeNull();
  });
});
