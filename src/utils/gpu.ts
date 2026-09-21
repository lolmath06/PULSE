import type { Availability, MetricDefinition, MetricRef, MetricSample } from '@/types/metrics';
import {
  GPU_PERFORMANCE_KEYS,
  GPU_PER_DEVICE_KEYS,
  GPU_THERMAL_KEYS,
  isGpuDeviceSource,
  metricRefId,
} from '@/types/wellknown';

/**
 * Deriving the GPU list from the metric catalog.
 *
 * **Nothing here knows how many GPUs a machine has, or what they are.** The
 * backend publishes seven metrics per device, and this module reads the catalog
 * back to discover which devices exist. A laptop with one card, a workstation
 * with four, and a headless server with none all go through the same code.
 *
 * Equally, nothing here parses a `SourceId`. Whether it encodes an NVML UUID, a
 * PCI address or a device-model tuple is the backend's business — the interface
 * treats it as an opaque, stable key, which is exactly what keeps a saved
 * dashboard working when a machine gains a proper driver and its identity
 * mechanism improves.
 *
 * Pure and free of React, so the ordering and filtering rules are tested
 * directly rather than through a rendered component.
 */

/** One GPU discovered in the catalog. */
export interface GpuDevice {
  /** Its stable source identifier. Opaque to the interface. */
  readonly sourceId: string;
  /** The name the backend gave it, e.g. `NVIDIA GeForce RTX 4070`. */
  readonly label: string;
}

/**
 * Finds every GPU the catalog describes.
 *
 * Ordered by `sourceId`, which is the order the backend's own catalog uses, so
 * the card's device order is deterministic across refreshes and restarts
 * instead of following driver enumeration.
 *
 * A device is listed as soon as it appears in *any* per-GPU metric, so a card
 * whose telemetry is entirely unsupported still gets a row — which is the whole
 * point: a recognised GPU with no backend must be visible, not hidden.
 */
export function discoverGpus(catalog: readonly MetricDefinition[]): GpuDevice[] {
  const found = new Map<string, GpuDevice>();

  for (const definition of catalog) {
    if (!GPU_PER_DEVICE_KEYS.includes(definition.metric.key)) continue;

    const { sourceId } = definition.metric;
    if (!isGpuDeviceSource(sourceId) || found.has(sourceId)) continue;

    // The backend owns the label, so the two can never disagree.
    found.set(sourceId, { sourceId, label: definition.sourceLabel });
  }

  return [...found.values()].sort((left, right) => left.sourceId.localeCompare(right.sourceId));
}

/**
 * The per-GPU metrics to request for a set of devices.
 *
 * Only what the card displays: seven references per device, and nothing for
 * devices the catalog does not describe.
 */
export function gpuMetrics(devices: readonly GpuDevice[]): MetricRef[] {
  return devices.flatMap((device) =>
    GPU_PER_DEVICE_KEYS.map((key) => ({ key, sourceId: device.sourceId })),
  );
}

/** Builds a per-GPU reference. */
export function gpuMetric(sourceId: string, key: string): MetricRef {
  return { key, sourceId };
}

/**
 * Shortens a device label when several devices share it.
 *
 * Two identical cards carry the same name, and the user needs to tell the rows
 * apart. This appends a positional hint **for display only** — the identity
 * underneath is untouched, so a saved widget still points at the right card.
 */
export function disambiguateLabels(devices: readonly GpuDevice[]): string[] {
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

// --- explaining an adapter with no performance telemetry --------------------

/**
 * What a device's two halves of telemetry are currently doing.
 *
 * The distinction exists because they fail independently, and because the
 * sentence a user needs is different in each case. A GeForce card running the
 * open-source `nouveau` driver is **detected, named and correctly identified**;
 * what it lacks is the vendor library that reports utilisation, VRAM and
 * clocks. Its thermal sensor may work perfectly at the same time.
 */
export interface GpuTelemetryState {
  /** True as soon as one performance metric produced a value. */
  readonly performanceAvailable: boolean;
  /** True as soon as one thermal or cooling metric produced a value. */
  readonly thermalAvailable: boolean;
  /**
   * The backend's own explanation for the missing performance telemetry, or
   * `null` when nothing is missing. Never invented here: it is the reason the
   * provider attached to the sample.
   */
  readonly reason: string | null;
}

/**
 * How specific an availability reason is, for choosing which to surface.
 *
 * A device can report several different reasons at once — an unsupported clock
 * domain beside a missing driver. The one worth showing is the one the user can
 * act on, so a permission problem outranks an unsupported sensor, and a
 * transient hiccup ranks last because it will probably be gone next refresh.
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

/** The human-readable reason carried by an availability, without its prefix. */
function reasonText(availability: Availability): string | null {
  switch (availability.status) {
    case 'unsupported':
    case 'notDetected':
    case 'permissionDenied':
    case 'temporarilyUnavailable':
    case 'notRegistered':
      return availability.reason;
    case 'providerError':
      return availability.error.message;
    default:
      return null;
  }
}

/** Whether a sample carries a usable number. */
function hasValue(sample: MetricSample | undefined): boolean {
  return sample?.value != null && sample.value.type === 'number';
}

/**
 * Splits one device's samples into "performance works" and "thermals work".
 *
 * Pure, so the message the card shows is tested directly rather than through a
 * rendered component — including the partial case that matters most here: no
 * performance counters, but a working temperature sensor.
 */
export function gpuTelemetryState(
  samples: ReadonlyMap<string, MetricSample>,
  sourceId: string,
): GpuTelemetryState {
  const sampleFor = (key: string) => samples.get(metricRefId({ key, sourceId }));

  const performance = GPU_PERFORMANCE_KEYS.map(sampleFor);
  const performanceAvailable = performance.some(hasValue);
  const thermalAvailable = GPU_THERMAL_KEYS.map(sampleFor).some(hasValue);

  if (performanceAvailable) {
    return { performanceAvailable, thermalAvailable, reason: null };
  }

  // Requested metrics that produced neither a value nor an explanation leave
  // `reason` null, and the card falls back to a generic sentence rather than
  // inventing a cause.
  const best = performance
    .filter((sample): sample is MetricSample => sample !== undefined)
    .map((sample) => sample.availability)
    .sort((left, right) => reasonRank(left) - reasonRank(right))
    .map(reasonText)
    .find((text): text is string => text !== null && text.length > 0);

  return { performanceAvailable, thermalAvailable, reason: best ?? null };
}
