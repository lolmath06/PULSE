import type { WidgetInstance } from '@/dashboard/model';
import { DEFAULT_FRAME } from '@/dashboard/model';
import type { MiniLayoutId } from '@/design/appearance';
import { K, chart, meter, ring, spark, strip, value } from '@/presets/widgets';

/**
 * Mini's own layouts: a few readings sized for a small window, not a
 * dashboard squeezed into one column. Heights are in px; widths follow the
 * window.
 */
export interface MiniRow {
  readonly widget: WidgetInstance;
  readonly height: number;
}

export interface MiniLayout {
  readonly id: MiniLayoutId;
  readonly name: string;
  readonly description: string;
  readonly rows: () => MiniRow[];
}

/** Each row is a card in the style's shape (its frame follows the style). */
const row = (widget: WidgetInstance, height: number, id: string): MiniRow => ({
  widget: { ...widget, id: `w-mini-${id}`, frame: { ...DEFAULT_FRAME, showTitle: false } },
  height,
});

export const MINI_LAYOUTS: readonly MiniLayout[] = [
  {
    id: 'vitals',
    name: 'Vitals',
    description: 'CPU, memory, temperature and traffic, each with its trend.',
    rows: () => [
      row(spark([K.cpu], [320, 40]), 40, 'v-cpu'),
      row(spark([K.ram], [320, 40]), 40, 'v-ram'),
      row(spark([{ ...K.cpuTemp, label: 'Temp' }], [320, 40], true), 40, 'v-temp'),
      row(spark([{ ...K.down, label: 'Net ↓' }], [320, 40]), 40, 'v-net'),
    ],
  },
  {
    id: 'thermals',
    name: 'Thermals',
    description: 'Temperatures over time, colour-banded, with the SSD.',
    rows: () => [
      row(chart([K.cpuTemp, K.gpuTemp], [320, 120], 'line', true), 120, 't-chart'),
      row(
        strip(
          [{ ...K.cpuTemp, label: 'CPU' }, { ...K.gpuTemp, label: 'GPU' }, K.ssdTemp],
          [320, 34],
        ),
        34,
        't-strip',
      ),
    ],
  },
  {
    id: 'network',
    name: 'Network',
    description: 'Download and upload, with Wi-Fi signal where there is one.',
    rows: () => [
      row(chart([{ ...K.down, label: 'Download' }], [320, 90]), 90, 'n-down'),
      row(chart([{ ...K.up, label: 'Upload' }], [320, 90]), 90, 'n-up'),
      row(value(K.wifi, [320, 32]), 32, 'n-wifi'),
    ],
  },
  {
    id: 'focus',
    name: 'Focus',
    description: 'One big CPU ring, memory and heat as meters.',
    rows: () => [
      row(ring(K.cpu, [320, 150]), 150, 'f-cpu'),
      row(meter(K.ram, [320, 30]), 30, 'f-ram'),
      row(meter({ ...K.cpuTemp, label: 'Temp' }, [320, 30]), 30, 'f-temp'),
    ],
  },
];

export function findMiniLayout(id: unknown): MiniLayout {
  return MINI_LAYOUTS.find((layout) => layout.id === id) ?? MINI_LAYOUTS[0]!;
}
