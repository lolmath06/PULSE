import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Availability, MetricDefinition, MetricRef, MetricSample } from '@/types/metrics';
import {
  STORAGE_CAPACITY_TOTAL_KEY,
  STORAGE_DEVICE_COUNT_KEY,
  STORAGE_HEALTH_AVAILABLE_SPARE_KEY,
  STORAGE_HEALTH_MEDIA_ERRORS_KEY,
  STORAGE_HEALTH_PERCENTAGE_USED_KEY,
  STORAGE_HEALTH_POWER_ON_HOURS_KEY,
  STORAGE_HEALTH_TEMPERATURE_KEY,
  STORAGE_HEALTH_UNSAFE_SHUTDOWNS_KEY,
  STORAGE_IO_KEYS,
  STORAGE_IO_READ_BYTES_KEY,
  STORAGE_IO_READ_IOPS_KEY,
  STORAGE_IO_READ_LATENCY_KEY,
  STORAGE_IO_WRITE_BYTES_KEY,
  STORAGE_IO_WRITE_IOPS_KEY,
  STORAGE_IO_WRITE_LATENCY_KEY,
  STORAGE_PER_DEVICE_KEYS,
  STORAGE_PER_VOLUME_KEYS,
  STORAGE_VOLUME_CAPACITY_AVAILABLE_KEY,
  STORAGE_VOLUME_CAPACITY_TOTAL_KEY,
  STORAGE_VOLUME_CAPACITY_USED_KEY,
  STORAGE_VOLUME_COUNT_KEY,
  STORAGE_VOLUME_USAGE_PERCENT_KEY,
} from '@/types/wellknown';
import { StorageDetailsCard } from '@/components/StorageDetailsCard/StorageDetailsCard';
import * as metricsService from '@/services/metrics';

const AVAILABLE: Availability = { status: 'available' };
const GIB = 1024 * 1024 * 1024;

const NVME = 'storage:wwid-eui.002538b331b36d03';
const USB = 'storage:wwid-t10.intenso-scsi-2019131398ab5';

interface FakeDevice {
  readonly sourceId: string;
  readonly label: string;
}

interface FakeVolume {
  readonly sourceId: string;
  readonly label: string;
}

function definition(key: string, sourceId: string, sourceLabel: string): MetricDefinition {
  return {
    metric: { key, sourceId },
    sourceLabel,
    displayName: key,
    description: '',
    category: 'storage',
    unit: 'bytes',
    valueType: 'number',
    kind: 'gauge',
    availability: AVAILABLE,
    providerId: 'linux.storage',
  };
}

function catalogFor(
  devices: readonly FakeDevice[],
  volumes: readonly FakeVolume[],
): MetricDefinition[] {
  const entries = [
    ...devices.flatMap((device) =>
      STORAGE_PER_DEVICE_KEYS.map((key) => definition(key, device.sourceId, device.label)),
    ),
    ...volumes.flatMap((volume) =>
      STORAGE_PER_VOLUME_KEYS.map((key) => definition(key, volume.sourceId, volume.label)),
    ),
    definition(STORAGE_DEVICE_COUNT_KEY, 'storage:system', 'Storage'),
    definition(STORAGE_VOLUME_COUNT_KEY, 'storage:system', 'Storage'),
  ];

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

/** A fully telemetered NVMe drive with a healthy controller. */
function healthy(
  metric: MetricRef,
  counts: { devices: number; volumes: number },
): number | Availability {
  switch (metric.key) {
    case STORAGE_DEVICE_COUNT_KEY:
      return counts.devices;
    case STORAGE_VOLUME_COUNT_KEY:
      return counts.volumes;
    case STORAGE_CAPACITY_TOTAL_KEY:
      return 2_048_408_248_320;
    case STORAGE_IO_READ_BYTES_KEY:
      return 131_072_000;
    case STORAGE_IO_WRITE_BYTES_KEY:
      return 18_874_368;
    case STORAGE_IO_READ_IOPS_KEY:
      return 940;
    case STORAGE_IO_WRITE_IOPS_KEY:
      return 120;
    case STORAGE_IO_READ_LATENCY_KEY:
      return 0.73;
    case STORAGE_IO_WRITE_LATENCY_KEY:
      return 1.2;
    case STORAGE_HEALTH_TEMPERATURE_KEY:
      return 43;
    case STORAGE_HEALTH_PERCENTAGE_USED_KEY:
      return 3;
    case STORAGE_HEALTH_AVAILABLE_SPARE_KEY:
      return 100;
    case STORAGE_HEALTH_POWER_ON_HOURS_KEY:
      return 421;
    case STORAGE_HEALTH_UNSAFE_SHUTDOWNS_KEY:
      return 7;
    case STORAGE_HEALTH_MEDIA_ERRORS_KEY:
      return 0;
    case STORAGE_VOLUME_CAPACITY_TOTAL_KEY:
      return 400 * GIB;
    case STORAGE_VOLUME_CAPACITY_USED_KEY:
      return 92 * GIB;
    case STORAGE_VOLUME_CAPACITY_AVAILABLE_KEY:
      return 300 * GIB;
    case STORAGE_VOLUME_USAGE_PERCENT_KEY:
      return 23;
    default:
      return { status: 'notRegistered', reason: 'unexpected' };
  }
}

function mockBackend(
  devices: readonly FakeDevice[],
  volumes: readonly FakeVolume[],
  value: (metric: MetricRef) => number | Availability = (metric) =>
    healthy(metric, { devices: devices.length, volumes: volumes.length }),
) {
  const catalog = vi
    .spyOn(metricsService, 'getMetricCatalog')
    .mockResolvedValue(catalogFor(devices, volumes));
  const sample = vi
    .spyOn(metricsService, 'sampleMetrics')
    .mockImplementation((requested) => Promise.resolve(respond(requested, value)));

  return { catalog, sample };
}

const SAMSUNG: FakeDevice = { sourceId: NVME, label: 'SAMSUNG MZVL22T0HBLB-00B00 · NVMe' };
const INTENSO: FakeDevice = { sourceId: USB, label: 'Intenso USB3.0 Device · USB' };

const ROOT: FakeVolume = {
  sourceId: `volume:${NVME.slice('storage:'.length)}-p8`,
  label: '/',
};
const BOOT: FakeVolume = {
  sourceId: `volume:${NVME.slice('storage:'.length)}-p7`,
  label: '/boot',
};
const DATA: FakeVolume = {
  sourceId: `volume:${USB.slice('storage:'.length)}-p4`,
  label: '/mnt/data',
};

async function card() {
  render(<StorageDetailsCard />);
  return screen.findByLabelText('Storage details');
}

/**
 * The value shown beside one key/value label.
 *
 * Looked up through the `<dt>` rather than by searching for the value's text:
 * `0` appears as a media-error count, a volume count and a device count at
 * once, and asserting on the bare string would silently test whichever one the
 * query happened to find first.
 */
function rowValue(region: HTMLElement, label: string): string {
  const term = within(region)
    .getAllByText(label)
    .find((node) => node.tagName === 'DT');

  return term?.nextElementSibling?.textContent?.trim() ?? '';
}

afterEach(() => {
  vi.restoreAllMocks();
});

// --- inventory -------------------------------------------------------------

describe('StorageDetailsCard inventory', () => {
  it('reports a machine with no storage without pretending it failed', async () => {
    mockBackend([], []);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText(/No physical storage device was inventoried/)).toBeVisible();
    });
    // Zero is a fact, reported as a count rather than as an error.
    expect(rowValue(region, 'Storage devices')).toBe('0');
  });

  it('shows one device with its capacity', async () => {
    mockBackend([SAMSUNG], []);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText(SAMSUNG.label)).toBeVisible();
    });
    expect(within(region).getByText('1.9 TiB')).toBeVisible();
  });

  it('shows several devices, each as its own block', async () => {
    mockBackend([SAMSUNG, INTENSO], []);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText(SAMSUNG.label)).toBeVisible();
    });
    expect(within(region).getByText(INTENSO.label)).toBeVisible();
    expect(within(region).getAllByRole('listitem').length).toBeGreaterThanOrEqual(2);
  });

  it('numbers two identical drives without touching their identities', async () => {
    const twins: FakeDevice[] = [
      { sourceId: 'storage:serial-s111', label: 'Twin SSD · NVMe' },
      { sourceId: 'storage:serial-s222', label: 'Twin SSD · NVMe' },
    ];
    mockBackend(twins, []);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('Twin SSD · NVMe #1')).toBeVisible();
    });
    // The tooltip carries the real, distinct identity.
    expect(within(region).getByText('Twin SSD · NVMe #1')).toHaveAttribute(
      'title',
      'storage:serial-s111',
    );
    expect(within(region).getByText('Twin SSD · NVMe #2')).toHaveAttribute(
      'title',
      'storage:serial-s222',
    );
  });

  it('shows a device whose model name is long without breaking the layout', async () => {
    const verbose: FakeDevice = {
      sourceId: 'storage:serial-abc',
      label: 'SAMSUNG MZVL22T0HBLB-00B00 Enterprise Edition Extended Product Name · NVMe',
    };
    mockBackend([verbose], []);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText(verbose.label)).toBeVisible();
    });
  });

  it('shows a device with no serial at all', async () => {
    // A USB bridge that reports none still gets a row, identified by whatever
    // the backend could derive.
    mockBackend([{ sourceId: 'storage:dev-sdb', label: 'Generic disk · USB' }], []);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('Generic disk · USB')).toBeVisible();
    });
  });
});

// --- activity --------------------------------------------------------------

describe('StorageDetailsCard activity', () => {
  it('shows throughput, IOPS and latency for an active disk', async () => {
    mockBackend([SAMSUNG], []);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('125 MiB/s')).toBeVisible();
    });
    expect(within(region).getByText('18.0 MiB/s')).toBeVisible();
    expect(within(region).getByText('940 IOPS')).toBeVisible();
    expect(within(region).getByText('120 IOPS')).toBeVisible();
    expect(within(region).getByText('0.73 ms')).toBeVisible();
    expect(within(region).getByText('1.2 ms')).toBeVisible();
  });

  it('says it is waiting when the first sample has no baseline', async () => {
    // The state a fresh launch is in. It must be explained in the card, not
    // only in a tooltip nobody will hover.
    const waiting: Availability = {
      status: 'temporarilyUnavailable',
      reason: 'disk activity is measured between two samples; waiting for the next one',
    };
    mockBackend([SAMSUNG], [], (metric) =>
      STORAGE_IO_KEYS.includes(metric.key) ? waiting : healthy(metric, { devices: 1, volumes: 0 }),
    );

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('Waiting for another sample')).toBeVisible();
    });
    expect(within(region).getByText(/Refresh to measure it/)).toBeVisible();
  });

  it('shows zero throughput and zero IOPS for an idle disk', async () => {
    // Over a real interval these are measurements, not absences, and must not
    // be shown as dashes.
    mockBackend([SAMSUNG], [], (metric) => {
      if (
        metric.key === STORAGE_IO_READ_LATENCY_KEY ||
        metric.key === STORAGE_IO_WRITE_LATENCY_KEY
      ) {
        return {
          status: 'temporarilyUnavailable',
          reason: 'no operation of this kind completed during the interval',
        };
      }
      if (STORAGE_IO_KEYS.includes(metric.key)) return 0;
      return healthy(metric, { devices: 1, volumes: 0 });
    });

    const region = await card();

    await waitFor(() => {
      expect(within(region).getAllByText('0 B/s')).toHaveLength(2);
    });
    expect(within(region).getAllByText('0 IOPS')).toHaveLength(2);
    // …and no baseline notice, because there is a baseline.
    expect(within(region).queryByText('Waiting for another sample')).toBeNull();
  });

  it('never fabricates a latency when no operation completed', async () => {
    const idle: Availability = {
      status: 'temporarilyUnavailable',
      reason: 'no operation of this kind completed during the interval, so there is no latency',
    };
    mockBackend([SAMSUNG], [], (metric) => {
      if (
        metric.key === STORAGE_IO_READ_LATENCY_KEY ||
        metric.key === STORAGE_IO_WRITE_LATENCY_KEY
      ) {
        return idle;
      }
      if (STORAGE_IO_KEYS.includes(metric.key)) return 0;
      return healthy(metric, { devices: 1, volumes: 0 });
    });

    const region = await card();

    await waitFor(() => {
      expect(within(region).getAllByText('0 IOPS')).toHaveLength(2);
    });

    const dashes = within(region).getAllByTitle(/no latency/);
    expect(dashes).toHaveLength(2);
    for (const dash of dashes) expect(dash).toHaveTextContent('—');
    // The one thing that must never appear.
    expect(within(region).queryByText('0.00 ms')).toBeNull();
  });
});

// --- health ----------------------------------------------------------------

describe('StorageDetailsCard health', () => {
  it('shows every standardised health value when the controller reports them', async () => {
    mockBackend([SAMSUNG], []);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('43 °C')).toBeVisible();
    });
    expect(within(region).getByText('3 %')).toBeVisible();
    expect(within(region).getByText('100 %')).toBeVisible();
    expect(within(region).getByText('421 h')).toBeVisible();
    expect(within(region).getByText('7')).toBeVisible();
    // Zero media errors is the answer a user hopes for, shown as the
    // measurement it is rather than as a dash.
    expect(rowValue(region, 'Media errors')).toBe('0');
  });

  it('keeps the temperature when the rest of the log is refused', async () => {
    // The unprivileged Fedora case: the kernel publishes a composite
    // temperature and the log page itself needs root.
    const denied: Availability = {
      status: 'permissionDenied',
      reason: 'reading the NVMe health log needs privileged access to /dev/nvme0',
    };
    mockBackend([SAMSUNG], [], (metric) => {
      if (metric.key === STORAGE_HEALTH_TEMPERATURE_KEY) return 52.85;
      if (metric.key.startsWith('storage.health.')) return denied;
      return healthy(metric, { devices: 1, volumes: 0 });
    });

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('53 °C')).toBeVisible();
    });
    // The refused values are dashes carrying the real reason, never zeros.
    expect(within(region).getAllByTitle(/privileged access/).length).toBeGreaterThan(0);
    expect(within(region).queryByText('0 %')).toBeNull();
  });

  it('explains a permission problem rather than hiding the rows', async () => {
    const denied: Availability = {
      status: 'permissionDenied',
      reason: 'reading the NVMe health log needs privileged access to /dev/nvme0',
    };
    mockBackend([SAMSUNG], [], (metric) =>
      metric.key.startsWith('storage.health.')
        ? denied
        : healthy(metric, { devices: 1, volumes: 0 }),
    );

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('Health reporting unavailable')).toBeVisible();
    });
    expect(within(region).getByText(/privileged access to \/dev\/nvme0/)).toBeVisible();
    // The disk itself is still fully present.
    expect(within(region).getByText('1.9 TiB')).toBeVisible();
    expect(within(region).getByText('940 IOPS')).toBeVisible();
  });

  it('says a SATA drive has no backend rather than claiming it is broken', async () => {
    const unsupported: Availability = {
      status: 'unsupported',
      reason:
        'PULSE reports only the standardised NVMe health log so far. ATA SMART attributes are vendor-defined.',
    };
    mockBackend(
      [{ sourceId: 'storage:serial-wd1', label: 'WDC WDS100T2B0A · SATA' }],
      [],
      (metric) =>
        metric.key.startsWith('storage.health.')
          ? unsupported
          : healthy(metric, { devices: 1, volumes: 0 }),
    );

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('Health reporting unavailable')).toBeVisible();
    });
    expect(within(region).getByText(/ATA SMART attributes are vendor-defined/)).toBeVisible();
  });

  it('shows a drive past its rated endurance without clamping it to 100', async () => {
    // The NVMe specification permits it, and it is exactly the reading a user
    // must not scroll past.
    mockBackend([SAMSUNG], [], (metric) =>
      metric.key === STORAGE_HEALTH_PERCENTAGE_USED_KEY
        ? 155
        : healthy(metric, { devices: 1, volumes: 0 }),
    );

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('155 %')).toBeVisible();
    });
  });

  it('shows no health verdict anywhere', async () => {
    // PULSE publishes measurements. There is deliberately no score.
    mockBackend([SAMSUNG], []);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('43 °C')).toBeVisible();
    });
    expect(region.textContent).not.toMatch(/\b(GOOD|healthy|health score|\d+\/100)\b/i);
  });
});

// --- volumes ---------------------------------------------------------------

describe('StorageDetailsCard volumes', () => {
  it('nests a volume under the device it lives on', async () => {
    mockBackend([SAMSUNG, INTENSO], [ROOT, BOOT, DATA]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('/')).toBeVisible();
    });
    expect(within(region).getByText('/boot')).toBeVisible();
    expect(within(region).getByText('/mnt/data')).toBeVisible();
    // Nothing fell through to the "other" section.
    expect(within(region).queryByLabelText('Other volumes')).toBeNull();
  });

  it('shows a volume PULSE could not attribute in its own section', async () => {
    // Attributing it to whichever disk looks plausible would be worse than
    // admitting the link is unknown.
    const share: FakeVolume = { sourceId: 'volume:mm-0-99', label: '/mnt/share' };
    mockBackend([SAMSUNG], [ROOT, share]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByLabelText('Other volumes')).toBeVisible();
    });
    const other = within(region).getByLabelText('Other volumes');
    expect(within(other).getByText('/mnt/share')).toBeVisible();
    expect(within(other).queryByText('/')).toBeNull();
  });

  it('shows used, total, free and a usage bar', async () => {
    mockBackend([SAMSUNG], [ROOT]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('92.0 / 400.0 GiB')).toBeVisible();
    });
    expect(within(region).getByText(/300\.0 GiB free/)).toBeVisible();
    expect(within(region).getByLabelText('/ is 23 % full')).toBeVisible();
  });

  it('draws no usage bar for a volume with no reported size', async () => {
    // An empty bar would read as "0 % used" on a filesystem nothing measured.
    const missing: Availability = { status: 'notDetected', reason: 'no size' };
    mockBackend([SAMSUNG], [ROOT], (metric) =>
      metric.key.startsWith('storage.volume.')
        ? missing
        : healthy(metric, { devices: 1, volumes: 1 }),
    );

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('/')).toBeVisible();
    });
    expect(within(region).queryByRole('img')).toBeNull();
  });

  it('shows several volumes on one device', async () => {
    mockBackend([SAMSUNG], [ROOT, BOOT]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText('/boot')).toBeVisible();
    });
    expect(within(region).getByText('/')).toBeVisible();
  });

  it('counts one filesystem once however many paths it is mounted at', async () => {
    // Fedora mounts the same btrfs filesystem at `/` and `/home`. The backend
    // publishes it as one volume; the card must not invent a second.
    mockBackend([SAMSUNG], [ROOT]);

    const region = await card();

    await waitFor(() => {
      expect(rowValue(region, 'Volumes')).toBe('1');
    });
  });
});

// --- refresh ---------------------------------------------------------------

describe('StorageDetailsCard refresh', () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('samples once on mount and once per refresh, never on a timer', async () => {
    const { catalog, sample } = mockBackend([SAMSUNG], [ROOT]);
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });

    const region = await card();
    await waitFor(() => {
      expect(sample).toHaveBeenCalledTimes(1);
    });

    // No hidden polling: a whole minute passes and nothing is requested.
    await vi.advanceTimersByTimeAsync(60_000);
    expect(sample).toHaveBeenCalledTimes(1);

    await user.click(within(region).getByRole('button', { name: 'Refresh storage details' }));
    await waitFor(() => {
      expect(sample).toHaveBeenCalledTimes(2);
    });

    // The inventory is discovered once and never re-enumerated.
    expect(catalog).toHaveBeenCalledTimes(1);
  });

  it('shows when the displayed figures were read', async () => {
    mockBackend([SAMSUNG], [ROOT]);

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText(/^Updated /)).toBeVisible();
    });
  });

  it('reports a backend failure without claiming the machine has no disks', async () => {
    vi.spyOn(metricsService, 'getMetricCatalog').mockRejectedValue(new Error('no runtime'));

    const region = await card();

    await waitFor(() => {
      expect(within(region).getByText(/Backend unavailable/)).toBeVisible();
    });
    expect(within(region).queryByText(/No physical storage device/)).toBeNull();
  });
});
