import type { MetricDefinition, MetricUnit } from '@/types/metrics';
import type { DeepPartial, VisualizationConfig } from '@/visualization/config';
import { BASE_CONFIG, mergeConfig } from '@/visualization/config';
import { PERCENT_THRESHOLDS } from '@/visualization/color';
import type { VisualizationMeta } from '@/visualization/types';

/**
 * What a widget needs to know about a metric key to present it well: a short
 * label, its unit when the catalog is not loaded yet, and a sensible first
 * look. Everything here is a starting point the user can change.
 */

interface KeyInfo {
  readonly label: string;
  readonly unit: MetricUnit;
}

const KEYS: Readonly<Record<string, KeyInfo>> = {
  'cpu.usage.total': { label: 'CPU', unit: 'percent' },
  'cpu.usage.logical': { label: 'Core', unit: 'percent' },
  'cpu.frequency.current': { label: 'Freq', unit: 'hertz' },
  'cpu.temperature.package': { label: 'CPU temp', unit: 'celsius' },
  'memory.usage.percent': { label: 'RAM', unit: 'percent' },
  'memory.used': { label: 'RAM used', unit: 'bytes' },
  'memory.available': { label: 'RAM free', unit: 'bytes' },
  'gpu.usage.core': { label: 'GPU', unit: 'percent' },
  'gpu.memory.used': { label: 'VRAM', unit: 'bytes' },
  'gpu.memory.usage.percent': { label: 'VRAM', unit: 'percent' },
  'gpu.temperature.core': { label: 'GPU temp', unit: 'celsius' },
  'gpu.temperature.hotspot': { label: 'Hotspot', unit: 'celsius' },
  'gpu.fan.speed': { label: 'GPU fan', unit: 'rpm' },
  'storage.io.read.bytes_per_second': { label: 'Read', unit: 'bytesPerSecond' },
  'storage.io.write.bytes_per_second': { label: 'Write', unit: 'bytesPerSecond' },
  'storage.volume.usage.percent': { label: 'Disk', unit: 'percent' },
  'storage.health.temperature': { label: 'SSD temp', unit: 'celsius' },
  'network.receive.bytes_per_second': { label: '↓', unit: 'bytesPerSecond' },
  'network.transmit.bytes_per_second': { label: '↑', unit: 'bytesPerSecond' },
  'network.wifi.signal.rssi': { label: 'Wi-Fi', unit: 'decibelMilliwatts' },
  'network.wifi.signal.quality': { label: 'Wi-Fi', unit: 'percent' },
  'process.count.total': { label: 'Processes', unit: 'count' },
  'process.count.running': { label: 'Running', unit: 'count' },
  'process.thread.count.total': { label: 'Threads', unit: 'count' },
};

export function keyLabel(key: string, definition?: MetricDefinition): string {
  return KEYS[key]?.label ?? definition?.displayName ?? key;
}

export function keyUnit(key: string, definition?: MetricDefinition): MetricUnit {
  return definition?.unit ?? KEYS[key]?.unit ?? 'none';
}

export function metaFor(
  key: string,
  label: string,
  definition?: MetricDefinition,
): VisualizationMeta {
  const unit = keyUnit(key, definition);
  return {
    label,
    unit,
    // Only a percentage has natural bounds. A temperature never gets an
    // invented 0–100.
    bounds: unit === 'percent' ? { min: 0, max: 100 } : null,
    decimals: unit === 'percent' ? (key === 'cpu.usage.total' ? 0 : 0) : undefined,
  };
}

/** The first look of a chart of `key`, by unit. */
export function chartDefaultsFor(
  key: string,
  definition?: MetricDefinition,
): DeepPartial<VisualizationConfig> {
  const unit = keyUnit(key, definition);
  if (unit === 'percent') {
    return {
      renderer: 'area',
      scale: { mode: 'fixed', min: 0, max: 100 },
      colors: { thresholds: PERCENT_THRESHOLDS },
    };
  }
  if (unit === 'celsius')
    return { renderer: 'line', scale: { mode: 'auto', min: null, max: null } };
  if (unit === 'bytesPerSecond') {
    return { renderer: 'area', fill: { mode: 'gradient', opacity: 0.22 } };
  }
  return { renderer: 'line' };
}

export function configFor(
  key: string,
  overrides: DeepPartial<VisualizationConfig> = {},
  definition?: MetricDefinition,
): VisualizationConfig {
  return mergeConfig(mergeConfig(BASE_CONFIG, chartDefaultsFor(key, definition)), overrides);
}
