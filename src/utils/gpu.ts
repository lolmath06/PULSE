import type { MetricDefinition, MetricRef } from '@/types/metrics';
import { GPU_PER_DEVICE_KEYS, isGpuDeviceSource } from '@/types/wellknown';

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
