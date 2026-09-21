import { describe, expect, it } from 'vitest';
import type { Availability, MetricDefinition, MetricSample } from '@/types/metrics';
import {
  STORAGE_HEALTH_KEYS,
  STORAGE_HEALTH_PERCENTAGE_USED_KEY,
  STORAGE_HEALTH_TEMPERATURE_KEY,
  STORAGE_IO_KEYS,
  STORAGE_IO_READ_BYTES_KEY,
  STORAGE_IO_READ_IOPS_KEY,
  STORAGE_PER_DEVICE_KEYS,
  STORAGE_PER_VOLUME_KEYS,
  STORAGE_VOLUME_CAPACITY_TOTAL_KEY,
  metricRefId,
} from '@/types/wellknown';
import {
  disambiguateStorageLabels,
  discoverStorageDevices,
  discoverStorageVolumes,
  storageMetric,
  storageMetrics,
  storageTelemetryState,
  volumeHasCapacity,
  volumesByDevice,
} from '@/utils/storage';

const AVAILABLE: Availability = { status: 'available' };

const NVME = 'storage:wwid-eui.002538b331b36d03';
const USB = 'storage:wwid-t10.intenso-scsi-2019131398ab5';

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

function deviceEntries(sourceId: string, label: string): MetricDefinition[] {
  return STORAGE_PER_DEVICE_KEYS.map((key) => definition(key, sourceId, label));
}

function volumeEntries(sourceId: string, label: string): MetricDefinition[] {
  return STORAGE_PER_VOLUME_KEYS.map((key) => definition(key, sourceId, label));
}

function sample(sourceId: string, key: string, value: number | Availability): MetricSample {
  const metric = storageMetric(sourceId, key);

  return typeof value === 'number'
    ? { metric, timestamp: 1, value: { type: 'number', value }, availability: AVAILABLE }
    : { metric, timestamp: 1, value: null, availability: value };
}

function sampleMap(samples: readonly MetricSample[]): ReadonlyMap<string, MetricSample> {
  return new Map(samples.map((entry) => [metricRefId(entry.metric), entry]));
}

// --- discovery -------------------------------------------------------------

describe('discoverStorageDevices', () => {
  it('finds no devices in an empty catalog', () => {
    expect(discoverStorageDevices([])).toEqual([]);
  });

  it('finds one device from its per-device metrics', () => {
    const devices = discoverStorageDevices(deviceEntries(NVME, 'Samsung SSD · NVMe'));

    expect(devices).toEqual([{ sourceId: NVME, label: 'Samsung SSD · NVMe' }]);
  });

  it('finds several devices and orders them deterministically', () => {
    // Declared in one order, discovered in another: the card's device order
    // must not follow whichever disk the kernel probed first.
    const forward = discoverStorageDevices([
      ...deviceEntries(USB, 'Intenso · USB'),
      ...deviceEntries(NVME, 'Samsung SSD · NVMe'),
    ]);
    const backward = discoverStorageDevices([
      ...deviceEntries(NVME, 'Samsung SSD · NVMe'),
      ...deviceEntries(USB, 'Intenso · USB'),
    ]);

    expect(forward.map((device) => device.sourceId)).toEqual([NVME, USB]);
    expect(forward).toEqual(backward);
  });

  it('lists a device whose telemetry is entirely unavailable', () => {
    // A recognised drive with no reachable counters or health must be visible,
    // not hidden — which is the whole point of keeping the definitions.
    const catalog = STORAGE_PER_DEVICE_KEYS.map((key) => ({
      ...definition(key, USB, 'Intenso · USB'),
      availability: { status: 'unsupported', reason: 'behind a bridge' } as Availability,
    }));

    expect(discoverStorageDevices(catalog)).toHaveLength(1);
  });

  it('never mistakes the machine-wide source for a device', () => {
    const catalog = [
      definition('storage.device.count', 'storage:system', 'Storage'),
      definition('storage.volume.count', 'storage:system', 'Storage'),
      ...deviceEntries(NVME, 'Samsung SSD · NVMe'),
    ];

    expect(discoverStorageDevices(catalog).map((device) => device.sourceId)).toEqual([NVME]);
  });

  it('never mistakes a volume for a device', () => {
    // Two source kinds, deliberately: a filesystem is not a disk.
    const catalog = [
      ...deviceEntries(NVME, 'Samsung SSD · NVMe'),
      ...volumeEntries(`volume:${NVME.slice('storage:'.length)}-p8`, '/'),
    ];

    expect(discoverStorageDevices(catalog).map((device) => device.sourceId)).toEqual([NVME]);
  });

  it('ignores metrics from other families on a storage-shaped source', () => {
    const catalog = [
      definition('cpu.usage.total', 'cpu:system', 'Processor'),
      definition('gpu.count', 'gpu:system', 'Graphics'),
    ];

    expect(discoverStorageDevices(catalog)).toEqual([]);
  });
});

describe('discoverStorageVolumes', () => {
  it('finds no volumes in an empty catalog', () => {
    expect(discoverStorageVolumes([])).toEqual([]);
  });

  it('orders volumes by the path a user recognises', () => {
    // By label, not by opaque source id: a user scanning the list looks for
    // `/` and `/boot`, and ordering by identifier would scatter them.
    const catalog = [
      ...volumeEntries('volume:zzz-p1', '/home'),
      ...volumeEntries('volume:aaa-p1', '/boot'),
      ...volumeEntries('volume:mmm-p1', '/'),
    ];

    expect(discoverStorageVolumes(catalog).map((volume) => volume.label)).toEqual([
      '/',
      '/boot',
      '/home',
    ]);
  });

  it('keeps two volumes that share a label apart by identity', () => {
    const catalog = [
      ...volumeEntries('volume:aaa-p1', '/mnt/data'),
      ...volumeEntries('volume:bbb-p1', '/mnt/data'),
    ];

    const volumes = discoverStorageVolumes(catalog);

    expect(volumes).toHaveLength(2);
    expect(volumes[0]?.sourceId).not.toEqual(volumes[1]?.sourceId);
  });

  it('never mistakes a device for a volume', () => {
    expect(discoverStorageVolumes(deviceEntries(NVME, 'Samsung SSD · NVMe'))).toEqual([]);
  });
});

// --- requests --------------------------------------------------------------

describe('storageMetrics', () => {
  it('requests nothing for an empty inventory', () => {
    expect(storageMetrics([], [])).toEqual([]);
  });

  it('requests every per-device and per-volume key exactly once', () => {
    const request = storageMetrics(
      [
        { sourceId: NVME, label: 'a' },
        { sourceId: USB, label: 'b' },
      ],
      [{ sourceId: 'volume:aaa-p1', label: '/' }],
    );

    expect(request).toHaveLength(
      STORAGE_PER_DEVICE_KEYS.length * 2 + STORAGE_PER_VOLUME_KEYS.length,
    );
    expect(new Set(request.map(metricRefId)).size).toBe(request.length);
  });
});

describe('disambiguateStorageLabels', () => {
  it('leaves distinct labels alone', () => {
    expect(
      disambiguateStorageLabels([
        { sourceId: NVME, label: 'Samsung SSD · NVMe' },
        { sourceId: USB, label: 'Intenso · USB' },
      ]),
    ).toEqual(['Samsung SSD · NVMe', 'Intenso · USB']);
  });

  it('numbers two identical drives for display only', () => {
    const devices = [
      { sourceId: 'storage:serial-s111', label: 'Twin SSD · NVMe' },
      { sourceId: 'storage:serial-s222', label: 'Twin SSD · NVMe' },
    ];

    expect(disambiguateStorageLabels(devices)).toEqual([
      'Twin SSD · NVMe #1',
      'Twin SSD · NVMe #2',
    ]);
    // The identities are untouched, so a saved widget still points at the
    // right disk.
    expect(devices[0]?.sourceId).toBe('storage:serial-s111');
  });
});

// --- attribution -----------------------------------------------------------

describe('volumesByDevice', () => {
  const nvmeInstance = NVME.slice('storage:'.length);
  const usbInstance = USB.slice('storage:'.length);

  const devices = [
    { sourceId: NVME, label: 'Samsung SSD · NVMe' },
    { sourceId: USB, label: 'Intenso · USB' },
  ];

  it('nests each volume under the disk its identity was derived from', () => {
    const volumes = [
      { sourceId: `volume:${nvmeInstance}-p8`, label: '/' },
      { sourceId: `volume:${nvmeInstance}-p7`, label: '/boot' },
      { sourceId: `volume:${usbInstance}-p4`, label: '/mnt/data' },
    ];

    const { byDevice, unattributed } = volumesByDevice(devices, volumes);

    expect(byDevice.get(NVME)?.map((volume) => volume.label)).toEqual(['/', '/boot']);
    expect(byDevice.get(USB)?.map((volume) => volume.label)).toEqual(['/mnt/data']);
    expect(unattributed).toEqual([]);
  });

  it('leaves a volume with no derivable parent unattributed', () => {
    // An NFS export, a volume spanning two disks: attributing it to whichever
    // disk looks plausible would be worse than admitting the link is unknown.
    const volumes = [{ sourceId: 'volume:mm-0-99', label: '/mnt/share' }];

    const { byDevice, unattributed } = volumesByDevice(devices, volumes);

    expect(unattributed.map((volume) => volume.label)).toEqual(['/mnt/share']);
    expect(byDevice.get(NVME)).toEqual([]);
    expect(byDevice.get(USB)).toEqual([]);
  });

  it('never lets one device claim another whose instance it prefixes', () => {
    // `storage:serial-s1` is a prefix of `storage:serial-s11`, and a naive
    // prefix match would hand the second disk's volumes to the first.
    const prefixed = [
      { sourceId: 'storage:serial-s1', label: 'A' },
      { sourceId: 'storage:serial-s11', label: 'B' },
    ];
    const volumes = [{ sourceId: 'volume:serial-s11-p1', label: '/data' }];

    const { byDevice } = volumesByDevice(prefixed, volumes);

    expect(byDevice.get('storage:serial-s11')?.map((volume) => volume.label)).toEqual(['/data']);
    expect(byDevice.get('storage:serial-s1')).toEqual([]);
  });

  it('refuses a partition suffix that is not a number', () => {
    const volumes = [{ sourceId: `volume:${nvmeInstance}-pX`, label: '/odd' }];

    expect(volumesByDevice(devices, volumes).unattributed).toHaveLength(1);
  });

  it('gives every device an entry even when it holds no volume', () => {
    const { byDevice } = volumesByDevice(devices, []);

    expect([...byDevice.keys()].sort()).toEqual([NVME, USB].sort());
    expect(byDevice.get(NVME)).toEqual([]);
  });
});

// --- telemetry state -------------------------------------------------------

describe('storageTelemetryState', () => {
  it('reports both halves available on a fully telemetered drive', () => {
    const samples = sampleMap([
      ...STORAGE_IO_KEYS.map((key) => sample(NVME, key, 1)),
      ...STORAGE_HEALTH_KEYS.map((key) => sample(NVME, key, 1)),
    ]);

    const state = storageTelemetryState(samples, NVME);

    expect(state.ioAvailable).toBe(true);
    expect(state.healthAvailable).toBe(true);
    expect(state.ioAwaitingBaseline).toBe(false);
    expect(state.healthReason).toBeNull();
  });

  it('recognises a first sample with no baseline yet', () => {
    // Every I/O metric temporarily unavailable at once is exactly the shape a
    // fresh launch produces, and it is not a failure.
    const waiting: Availability = {
      status: 'temporarilyUnavailable',
      reason: 'disk activity is measured between two samples; waiting for the next one',
    };
    const samples = sampleMap(STORAGE_IO_KEYS.map((key) => sample(NVME, key, waiting)));

    const state = storageTelemetryState(samples, NVME);

    expect(state.ioAvailable).toBe(false);
    expect(state.ioAwaitingBaseline).toBe(true);
  });

  it('does not call an unsupported counter a missing baseline', () => {
    // A disk Windows keeps no counters for is a different situation, and the
    // "refresh to measure it" hint would be a lie.
    const unsupported: Availability = { status: 'unsupported', reason: 'no counters' };
    const samples = sampleMap(STORAGE_IO_KEYS.map((key) => sample(NVME, key, unsupported)));

    const state = storageTelemetryState(samples, NVME);

    expect(state.ioAvailable).toBe(false);
    expect(state.ioAwaitingBaseline).toBe(false);
  });

  it('counts an idle disk as measured, not as waiting', () => {
    // Zero throughput over a real interval is a measurement.
    const samples = sampleMap([
      sample(NVME, STORAGE_IO_READ_BYTES_KEY, 0),
      sample(NVME, STORAGE_IO_READ_IOPS_KEY, 0),
    ]);

    const state = storageTelemetryState(samples, NVME);

    expect(state.ioAvailable).toBe(true);
    expect(state.ioAwaitingBaseline).toBe(false);
  });

  it('keeps health available when only the temperature came back', () => {
    // The unprivileged Fedora case: the kernel publishes a composite
    // temperature and the rest of the log needs root.
    const denied: Availability = { status: 'permissionDenied', reason: 'needs /dev/nvme0' };
    const samples = sampleMap([
      sample(NVME, STORAGE_HEALTH_TEMPERATURE_KEY, 52.85),
      ...STORAGE_HEALTH_KEYS.filter((key) => key !== STORAGE_HEALTH_TEMPERATURE_KEY).map((key) =>
        sample(NVME, key, denied),
      ),
    ]);

    expect(storageTelemetryState(samples, NVME).healthAvailable).toBe(true);
  });

  it('surfaces the most actionable reason when no health value came back', () => {
    // A permission problem the user can act on outranks an unsupported
    // interface they cannot.
    const samples = sampleMap([
      sample(NVME, STORAGE_HEALTH_TEMPERATURE_KEY, {
        status: 'unsupported',
        reason: 'no sensor',
      }),
      sample(NVME, STORAGE_HEALTH_PERCENTAGE_USED_KEY, {
        status: 'permissionDenied',
        reason: 'needs privileged access to /dev/nvme0',
      }),
    ]);

    const state = storageTelemetryState(samples, NVME);

    expect(state.healthAvailable).toBe(false);
    expect(state.healthReason).toBe('needs privileged access to /dev/nvme0');
  });

  it('reports no reason at all when the samples never arrived', () => {
    const state = storageTelemetryState(new Map(), NVME);

    expect(state.ioAvailable).toBe(false);
    expect(state.healthAvailable).toBe(false);
    expect(state.ioAwaitingBaseline).toBe(false);
    expect(state.healthReason).toBeNull();
  });
});

describe('volumeHasCapacity', () => {
  it('is true for a sized filesystem', () => {
    const samples = sampleMap([sample('volume:aaa-p1', STORAGE_VOLUME_CAPACITY_TOTAL_KEY, 1024)]);

    expect(volumeHasCapacity(samples, 'volume:aaa-p1')).toBe(true);
  });

  it('is false for a filesystem with no reported size', () => {
    // No bar rather than an empty one, which would read as "0 % used".
    const samples = sampleMap([
      sample('volume:aaa-p1', STORAGE_VOLUME_CAPACITY_TOTAL_KEY, {
        status: 'notDetected',
        reason: 'no size',
      }),
    ]);

    expect(volumeHasCapacity(samples, 'volume:aaa-p1')).toBe(false);
    expect(volumeHasCapacity(new Map(), 'volume:aaa-p1')).toBe(false);
  });

  it('is false for a zero-sized filesystem', () => {
    const samples = sampleMap([sample('volume:aaa-p1', STORAGE_VOLUME_CAPACITY_TOTAL_KEY, 0)]);

    expect(volumeHasCapacity(samples, 'volume:aaa-p1')).toBe(false);
  });
});
