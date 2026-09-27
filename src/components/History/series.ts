import type { MetricDefinition } from '@/types/metrics';
import {
  CPU_TEMPERATURE_PACKAGE_KEY,
  CPU_USAGE_TOTAL,
  GPU_TEMPERATURE_CORE_KEY,
  NETWORK_RECEIVE_BYTES_KEY,
  NETWORK_TRANSMIT_BYTES_KEY,
  STORAGE_IO_READ_BYTES_KEY,
  STORAGE_IO_WRITE_BYTES_KEY,
  isGpuDeviceSource,
} from '@/types/wellknown';
import type { HistorySeriesSpec } from '@/utils/history';
import { describeAvailability } from '@/utils/metrics';
import type { NetworkInterface } from '@/utils/network';
import { isPrimaryKind, networkMetric, orderInterfaces } from '@/utils/network';
import { storageMetric } from '@/utils/storage';

/**
 * Which metrics each history section plots. Pure, so the choices — CPU Total
 * being the backend's own aggregate, an unavailable sensor never becoming a
 * series, the default network interface — are tested without rendering.
 */

/** The series of the CPU Total chart. Exported so a test can pin it. */
export const CPU_TOTAL_SERIES: readonly HistorySeriesSpec[] = [
  { ref: CPU_USAGE_TOTAL, label: 'CPU' },
];

/**
 * CPU package and GPU core temperatures on one chart.
 *
 * Only sensors the catalog declares available become series. One that is not
 * — a Windows CPU without an unprivileged sensor, a GPU whose driver exposes
 * no temperature — is listed under the chart with its reason and **never**
 * drawn as a flat zero.
 */
export function thermalSeries(catalog: readonly MetricDefinition[]): {
  readonly series: HistorySeriesSpec[];
  readonly unavailable: string[];
} {
  const packages = catalog.filter((d) => d.metric.key === CPU_TEMPERATURE_PACKAGE_KEY);
  const gpus = catalog.filter(
    (d) => d.metric.key === GPU_TEMPERATURE_CORE_KEY && isGpuDeviceSource(d.metric.sourceId),
  );
  const series: HistorySeriesSpec[] = [];
  const unavailable: string[] = [];

  const add = (definition: MetricDefinition, label: string) => {
    if (definition.availability.status === 'available') {
      series.push({ ref: definition.metric, label });
    } else {
      unavailable.push(`${label}: ${describeAvailability(definition.availability)}`);
    }
  };

  packages.forEach((definition) =>
    add(definition, packages.length === 1 ? 'CPU' : `CPU ${definition.sourceLabel}`),
  );
  gpus.forEach((definition) =>
    add(definition, gpus.length === 1 ? 'GPU' : `GPU ${definition.sourceLabel}`),
  );
  return { series, unavailable };
}

/** Read and write throughput of one storage device. */
export function storageSeries(sourceId: string): HistorySeriesSpec[] {
  return [
    { ref: storageMetric(sourceId, STORAGE_IO_READ_BYTES_KEY), label: 'Read' },
    { ref: storageMetric(sourceId, STORAGE_IO_WRITE_BYTES_KEY), label: 'Write' },
  ];
}

/** Download and upload throughput of one interface. */
export function networkSeries(sourceId: string): HistorySeriesSpec[] {
  return [
    { ref: networkMetric(sourceId, NETWORK_RECEIVE_BYTES_KEY), label: 'Download' },
    { ref: networkMetric(sourceId, NETWORK_TRANSMIT_BYTES_KEY), label: 'Upload' },
  ];
}

/**
 * The interface shown first: real hardware (Ethernet, Wi-Fi) that has
 * recently carried traffic, then any real hardware, then anything.
 *
 * A **default**, not a claim about which interface is "the Internet one":
 * PULSE cannot know that, and the selector is right there.
 */
export function defaultInterface(
  interfaces: readonly NetworkInterface[],
  recentTraffic: ReadonlyMap<string, number>,
): NetworkInterface | undefined {
  const ordered = orderInterfaces(interfaces, (sourceId) => recentTraffic.has(sourceId));
  const active = ordered
    .filter((entry) => isPrimaryKind(entry.kind) && recentTraffic.has(entry.sourceId))
    .sort(
      (left, right) =>
        (recentTraffic.get(right.sourceId) ?? 0) - (recentTraffic.get(left.sourceId) ?? 0),
    );
  return active[0] ?? ordered.find((entry) => isPrimaryKind(entry.kind)) ?? ordered[0];
}
