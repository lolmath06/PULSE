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

// --- GPU ------------------------------------------------------------------

/** Number of hardware graphics adapters inventoried. */
export const GPU_COUNT_KEY = 'gpu.count';
/** Share of a GPU's graphics engine in use, 0–100 percent. */
export const GPU_USAGE_CORE_KEY = 'gpu.usage.core';
/** Dedicated video memory installed on an adapter, in bytes. */
export const GPU_MEMORY_TOTAL_KEY = 'gpu.memory.total';
/** Dedicated video memory in use, in bytes. */
export const GPU_MEMORY_USED_KEY = 'gpu.memory.used';
/** Dedicated video memory still allocatable, in bytes. */
export const GPU_MEMORY_FREE_KEY = 'gpu.memory.free';
/** Share of dedicated video memory in use, 0–100 percent. */
export const GPU_MEMORY_USAGE_PERCENT_KEY = 'gpu.memory.usage.percent';
/** Current graphics clock, in hertz. */
export const GPU_FREQUENCY_CORE_KEY = 'gpu.frequency.core';
/** Current video memory clock, in hertz. */
export const GPU_FREQUENCY_MEMORY_KEY = 'gpu.frequency.memory';
/** GPU die temperature, in degrees Celsius. */
export const GPU_TEMPERATURE_CORE_KEY = 'gpu.temperature.core';
/** Hottest point on the GPU package, in degrees Celsius. */
export const GPU_TEMPERATURE_HOTSPOT_KEY = 'gpu.temperature.hotspot';
/** Video memory temperature, in degrees Celsius. */
export const GPU_TEMPERATURE_MEMORY_KEY = 'gpu.temperature.memory';
/** Fan speed, in revolutions per minute. */
export const GPU_FAN_SPEED_KEY = 'gpu.fan.speed';

/** The machine-wide GPU source. */
export const GPU_SYSTEM_SOURCE: SourceId = 'gpu:system';

/**
 * The GPU metrics that describe **performance**: what the engine is doing and
 * how fast, as opposed to how hot it is.
 *
 * Kept apart from the thermal set because the two fail independently. An
 * open-source driver commonly exposes a temperature and no utilisation
 * counter, and telling such a user "GPU telemetry unavailable" would be wrong
 * on both counts — the GPU is detected, and one half of its telemetry works.
 */
export const GPU_PERFORMANCE_KEYS: readonly string[] = [
  GPU_USAGE_CORE_KEY,
  GPU_MEMORY_TOTAL_KEY,
  GPU_MEMORY_USED_KEY,
  GPU_MEMORY_FREE_KEY,
  GPU_MEMORY_USAGE_PERCENT_KEY,
  GPU_FREQUENCY_CORE_KEY,
  GPU_FREQUENCY_MEMORY_KEY,
] as const;

/** The GPU metrics that describe **temperature and cooling**. */
export const GPU_THERMAL_KEYS: readonly string[] = [
  GPU_TEMPERATURE_CORE_KEY,
  GPU_TEMPERATURE_HOTSPOT_KEY,
  GPU_TEMPERATURE_MEMORY_KEY,
  GPU_FAN_SPEED_KEY,
] as const;

/**
 * Every metric PULSE publishes per GPU.
 *
 * Mirrors `metrics::wellknown::gpu::PER_GPU_KEYS`. The frontend never assumes
 * how many GPUs exist — it discovers their sources from the catalog.
 */
export const GPU_PER_DEVICE_KEYS: readonly string[] = [...GPU_PERFORMANCE_KEYS] as const;

/** How many hardware GPUs this machine has. */
export const GPU_COUNT: MetricRef = {
  key: GPU_COUNT_KEY,
  sourceId: GPU_SYSTEM_SOURCE,
};

/**
 * Whether a source identifies one physical GPU.
 *
 * Every GPU source is `gpu:<instance>`; `gpu:system` is the machine-wide
 * aggregate and is deliberately excluded. Nothing here parses the instance —
 * it is a stable identifier whose internal shape (an NVML UUID, a PCI address,
 * a device-model tuple) is the backend's business, not the interface's.
 */
export function isGpuDeviceSource(sourceId: SourceId): boolean {
  return sourceId.startsWith('gpu:') && sourceId !== GPU_SYSTEM_SOURCE;
}

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
