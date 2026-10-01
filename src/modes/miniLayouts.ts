import type { WidgetInstance } from '@/dashboard/model';
import { DEFAULT_FRAME } from '@/dashboard/model';
import type { MiniLayoutId } from '@/design/appearance';
import { K, chart, meter, ring, spark, strip, tx, value } from '@/presets/widgets';

/**
 * Mini's own layouts: a few readings sized for a small window, not a
 * dashboard squeezed into one column. Heights are in px; widths follow the
 * window.
 */
export interface MiniRow {
  readonly widget: WidgetInstance;
  readonly height: number;
}

/** Name and description: `modes.miniLayouts.<id>.name` / `.description`. */
export interface MiniLayout {
  readonly id: MiniLayoutId;
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
    rows: () => [
      row(spark([K.cpu], [320, 40]), 40, 'v-cpu'),
      row(spark([K.ram], [320, 40]), 40, 'v-ram'),
      row(spark([{ ...K.cpuTemp, label: tx('temp') }], [320, 40], true), 40, 'v-temp'),
      row(spark([{ ...K.down, label: tx('netDown') }], [320, 40]), 40, 'v-net'),
    ],
  },
  {
    id: 'thermals',
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
    rows: () => [
      row(chart([{ ...K.down, label: tx('download') }], [320, 90]), 90, 'n-down'),
      row(chart([{ ...K.up, label: tx('upload') }], [320, 90]), 90, 'n-up'),
      row(value(K.wifi, [320, 32]), 32, 'n-wifi'),
    ],
  },
  {
    id: 'focus',
    rows: () => [
      row(ring(K.cpu, [320, 150]), 150, 'f-cpu'),
      row(meter(K.ram, [320, 30]), 30, 'f-ram'),
      row(meter({ ...K.cpuTemp, label: tx('temp') }, [320, 30]), 30, 'f-temp'),
    ],
  },
];

export function findMiniLayout(id: unknown): MiniLayout {
  return MINI_LAYOUTS.find((layout) => layout.id === id) ?? MINI_LAYOUTS[0]!;
}
