import type { MetricDefinition } from '@/types/metrics';
import { describeAvailability } from '@/utils/metrics';
import type { DeepPartial, VisualizationConfig } from '@/visualization/config';
import type { WidgetBinding, WidgetInstance, WidgetKind } from '@/dashboard/model';
import { DEFAULT_FRAME, DEFAULT_GROUP, KIND_LIMITS, defaultPixelSize } from '@/dashboard/model';
import { configFor } from '@/dashboard/metricInfo';
import { newId } from '@/dashboard/ids';
import { hasKey, t } from '@/i18n/i18n';
import { metricNameKey } from '@/i18n/metrics';
import type { Text } from '@/i18n/text';
import { englishOf, keyOf } from '@/i18n/text';

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
  readonly label?: Text;
}

/**
 * A library entry. Its name and description are translations keyed by `id`
 * (`library.blueprints.<id>.label` / `.description`).
 */
export interface WidgetBlueprint {
  readonly id: string;
  readonly category: Category;
  readonly kind: WidgetKind;
  readonly bindings: readonly BlueprintBinding[];
  readonly style?: DeepPartial<VisualizationConfig>;
  readonly group?: { orientation: 'rows' | 'inline'; sparklines: boolean };
  readonly size?: { w: number; h: number };
  readonly pixelSize?: { width: number; height: number };
  readonly title?: Text;
}

/** Built-in widget words, by id: `presets.text.<id>`. */
export const tx = (id: string): Text => ({ key: `presets.text.${id}` });

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
    kind: 'visualization',
    bindings: [{ key: 'cpu.usage.total', label: 'CPU' }],
    size: { w: 6, h: 4 },
  },
  {
    id: 'cpu-value',
    category: 'CPU',
    kind: 'value',
    bindings: [{ key: 'cpu.usage.total', label: 'CPU' }],
    style: VALUE_STYLE,
  },
  {
    id: 'cpu-sparkline',
    category: 'CPU',
    kind: 'visualization',
    bindings: [{ key: 'cpu.usage.total', label: 'CPU' }],
    style: SPARK_STYLE,
    size: { w: 3, h: 2 },
    pixelSize: { width: 160, height: 40 },
  },
  {
    id: 'cpu-group',
    category: 'CPU',
    kind: 'group',
    title: 'CPU',
    bindings: [
      { key: 'cpu.usage.total', label: tx('usage') },
      { key: 'cpu.temperature.package', label: tx('temp') },
    ],
  },
  {
    id: 'cpu-logical',
    category: 'CPU',
    kind: 'visualization',
    bindings: [{ key: 'cpu.usage.logical' }],
    style: SPARK_STYLE,
    size: { w: 3, h: 2 },
  },
  // Memory
  {
    id: 'memory',
    category: 'Memory',
    kind: 'visualization',
    bindings: [{ key: 'memory.usage.percent', label: 'RAM' }],
    size: { w: 6, h: 4 },
  },
  {
    id: 'memory-value',
    category: 'Memory',
    kind: 'value',
    bindings: [{ key: 'memory.usage.percent', label: 'RAM' }],
    style: VALUE_STYLE,
  },
  {
    id: 'memory-used',
    category: 'Memory',
    kind: 'value',
    bindings: [{ key: 'memory.used', label: 'RAM' }],
    style: VALUE_STYLE,
  },
  // GPU
  {
    id: 'gpu',
    category: 'GPU',
    kind: 'visualization',
    bindings: [{ key: 'gpu.usage.core', label: 'GPU' }],
  },
  {
    id: 'gpu-value',
    category: 'GPU',
    kind: 'value',
    bindings: [{ key: 'gpu.usage.core', label: 'GPU' }],
    style: VALUE_STYLE,
  },
  {
    id: 'gpu-group',
    category: 'GPU',
    kind: 'group',
    title: 'GPU',
    bindings: [
      { key: 'gpu.usage.core', label: tx('usage') },
      { key: 'gpu.memory.used', label: 'VRAM' },
      { key: 'gpu.temperature.core', label: tx('temp') },
    ],
  },
  // Thermals
  {
    id: 'thermal',
    category: 'Thermals',
    kind: 'visualization',
    bindings: [
      { key: 'cpu.temperature.package', label: 'CPU' },
      { key: 'gpu.temperature.core', label: 'GPU' },
    ],
  },
  {
    id: 'cpu-temp-value',
    category: 'Thermals',
    kind: 'value',
    bindings: [{ key: 'cpu.temperature.package', label: 'CPU' }],
    style: VALUE_STYLE,
  },
  {
    id: 'gpu-temp-value',
    category: 'Thermals',
    kind: 'value',
    bindings: [{ key: 'gpu.temperature.core', label: 'GPU' }],
    style: VALUE_STYLE,
  },
  {
    id: 'ssd-temp',
    category: 'Thermals',
    kind: 'value',
    bindings: [{ key: 'storage.health.temperature', label: 'SSD' }],
    style: VALUE_STYLE,
  },
  // Storage
  {
    id: 'storage',
    category: 'Storage',
    kind: 'visualization',
    bindings: [
      { key: 'storage.io.read.bytes_per_second', label: tx('read') },
      { key: 'storage.io.write.bytes_per_second', label: tx('write') },
    ],
    size: { w: 6, h: 4 },
  },
  {
    id: 'volume',
    category: 'Storage',
    kind: 'visualization',
    bindings: [{ key: 'storage.volume.usage.percent', label: tx('disk') }],
    style: { renderer: 'bar' },
    size: { w: 4, h: 2 },
  },
  // Network
  {
    id: 'network',
    category: 'Network',
    kind: 'visualization',
    bindings: [
      { key: 'network.receive.bytes_per_second', label: tx('download') },
      { key: 'network.transmit.bytes_per_second', label: tx('upload') },
    ],
  },
  {
    id: 'network-down-value',
    category: 'Network',
    kind: 'value',
    bindings: [{ key: 'network.receive.bytes_per_second', label: tx('netDownCaps') }],
    style: VALUE_STYLE,
  },
  {
    id: 'wifi',
    category: 'Network',
    kind: 'value',
    bindings: [{ key: 'network.wifi.signal.rssi', label: 'Wi-Fi' }],
    style: VALUE_STYLE,
  },
  // Processes
  {
    id: 'processes',
    category: 'Processes',
    kind: 'group',
    title: tx('processes'),
    bindings: [
      { key: 'process.count.total', label: tx('processes') },
      { key: 'process.count.running', label: tx('running') },
      { key: 'process.thread.count.total', label: tx('threads') },
    ],
  },
  // Custom
  {
    id: 'summary',
    category: 'Custom',
    kind: 'summary',
    title: tx('system'),
    bindings: [
      { key: 'cpu.usage.total', label: 'CPU' },
      { key: 'gpu.usage.core', label: 'GPU' },
      { key: 'memory.usage.percent', label: 'RAM' },
      { key: 'network.receive.bytes_per_second', label: tx('netDownCaps') },
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
          ? t('metrics.notReported')
          : describeAvailability(definitions[0]!.availability);
    }
  }
  return { ok: false, reason: firstReason ?? t('metrics.noneSelected') };
}

/** A new widget from a blueprint, with fresh ids. Placement is the caller's. */
export function createWidget(blueprint: WidgetBlueprint): WidgetInstance {
  const bindings: WidgetBinding[] = blueprint.bindings.map((binding) => {
    const labelKey = keyOf(binding.label);
    return {
      key: binding.key,
      source: { mode: 'auto' },
      label: binding.label === undefined ? null : englishOf(binding.label),
      ...(labelKey ? { labelKey } : {}),
    };
  });
  const titleKey = keyOf(blueprint.title);
  const config = configFor(blueprint.bindings[0]!.key, blueprint.style);
  const limits = KIND_LIMITS[blueprint.kind];
  return {
    id: newId('w'),
    kind: blueprint.kind,
    title: blueprint.title === undefined ? null : englishOf(blueprint.title),
    ...(titleKey ? { titleKey } : {}),
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

/**
 * A widget for any single catalog metric — the *Custom* entry. Its label is
 * the metric's name: translated through the metric key when PULSE knows it,
 * the backend's own name otherwise.
 */
export function createCustomWidget(definition: MetricDefinition): WidgetInstance {
  const key = metricNameKey(definition.metric.key);
  return createWidget({
    id: 'custom',
    category: 'Custom',
    kind: 'visualization',
    bindings: [
      {
        key: definition.metric.key,
        label: hasKey(key) ? { key } : definition.displayName.slice(0, 24),
      },
    ],
  });
}

export function findBlueprint(id: string): WidgetBlueprint | undefined {
  return BLUEPRINTS.find((blueprint) => blueprint.id === id);
}
