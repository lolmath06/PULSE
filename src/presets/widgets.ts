import type { DeepPartial, ThresholdBand, VisualizationConfig } from '@/visualization/config';
import type { GridRect, WidgetFrame, WidgetInstance, WidgetKind } from '@/dashboard/model';
import { createWidget, tx } from '@/dashboard/library';
import type { Text } from '@/i18n/text';

/**
 * Building blocks for PULSE's curated overlay packs and dashboard templates.
 *
 * Every widget binds real metric keys from the catalog; a metric this machine
 * cannot read shows "—" with the backend's reason, never an invented number.
 * There is no FPS anywhere: PULSE has no real FPS source.
 */

/** Visual temperature bands for packs: a look, not a verdict about the hardware. */
export const TEMP_BANDS: readonly ThresholdBand[] = [
  { upTo: 70, color: '#4ade80' },
  { upTo: 85, color: '#fbbf24' },
  { upTo: null, color: '#f87171' },
];

const NO_FRAME: Partial<WidgetFrame> = {
  showTitle: false,
  padding: 2,
  border: 'none',
  radius: 0,
  background: null,
};

/** A micro widget's chrome-free look: the card or the overlay is the surface. */
const MICRO: DeepPartial<VisualizationConfig> = {
  display: { compact: true },
  background: { mode: 'none', opacity: 0 },
  frame: { border: 'none', radius: 0, shadow: 'none' },
};

export interface MetricSpec {
  readonly key: string;
  /** Language-neutral (`CPU`, `↓`) or a built-in word (`tx('download')`). */
  readonly label?: Text;
}

export interface WidgetSpec {
  readonly metrics: readonly MetricSpec[];
  readonly kind?: WidgetKind;
  readonly title?: Text;
  /** Pixel size in an overlay or Mini. */
  readonly size?: readonly [number, number];
  /** Grid rectangle on a dashboard: x, y, w, h. */
  readonly rect?: readonly [number, number, number, number];
  readonly style?: DeepPartial<VisualizationConfig>;
  readonly group?: { orientation: 'rows' | 'inline'; sparklines: boolean };
  /** Show the frame title (dashboards); overlays default to none. */
  readonly titled?: boolean;
  /** Temperature bands on the value. */
  readonly thermal?: boolean;
}

export function widget(spec: WidgetSpec): WidgetInstance {
  const kind = spec.kind ?? 'visualization';
  const thermal: DeepPartial<VisualizationConfig> = spec.thermal
    ? { colors: { mode: 'threshold', thresholds: TEMP_BANDS } }
    : {};
  const style = mergeStyles(
    kind === 'value' ? { renderer: 'value', ...MICRO } : {},
    spec.style,
    thermal,
  );
  const created = createWidget({
    id: 'preset',
    category: 'Custom',
    kind,
    title: spec.title,
    bindings: spec.metrics.map((metric) => ({ key: metric.key, label: metric.label })),
    style,
    group: spec.group,
    pixelSize: spec.size ? { width: spec.size[0], height: spec.size[1] } : undefined,
  });
  const titled = spec.titled ?? (spec.rect !== undefined && kind !== 'value');
  const rect: GridRect | undefined = spec.rect
    ? { x: spec.rect[0], y: spec.rect[1], w: spec.rect[2], h: spec.rect[3] }
    : undefined;
  return {
    ...created,
    // Templates keep the "clean" preset unmodified, so the style dresses it.
    frame: titled ? { ...created.frame, showTitle: true } : { ...created.frame, ...NO_FRAME },
    layout: rect ?? created.layout,
  };
}

function mergeStyles(
  ...parts: (DeepPartial<VisualizationConfig> | undefined)[]
): DeepPartial<VisualizationConfig> {
  const out: Record<string, unknown> = {};
  for (const part of parts) {
    if (!part) continue;
    for (const [key, value] of Object.entries(part)) {
      const current = out[key];
      out[key] =
        typeof current === 'object' &&
        current &&
        typeof value === 'object' &&
        value &&
        !Array.isArray(value)
          ? { ...(current as object), ...(value as object) }
          : value;
    }
  }
  return out as DeepPartial<VisualizationConfig>;
}

// --- shorthands ---------------------------------------------------------------

export { tx };

export const K = {
  cpu: { key: 'cpu.usage.total', label: 'CPU' },
  cpuTemp: { key: 'cpu.temperature.package', label: 'CPU' },
  gpu: { key: 'gpu.usage.core', label: 'GPU' },
  gpuTemp: { key: 'gpu.temperature.core', label: 'GPU' },
  hotspot: { key: 'gpu.temperature.hotspot', label: tx('hotspot') },
  vram: { key: 'gpu.memory.used', label: 'VRAM' },
  ram: { key: 'memory.usage.percent', label: 'RAM' },
  ramUsed: { key: 'memory.used', label: 'RAM' },
  down: { key: 'network.receive.bytes_per_second', label: '↓' },
  up: { key: 'network.transmit.bytes_per_second', label: '↑' },
  read: { key: 'storage.io.read.bytes_per_second', label: tx('read') },
  write: { key: 'storage.io.write.bytes_per_second', label: tx('write') },
  disk: { key: 'storage.volume.usage.percent', label: tx('disk') },
  ssdTemp: { key: 'storage.health.temperature', label: 'SSD' },
  wifi: { key: 'network.wifi.signal.rssi', label: 'Wi-Fi' },
  procs: { key: 'process.count.total', label: tx('processes') },
  running: { key: 'process.count.running', label: tx('running') },
  threads: { key: 'process.thread.count.total', label: tx('threads') },
  freq: { key: 'cpu.frequency.current', label: tx('clock') },
} as const satisfies Record<string, MetricSpec>;

/** `CPU 23 %` — a number, as large as its box allows. */
export const value = (metric: MetricSpec, size: readonly [number, number], thermal = false) =>
  widget({ metrics: [metric], kind: 'value', size, thermal });

/** `CPU 23 % ∿∿∿` — the number with its live trend beside it. */
export const spark = (
  metrics: readonly MetricSpec[],
  size: readonly [number, number],
  thermal = false,
) =>
  widget({
    metrics,
    size,
    thermal,
    style: {
      renderer: 'sparkline',
      size: { preset: 'small', width: null, height: null },
      ...MICRO,
    },
  });

/** `CPU ███████░░░ 72 %`. */
export const meter = (metric: MetricSpec, size: readonly [number, number]) =>
  widget({ metrics: [metric], size, style: { renderer: 'bar', ...MICRO } });

/** A ring. */
export const ring = (metric: MetricSpec, size: readonly [number, number], thermal = false) =>
  widget({
    metrics: [metric],
    size,
    thermal,
    style: {
      renderer: 'gauge',
      gauge: { thickness: 0.13, arc: 'three-quarter' },
      ...(thermal ? { scale: { mode: 'fixed' as const, min: 20, max: 100 } } : {}),
      ...MICRO,
    },
  });

/** Several values in rows (`Usage 37 %` / `Temp 68 °C`). */
export const rows = (
  title: Text,
  metrics: readonly MetricSpec[],
  size: readonly [number, number],
) =>
  widget({
    metrics,
    kind: 'group',
    title,
    size,
    group: { orientation: 'rows', sparklines: false },
  });

/** `CPU 24 % | GPU 84 % | RAM 41 %`, optionally with trends. */
export const strip = (
  metrics: readonly MetricSpec[],
  size: readonly [number, number],
  sparklines = false,
) =>
  widget({
    metrics,
    kind: 'summary',
    title: tx('system'),
    size,
    group: { orientation: 'inline', sparklines },
  });

/** A chart for an overlay or Mini, without axes clutter. */
export const chart = (
  metrics: readonly MetricSpec[],
  size: readonly [number, number],
  renderer: 'area' | 'line' = 'area',
  thermal = false,
) =>
  widget({
    metrics,
    size,
    thermal,
    style: {
      renderer,
      axes: { x: false, y: false, grid: false },
      display: { legend: false, min: false, max: false, average: false },
      background: { mode: 'none', opacity: 0 },
      frame: { border: 'none', radius: 0, shadow: 'none' },
    },
  });
