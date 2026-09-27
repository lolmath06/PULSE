import type { DeepPartial, VisualizationConfig } from '@/visualization/config';
import { PERCENT_THRESHOLDS } from '@/visualization/color';
import type { VisualizationMeta } from '@/visualization/types';

/**
 * Per-metric starting points. The user can change every one of them; these
 * only decide what a chart looks like the first time.
 *
 * Module constants on purpose: the visualization store keys its memoisation
 * on their identity.
 */

const PERCENT_SCALE = { mode: 'fixed', min: 0, max: 100 } as const;

/** CPU: a percentage over time, filled. */
export const CPU_DEFAULTS: DeepPartial<VisualizationConfig> = {
  renderer: 'area',
  scale: PERCENT_SCALE,
  colors: { thresholds: PERCENT_THRESHOLDS },
};

/** Memory: a percentage, filled. */
export const MEMORY_DEFAULTS: DeepPartial<VisualizationConfig> = {
  renderer: 'area',
  scale: PERCENT_SCALE,
  colors: { thresholds: PERCENT_THRESHOLDS },
};

/** GPU usage: a percentage, filled. */
export const GPU_DEFAULTS: DeepPartial<VisualizationConfig> = {
  renderer: 'area',
  scale: PERCENT_SCALE,
  colors: { thresholds: PERCENT_THRESHOLDS },
};

/** Temperatures: a line on an auto scale. No invented maximum. */
export const THERMAL_DEFAULTS: DeepPartial<VisualizationConfig> = {
  renderer: 'line',
  scale: { mode: 'auto', min: null, max: null },
};

/** Throughput: two series, lightly filled, auto scale from zero. */
export const THROUGHPUT_DEFAULTS: DeepPartial<VisualizationConfig> = {
  renderer: 'area',
  fill: { mode: 'gradient', opacity: 0.22 },
  scale: { mode: 'auto', min: null, max: null },
};

/** Process and thread counts: a line. */
export const COUNT_DEFAULTS: DeepPartial<VisualizationConfig> = {
  renderer: 'line',
  scale: { mode: 'auto', min: null, max: null },
};

/** The sparkline cells of the per-processor view. */
export const LOGICAL_CELL_STYLE: DeepPartial<VisualizationConfig> = {
  renderer: 'sparkline',
  display: { compact: true },
};

export const CPU_META: VisualizationMeta = {
  label: 'CPU',
  unit: 'percent',
  bounds: { min: 0, max: 100 },
  decimals: 1,
};

export const MEMORY_META: VisualizationMeta = {
  label: 'Memory',
  unit: 'percent',
  bounds: { min: 0, max: 100 },
  decimals: 1,
};

export const GPU_META: VisualizationMeta = {
  label: 'GPU',
  unit: 'percent',
  bounds: { min: 0, max: 100 },
  decimals: 0,
};

export const THERMAL_META: VisualizationMeta = {
  label: 'Temperature',
  unit: 'celsius',
  bounds: null,
  decimals: 0,
};

export const STORAGE_META: VisualizationMeta = {
  label: 'Disk',
  unit: 'bytesPerSecond',
  bounds: null,
  decimals: 1,
};

export const NETWORK_META: VisualizationMeta = {
  label: 'Network',
  unit: 'bytesPerSecond',
  bounds: null,
  decimals: 1,
};
