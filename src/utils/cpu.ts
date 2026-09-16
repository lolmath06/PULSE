import type { MetricDefinition, MetricRef, MetricSample } from '@/types/metrics';
import {
  CPU_FREQUENCY_CURRENT_KEY,
  CPU_FREQUENCY_MAX_KEY,
  CPU_USAGE_LOGICAL_KEY,
  cpuLogicalOrdinal,
  cpuLogicalSourceId,
  metricRefId,
} from '@/types/wellknown';

/**
 * Deriving the CPU layout from the metric catalog.
 *
 * **Nothing here knows how many processors a machine has.** The backend
 * publishes three metrics per logical processor, and this module reads the
 * catalog back to discover which processors exist. A four-thread virtual
 * machine and a 128-thread workstation go through exactly the same code; there
 * is no list of `CPU 0`…`CPU 31` anywhere in the interface.
 *
 * Pure and free of React, so the ordering and filtering rules below are tested
 * directly rather than through a rendered component.
 */

/** One logical processor discovered in the catalog. */
export interface LogicalProcessor {
  /** The PULSE ordinal, e.g. `7` for `cpu:logical-7`. */
  readonly ordinal: number;
  /** Its canonical source identifier. */
  readonly sourceId: string;
  /** The label the backend gave it, e.g. `CPU 7`. */
  readonly label: string;
}

/** The three metrics PULSE publishes for every logical processor. */
const PER_PROCESSOR_KEYS: readonly string[] = [
  CPU_USAGE_LOGICAL_KEY,
  CPU_FREQUENCY_CURRENT_KEY,
  CPU_FREQUENCY_MAX_KEY,
];

/**
 * Finds every logical processor the catalog describes.
 *
 * Sorted **numerically by ordinal**, which is the entire point: the catalog
 * arrives ordered by the backend's `(key, sourceId)` string comparison, so a
 * 32-thread machine hands us `logical-1`, `logical-10`, `logical-11`,
 * `logical-2`. Rendering that order would scramble the table. Parsing the
 * ordinal out and sorting on the number is what produces 0, 1, 2, …, 10, 11.
 *
 * A processor is listed as soon as it appears in *any* per-processor metric,
 * so one whose frequency is unsupported still gets a row showing its usage.
 * Sources that are not logical processors — `cpu:system` above all — are
 * ignored.
 */
export function discoverLogicalProcessors(
  catalog: readonly MetricDefinition[],
): LogicalProcessor[] {
  const found = new Map<number, LogicalProcessor>();

  for (const definition of catalog) {
    if (!PER_PROCESSOR_KEYS.includes(definition.metric.key)) continue;

    const ordinal = cpuLogicalOrdinal(definition.metric.sourceId);
    if (ordinal === null || found.has(ordinal)) continue;

    found.set(ordinal, {
      ordinal,
      sourceId: definition.metric.sourceId,
      // The backend owns the label; the UI does not invent "CPU n" itself, so
      // the two can never disagree.
      label: definition.sourceLabel,
    });
  }

  return [...found.values()].sort((left, right) => left.ordinal - right.ordinal);
}

/**
 * The per-processor metrics to request for a set of processors.
 *
 * Only what the card actually displays: three references per processor, and
 * nothing for processors the catalog does not describe. Asking for the whole
 * catalog would pull in memory metrics another card already owns.
 */
export function logicalProcessorMetrics(processors: readonly LogicalProcessor[]): MetricRef[] {
  return processors.flatMap((processor) =>
    PER_PROCESSOR_KEYS.map((key) => ({
      key,
      sourceId: cpuLogicalSourceId(processor.ordinal),
    })),
  );
}

/** A sample's numeric value, or `null` when it carries none. */
export function numberOf(
  samples: ReadonlyMap<string, MetricSample>,
  metric: MetricRef,
): number | null {
  const sample = samples.get(metricRefId(metric));
  if (!sample || sample.value === null || sample.value.type !== 'number') return null;

  return sample.value.value;
}

/** Looks a sample up by reference. */
export function sampleOf(
  samples: ReadonlyMap<string, MetricSample>,
  metric: MetricRef,
): MetricSample | undefined {
  return samples.get(metricRefId(metric));
}

/** Builds a per-processor reference. */
export function processorMetric(ordinal: number, key: string): MetricRef {
  return { key, sourceId: cpuLogicalSourceId(ordinal) };
}
