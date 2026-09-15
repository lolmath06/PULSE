import type { MetricRef } from '@/types/metrics';

/**
 * The metric references PULSE currently ships.
 *
 * Mirrors `src-tauri/src/metrics/wellknown/`. These strings are the stable
 * contract a saved dashboard will store, and they are identical on Fedora and
 * Windows — only the backing provider differs, which the UI never sees.
 *
 * A Rust contract test asserts both platforms declare exactly these.
 */

/** Aggregate CPU utilisation, 0–100 percent. */
export const CPU_USAGE_TOTAL: MetricRef = {
  key: 'cpu.usage.total',
  sourceId: 'cpu:system',
};

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
