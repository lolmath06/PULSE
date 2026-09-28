import type { MetricDefinition } from '@/types/metrics';
import { describeAvailability } from '@/utils/metrics';
import type { DeepPartial, VisualizationConfig } from '@/visualization/config';
import type { WidgetBinding, WidgetInstance, WidgetKind } from '@/dashboard/model';
import { DEFAULT_FRAME, DEFAULT_GROUP, KIND_LIMITS, defaultPixelSize } from '@/dashboard/model';
import { configFor } from '@/dashboard/metricInfo';
import { newId } from '@/dashboard/ids';

/**
 * The widget library: what *Add widget* offers.
 *
 * A short list of useful starting points per category — not forty
 * specialised types. Every entry is built from the four generic kinds and the
 * Phase 10 renderers, and is offered only as far as the live catalog allows:
 * an entry whose metrics do not exist or cannot be read on this machine is
 * shown disabled, with the backend's own reason.
 */

export const CATEGORIES = [
  'CPU',
  'Memory',
  'GPU',
  'Thermals',
  'Storage',
  'Network',
  'Processes',
  'Custom',
] as const;
export type Category = (typeof CATEGORIES)[number];

export interface BlueprintBinding {
  readonly key: string;
  readonly label?: string;
}

export interface WidgetBlueprint {
  readonly id: string;
  readonly category: Category;
  readonly label: string;
  readonly description: string;
  readonly kind: WidgetKind;
  readonly bindings: readonly BlueprintBinding[];
  readonly style?: DeepPartial<VisualizationConfig>;
  readonly group?: { orientation: 'rows' | 'inline'; sparklines: boolean };
  readonly size?: { w: number; h: number };
  readonly pixelSize?: { width: number; height: number };
  readonly title?: string;
}

const VALUE_STYLE: DeepPartial<VisualizationConfig> = {
  renderer: 'value',
  display: { compact: true },
  background: { mode: 'none', opacity: 0 },
  frame: { border: 'none', radius: 0 },
};

const SPARK_STYLE: DeepPartial<VisualizationConfig> = {
  renderer: 'sparkline',
  size: { preset: 'small', width: null, height: null },
  display: { compact: true },
  background: { mode: 'none', opacity: 0 },
  frame: { border: 'none', radius: 0 },
};

export const BLUEPRINTS: readonly WidgetBlueprint[] = [
  // CPU — CPU Total is `cpu.usage.total`, the backend's own aggregate.
  {
    id: 'cpu-total',
    category: 'CPU',
    label: 'CPU Total',
    description: 'Machine-wide CPU usage over time.',
    kind: 'visualization',
    bindings: [{ key: 'cpu.usage.total', label: 'CPU' }],
    size: { w: 6, h: 4 },
  },
  {
    id: 'cpu-value',
    category: 'CPU',
    label: 'CPU Total — value',
    description: 'Just the number: CPU 23 %.',
    kind: 'value',
    bindings: [{ key: 'cpu.usage.total', label: 'CPU' }],
    style: VALUE_STYLE,
  },
  {
    id: 'cpu-sparkline',
    category: 'CPU',
    label: 'CPU Total — sparkline',
    description: 'CPU 23 % with a tiny trend.',
    kind: 'visualization',
    bindings: [{ key: 'cpu.usage.total', label: 'CPU' }],
    style: SPARK_STYLE,
    size: { w: 3, h: 2 },
    pixelSize: { width: 160, height: 40 },
  },
  {
    id: 'cpu-group',
    category: 'CPU',
    label: 'CPU group',
    description: 'Usage and package temperature together.',
    kind: 'group',
    title: 'CPU',
    bindings: [
      { key: 'cpu.usage.total', label: 'Usage' },
      { key: 'cpu.temperature.package', label: 'Temp' },
    ],
  },
  {
    id: 'cpu-logical',
    category: 'CPU',
    label: 'One logical processor',
    description: 'A single logical processor; pick which one in Customize.',
    kind: 'visualization',
    bindings: [{ key: 'cpu.usage.logical' }],
    style: SPARK_STYLE,
    size: { w: 3, h: 2 },
  },
  // Memory
  {
    id: 'memory',
    category: 'Memory',
    label: 'Memory usage',
    description: 'Physical memory in use, as a percentage.',
    kind: 'visualization',
    bindings: [{ key: 'memory.usage.percent', label: 'RAM' }],
    size: { w: 6, h: 4 },
  },
  {
    id: 'memory-value',
    category: 'Memory',
    label: 'Memory — value',
    description: 'RAM 41 %.',
    kind: 'value',
    bindings: [{ key: 'memory.usage.percent', label: 'RAM' }],
    style: VALUE_STYLE,
  },
  {
    id: 'memory-used',
    category: 'Memory',
    label: 'Memory used — value',
    description: 'Bytes in use, e.g. 12.0 GiB.',
    kind: 'value',
    bindings: [{ key: 'memory.used', label: 'RAM' }],
    style: VALUE_STYLE,
  },
  // GPU
  {
    id: 'gpu',
    category: 'GPU',
    label: 'GPU usage',
    description: 'GPU core load over time.',
    kind: 'visualization',
    bindings: [{ key: 'gpu.usage.core', label: 'GPU' }],
  },
  {
    id: 'gpu-value',
    category: 'GPU',
    label: 'GPU — value',
    description: 'GPU 97 %.',
    kind: 'value',
    bindings: [{ key: 'gpu.usage.core', label: 'GPU' }],
    style: VALUE_STYLE,
  },
  {
    id: 'gpu-group',
    category: 'GPU',
    label: 'GPU group',
    description: 'Usage, VRAM and temperature.',
    kind: 'group',
    title: 'GPU',
    bindings: [
      { key: 'gpu.usage.core', label: 'Usage' },
      { key: 'gpu.memory.used', label: 'VRAM' },
      { key: 'gpu.temperature.core', label: 'Temp' },
    ],
  },
  // Thermals
  {
    id: 'thermal',
    category: 'Thermals',
    label: 'CPU & GPU temperature',
    description: 'Both sensors on one chart; an unreadable one is simply absent.',
    kind: 'visualization',
    bindings: [
      { key: 'cpu.temperature.package', label: 'CPU' },
      { key: 'gpu.temperature.core', label: 'GPU' },
    ],
  },
  {
    id: 'cpu-temp-value',
    category: 'Thermals',
    label: 'CPU temperature — value',
    description: '71 °C.',
    kind: 'value',
    bindings: [{ key: 'cpu.temperature.package', label: 'CPU' }],
    style: VALUE_STYLE,
  },
  {
    id: 'gpu-temp-value',
    category: 'Thermals',
    label: 'GPU temperature — value',
    description: '72 °C.',
    kind: 'value',
    bindings: [{ key: 'gpu.temperature.core', label: 'GPU' }],
    style: VALUE_STYLE,
  },
  {
    id: 'ssd-temp',
    category: 'Thermals',
    label: 'Drive temperature — value',
    description: 'Read from the drive on demand, not every second.',
    kind: 'value',
    bindings: [{ key: 'storage.health.temperature', label: 'SSD' }],
    style: VALUE_STYLE,
  },
  // Storage
  {
    id: 'storage',
    category: 'Storage',
    label: 'Disk read / write',
    description: 'Throughput of one drive (automatic or chosen).',
    kind: 'visualization',
    bindings: [
      { key: 'storage.io.read.bytes_per_second', label: 'Read' },
      { key: 'storage.io.write.bytes_per_second', label: 'Write' },
    ],
    size: { w: 6, h: 4 },
  },
  {
    id: 'volume',
    category: 'Storage',
    label: 'Filesystem usage',
    description: 'How full one filesystem is.',
    kind: 'visualization',
    bindings: [{ key: 'storage.volume.usage.percent', label: 'Disk' }],
    style: { renderer: 'bar' },
    size: { w: 4, h: 2 },
  },
  // Network
  {
    id: 'network',
    category: 'Network',
    label: 'Download / upload',
    description: 'Traffic of one interface (automatic or chosen).',
    kind: 'visualization',
    bindings: [
      { key: 'network.receive.bytes_per_second', label: 'Download' },
      { key: 'network.transmit.bytes_per_second', label: 'Upload' },
    ],
  },
  {
    id: 'network-down-value',
    category: 'Network',
    label: 'Download — value',
    description: 'NET ↓ 12 MiB/s.',
    kind: 'value',
    bindings: [{ key: 'network.receive.bytes_per_second', label: 'NET ↓' }],
    style: VALUE_STYLE,
  },
  {
    id: 'wifi',
    category: 'Network',
    label: 'Wi-Fi signal',
    description: 'Signal strength of a wireless interface.',
    kind: 'value',
    bindings: [{ key: 'network.wifi.signal.rssi', label: 'Wi-Fi' }],
    style: VALUE_STYLE,
  },
  // Processes
  {
    id: 'processes',
    category: 'Processes',
    label: 'Process & thread counts',
    description: 'Machine-wide counts; never an individual process.',
    kind: 'group',
    title: 'Processes',
    bindings: [
      { key: 'process.count.total', label: 'Processes' },
      { key: 'process.count.running', label: 'Running' },
      { key: 'process.thread.count.total', label: 'Threads' },
    ],
  },
  // Custom
  {
    id: 'summary',
    category: 'Custom',
    label: 'System summary',
    description: 'CPU / GPU / RAM / NET in one strip, with tiny trends.',
    kind: 'summary',
    title: 'System',
    bindings: [
      { key: 'cpu.usage.total', label: 'CPU' },
      { key: 'gpu.usage.core', label: 'GPU' },
      { key: 'memory.usage.percent', label: 'RAM' },
      { key: 'network.receive.bytes_per_second', label: 'NET ↓' },
    ],
    group: { orientation: 'inline', sparklines: true },
  },
];

export type Availability = { readonly ok: true } | { readonly ok: false; readonly reason: string };

/**
 * Whether an entry can show anything on this machine.
 *
 * Usable when at least one of its metrics exists and at least one source of
 * it is available. Otherwise disabled, with a reason: absent from the catalog,
 * or the backend's own explanation (no NVML, no sensor, …).
 */
export function blueprintAvailability(
  blueprint: Pick<WidgetBlueprint, 'bindings'>,
  catalog: readonly MetricDefinition[],
): Availability {
  let firstReason: string | null = null;
  for (const binding of blueprint.bindings) {
    const definitions = catalog.filter((definition) => definition.metric.key === binding.key);
    if (definitions.some((definition) => definition.availability.status === 'available')) {
      return { ok: true };
    }
    if (!firstReason) {
      firstReason =
        definitions.length === 0
          ? 'This machine does not report this metric.'
          : describeAvailability(definitions[0]!.availability);
    }
  }
  return { ok: false, reason: firstReason ?? 'No metric selected.' };
}

/** A new widget from a blueprint, with fresh ids. Placement is the caller's. */
export function createWidget(blueprint: WidgetBlueprint): WidgetInstance {
  const bindings: WidgetBinding[] = blueprint.bindings.map((binding) => ({
    key: binding.key,
    source: { mode: 'auto' },
    label: binding.label ?? null,
  }));
  const config = configFor(blueprint.bindings[0]!.key, blueprint.style);
  const limits = KIND_LIMITS[blueprint.kind];
  return {
    id: newId('w'),
    kind: blueprint.kind,
    title: blueprint.title ?? null,
    bindings,
    visual: { presetId: 'clean', modified: false, config, range: '15m' },
    dataMode: 'auto',
    group: blueprint.group ?? DEFAULT_GROUP,
    frame: {
      ...DEFAULT_FRAME,
      showTitle: blueprint.kind === 'visualization' || blueprint.kind === 'group',
    },
    layout: { x: 0, y: 0, w: blueprint.size?.w ?? limits.w, h: blueprint.size?.h ?? limits.h },
    size: blueprint.pixelSize ?? defaultPixelSize(blueprint.kind, config.renderer),
  };
}

/** A widget for any single catalog metric — the *Custom* entry. */
export function createCustomWidget(definition: MetricDefinition): WidgetInstance {
  return createWidget({
    id: 'custom',
    category: 'Custom',
    label: definition.displayName,
    description: '',
    kind: 'visualization',
    bindings: [{ key: definition.metric.key, label: definition.displayName.slice(0, 24) }],
  });
}

export function findBlueprint(id: string): WidgetBlueprint | undefined {
  return BLUEPRINTS.find((blueprint) => blueprint.id === id);
}
