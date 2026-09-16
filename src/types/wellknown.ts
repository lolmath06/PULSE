import type { MetricRef, SourceId } from '@/types/metrics';

/**
 * The metric references PULSE currently ships.
 *
 * Mirrors `src-tauri/src/metrics/wellknown/`. These strings are the stable
 * contract a saved dashboard will store, and they are identical on Fedora and
 * Windows — only the backing provider differs, which the UI never sees.
 *
 * A Rust contract test asserts both platforms declare exactly these.
 */

// --- metric keys ----------------------------------------------------------
// Keys are referenced on their own wherever a metric exists once per logical
// processor, because there the source is discovered at runtime rather than
// known in advance.

/** Aggregate CPU utilisation, 0–100 percent. */
export const CPU_USAGE_TOTAL_KEY = 'cpu.usage.total';
/** Utilisation of one logical processor, 0–100 percent. */
export const CPU_USAGE_LOGICAL_KEY = 'cpu.usage.logical';
/** Clock the OS currently reports for one logical processor, in hertz. */
export const CPU_FREQUENCY_CURRENT_KEY = 'cpu.frequency.current';
/** Maximum clock the platform reports for one logical processor, in hertz. */
export const CPU_FREQUENCY_MAX_KEY = 'cpu.frequency.max';
/** Number of logical processors (hardware threads). */
export const CPU_COUNT_LOGICAL_KEY = 'cpu.count.logical';
/** Number of physical execution cores. */
export const CPU_COUNT_PHYSICAL_KEY = 'cpu.count.physical';
/** Number of processor packages (sockets). */
export const CPU_COUNT_PACKAGE_KEY = 'cpu.count.package';

// --- sources --------------------------------------------------------------

/** The machine-wide CPU source. */
export const CPU_SYSTEM_SOURCE: SourceId = 'cpu:system';

/** Prefix of a logical processor's source instance: `cpu:logical-0`. */
const CPU_LOGICAL_PREFIX = 'cpu:logical-';

/**
 * Builds the canonical source of one logical processor, e.g. `cpu:logical-7`.
 *
 * Mirrors `metrics::wellknown::cpu::topology::LogicalId::source_id`.
 */
export function cpuLogicalSourceId(ordinal: number): SourceId {
  return `${CPU_LOGICAL_PREFIX}${ordinal}`;
}

/**
 * Recovers a logical processor's ordinal from its source identifier.
 *
 * Returns `null` for any other source, including `cpu:system`. Parsing is
 * strict — `cpu:logical-01` and `cpu:logical-1x` are rejected rather than
 * coerced — so a malformed identifier can never be silently folded onto a
 * real processor's row.
 */
export function cpuLogicalOrdinal(sourceId: SourceId): number | null {
  if (!sourceId.startsWith(CPU_LOGICAL_PREFIX)) return null;

  const suffix = sourceId.slice(CPU_LOGICAL_PREFIX.length);
  if (!/^(0|[1-9][0-9]*)$/.test(suffix)) return null;

  const ordinal = Number(suffix);
  return Number.isSafeInteger(ordinal) ? ordinal : null;
}

/** Aggregate CPU utilisation, 0–100 percent. */
export const CPU_USAGE_TOTAL: MetricRef = {
  key: CPU_USAGE_TOTAL_KEY,
  sourceId: CPU_SYSTEM_SOURCE,
};

/** Number of logical processors this machine has. */
export const CPU_COUNT_LOGICAL: MetricRef = {
  key: CPU_COUNT_LOGICAL_KEY,
  sourceId: CPU_SYSTEM_SOURCE,
};

/** Number of physical cores this machine has. */
export const CPU_COUNT_PHYSICAL: MetricRef = {
  key: CPU_COUNT_PHYSICAL_KEY,
  sourceId: CPU_SYSTEM_SOURCE,
};

/** Number of processor packages this machine has. */
export const CPU_COUNT_PACKAGE: MetricRef = {
  key: CPU_COUNT_PACKAGE_KEY,
  sourceId: CPU_SYSTEM_SOURCE,
};

/** The machine-wide CPU metrics the CPU details card always requests. */
export const CPU_TOPOLOGY_METRICS: readonly MetricRef[] = [
  CPU_COUNT_PHYSICAL,
  CPU_COUNT_LOGICAL,
  CPU_COUNT_PACKAGE,
] as const;

/** Installed physical memory, in bytes. */
export const MEMORY_TOTAL: MetricRef = {
  key: 'memory.total',
  sourceId: 'memory:system',
};

/** Physical memory in use (`total - available`), in bytes. */
export const MEMORY_USED: MetricRef = {
  key: 'memory.used',
  sourceId: 'memory:system',
};

/** Physical memory obtainable without swapping, in bytes. */
export const MEMORY_AVAILABLE: MetricRef = {
  key: 'memory.available',
  sourceId: 'memory:system',
};

/** Share of physical memory in use, 0–100 percent. */
export const MEMORY_USAGE_PERCENT: MetricRef = {
  key: 'memory.usage.percent',
  sourceId: 'memory:system',
};

/** Everything the "Live system sample" card requests, in display order. */
export const LIVE_SAMPLE_METRICS: readonly MetricRef[] = [
  CPU_USAGE_TOTAL,
  MEMORY_TOTAL,
  MEMORY_USED,
  MEMORY_AVAILABLE,
  MEMORY_USAGE_PERCENT,
] as const;

/** Builds the `key@sourceId` string used to index a sample response. */
export function metricRefId(metric: MetricRef): string {
  return `${metric.key}@${metric.sourceId}`;
}
