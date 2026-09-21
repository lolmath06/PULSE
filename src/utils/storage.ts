import type { Availability, MetricDefinition, MetricRef, MetricSample } from '@/types/metrics';
import {
  STORAGE_HEALTH_KEYS,
  STORAGE_IO_KEYS,
  STORAGE_PER_DEVICE_KEYS,
  STORAGE_PER_VOLUME_KEYS,
  STORAGE_VOLUME_CAPACITY_TOTAL_KEY,
  isStorageDeviceSource,
  isStorageVolumeSource,
  metricRefId,
} from '@/types/wellknown';

/**
 * Deriving the storage inventory from the metric catalog.
 *
 * **Nothing here knows how many disks a machine has, what they are, or how
 * they relate to its filesystems.** The backend publishes thirteen metrics per
 * device and four per volume, and this module reads the catalog back to
 * discover which of each exist. A laptop with one NVMe drive, a workstation
 * with four disks and a VM with a single virtio device all go through the same
 * code.
 *
 * Equally, nothing here parses a `SourceId`. Whether it encodes a WWN, a
 * serial number, a volume GUID or a kernel name is the backend's business —
 * the interface treats it as an opaque, stable key, which is exactly what
 * keeps a saved dashboard working when a machine gains a better identifier.
 *
 * Pure and free of React, so the grouping, ordering and attribution rules are
 * tested directly rather than through a rendered component.
 */

/** One physical storage device discovered in the catalog. */
export interface StorageDevice {
  /** Its stable source identifier. Opaque to the interface. */
  readonly sourceId: string;
  /** The name the backend gave it, e.g. `Samsung SSD 990 PRO · NVMe`. */
  readonly label: string;
}

/** One filesystem discovered in the catalog. */
export interface StorageVolume {
  /** Its stable source identifier. Opaque to the interface. */
  readonly sourceId: string;
  /** The name the backend gave it — usually its primary mount point. */
  readonly label: string;
}

/**
 * Finds every physical storage device the catalog describes.
 *
 * Ordered by `sourceId`, which is the order the backend's own catalog uses, so
 * the device order is deterministic across refreshes and restarts instead of
 * following enumeration order.
 *
 * A device is listed as soon as it appears in *any* per-device metric, so a
 * disk whose health and I/O are both unavailable still gets a row — which is
 * the point: a recognised drive with no reachable telemetry must be visible,
 * not hidden.
 */
export function discoverStorageDevices(catalog: readonly MetricDefinition[]): StorageDevice[] {
  const found = new Map<string, StorageDevice>();

  for (const definition of catalog) {
    if (!STORAGE_PER_DEVICE_KEYS.includes(definition.metric.key)) continue;

    const { sourceId } = definition.metric;
    if (!isStorageDeviceSource(sourceId) || found.has(sourceId)) continue;

    // The backend owns the label, so the two can never disagree.
    found.set(sourceId, { sourceId, label: definition.sourceLabel });
  }

  return [...found.values()].sort((left, right) => left.sourceId.localeCompare(right.sourceId));
}

/**
 * Finds every volume the catalog describes.
 *
 * Ordered by **label** rather than by `sourceId`: a user scanning the list
 * looks for `/`, `/boot`, `C:` — the paths they know — and ordering by an
 * opaque identifier would scatter them arbitrarily. The identity underneath is
 * untouched.
 */
export function discoverStorageVolumes(catalog: readonly MetricDefinition[]): StorageVolume[] {
  const found = new Map<string, StorageVolume>();

  for (const definition of catalog) {
    if (!STORAGE_PER_VOLUME_KEYS.includes(definition.metric.key)) continue;

    const { sourceId } = definition.metric;
    if (!isStorageVolumeSource(sourceId) || found.has(sourceId)) continue;

    found.set(sourceId, { sourceId, label: definition.sourceLabel });
  }

  return [...found.values()].sort(
    (left, right) =>
      left.label.localeCompare(right.label) || left.sourceId.localeCompare(right.sourceId),
  );
}

/**
 * The metrics to request for a set of devices and volumes.
 *
 * Only what the card displays: thirteen references per device and four per
 * volume, and nothing for sources the catalog does not describe.
 */
export function storageMetrics(
  devices: readonly StorageDevice[],
  volumes: readonly StorageVolume[],
): MetricRef[] {
  return [
    ...devices.flatMap((device) =>
      STORAGE_PER_DEVICE_KEYS.map((key) => ({ key, sourceId: device.sourceId })),
    ),
    ...volumes.flatMap((volume) =>
      STORAGE_PER_VOLUME_KEYS.map((key) => ({ key, sourceId: volume.sourceId })),
    ),
  ];
}

/** Builds a reference for one storage source. */
export function storageMetric(sourceId: string, key: string): MetricRef {
  return { key, sourceId };
}

/**
 * Shortens a device label when several devices share it.
 *
 * Two identical drives carry the same name, and the user needs to tell the
 * rows apart. This appends a positional hint **for display only** — the
 * identity underneath is untouched, so a saved widget still points at the
 * right disk.
 */
export function disambiguateStorageLabels(devices: readonly StorageDevice[]): string[] {
  const counts = new Map<string, number>();
  for (const device of devices) {
    counts.set(device.label, (counts.get(device.label) ?? 0) + 1);
  }

  const seen = new Map<string, number>();

  return devices.map((device) => {
    if ((counts.get(device.label) ?? 0) <= 1) return device.label;

    const position = (seen.get(device.label) ?? 0) + 1;
    seen.set(device.label, position);

    return `${device.label} #${position}`;
  });
}

// --- attributing volumes to devices ----------------------------------------

/**
 * Which device each volume lives on, as the **backend** determined it.
 *
 * The attribution cannot be derived here, and deliberately is not attempted:
 * it comes from the volume's own device number on Fedora and from its disk
 * extents on Windows. What the interface can see is the catalog, so the link
 * is carried through the one place both sides already agree on — the volume's
 * `sourceId`, whose instance the backend derives from its parent's when, and
 * only when, it could correlate them.
 *
 * A volume whose identity was *not* derived from a device — an NFS export, a
 * volume spanning two disks, anything the backend refused to guess about — has
 * no parent here either, and belongs under "Other volumes" rather than being
 * attached to whichever disk looks plausible.
 */
export function volumesByDevice(
  devices: readonly StorageDevice[],
  volumes: readonly StorageVolume[],
): {
  readonly byDevice: ReadonlyMap<string, StorageVolume[]>;
  readonly unattributed: StorageVolume[];
} {
  const byDevice = new Map<string, StorageVolume[]>();
  for (const device of devices) byDevice.set(device.sourceId, []);

  const unattributed: StorageVolume[] = [];

  for (const volume of volumes) {
    const parent = parentDeviceOf(volume.sourceId, devices);

    if (parent) {
      byDevice.get(parent)?.push(volume);
    } else {
      unattributed.push(volume);
    }
  }

  return { byDevice, unattributed };
}

/**
 * The device a volume's identifier was derived from, if any.
 *
 * The backend builds a correlated volume's source as `volume:<the device's
 * instance>-p<partition>`. Matching on that prefix is the *only* structural
 * assumption the interface makes about a `SourceId`, it is one the backend
 * documents and tests, and it fails closed: a volume whose instance does not
 * start with a known device's instance is reported as unattributed rather than
 * attached to a near-match.
 */
function parentDeviceOf(volumeSourceId: string, devices: readonly StorageDevice[]): string | null {
  const instance = volumeSourceId.slice('volume:'.length);

  // Longest instance first, so a device whose instance is a prefix of
  // another's cannot claim the other's volumes.
  const candidates = [...devices].sort(
    (left, right) => right.sourceId.length - left.sourceId.length,
  );

  for (const device of candidates) {
    const deviceInstance = device.sourceId.slice('storage:'.length);
    const suffix = instance.startsWith(`${deviceInstance}-p`)
      ? instance.slice(deviceInstance.length + 2)
      : null;

    if (suffix !== null && /^(0|[1-9][0-9]*)$/.test(suffix)) {
      return device.sourceId;
    }
  }

  return null;
}

// --- explaining a device with no telemetry ---------------------------------

/**
 * What a device's two independent halves of telemetry are currently doing.
 *
 * The distinction exists because they fail for different reasons, and because
 * the sentence a user needs is different in each case. A USB disk has
 * perfectly good I/O counters and unreachable SMART data; an NVMe drive on an
 * unelevated PULSE has working counters, a working temperature, and five
 * health values the operating system refuses.
 */
export interface StorageTelemetryState {
  /** True as soon as one activity metric produced a value. */
  readonly ioAvailable: boolean;
  /** True as soon as one health metric produced a value. */
  readonly healthAvailable: boolean;
  /**
   * True when the device's activity metrics are absent only because no
   * baseline exists yet — the state a fresh launch is in, which resolves
   * itself on the next refresh and is not a failure.
   */
  readonly ioAwaitingBaseline: boolean;
  /**
   * The backend's own explanation for the missing health values, or `null`
   * when none are missing. Never invented here.
   */
  readonly healthReason: string | null;
}

/**
 * How specific an availability reason is, for choosing which to surface.
 *
 * A device can report several reasons at once. The one worth showing is the
 * one the user can act on, so a permission problem outranks an unsupported
 * interface, and a transient state ranks last because it will probably be gone
 * next refresh.
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

/** Summarises one device's telemetry from the samples that came back. */
export function storageTelemetryState(
  samples: ReadonlyMap<string, MetricSample>,
  sourceId: string,
): StorageTelemetryState {
  const sampleOf = (key: string): MetricSample | undefined =>
    samples.get(metricRefId(storageMetric(sourceId, key)));

  const hasValue = (key: string): boolean => sampleOf(key)?.value?.type === 'number';

  const ioAvailable = STORAGE_IO_KEYS.some(hasValue);
  const healthAvailable = STORAGE_HEALTH_KEYS.some(hasValue);

  // "Waiting for a second sample" is `temporarilyUnavailable` on every I/O
  // metric at once, which is exactly the shape a fresh launch produces.
  const ioAwaitingBaseline =
    !ioAvailable &&
    STORAGE_IO_KEYS.every((key) => sampleOf(key)?.availability.status === 'temporarilyUnavailable');

  let best: { rank: number; reason: string } | null = null;

  if (!healthAvailable) {
    for (const key of STORAGE_HEALTH_KEYS) {
      const sample = sampleOf(key);
      if (!sample) continue;

      const reason = reasonOf(sample.availability);
      if (reason === null) continue;

      const rank = reasonRank(sample.availability);
      if (best === null || rank < best.rank) best = { rank, reason };
    }
  }

  return {
    ioAvailable,
    healthAvailable,
    ioAwaitingBaseline,
    healthReason: best?.reason ?? null,
  };
}

/**
 * Whether the catalog says this volume has a size at all.
 *
 * Used to decide whether a usage bar can be drawn. A volume whose total is
 * unavailable gets no bar rather than an empty one, which would read as "0 %
 * used" on a filesystem nothing measured.
 */
export function volumeHasCapacity(
  samples: ReadonlyMap<string, MetricSample>,
  sourceId: string,
): boolean {
  const sample = samples.get(
    metricRefId(storageMetric(sourceId, STORAGE_VOLUME_CAPACITY_TOTAL_KEY)),
  );

  return sample?.value?.type === 'number' && sample.value.value > 0;
}
