/**
 * PULSE's visualization engine — the public surface.
 *
 * Everything a page, a dashboard widget or a future desktop overlay needs to
 * draw a metric, and nothing about where the data comes from. See
 * `docs/visualization/architecture.md` for the contract.
 */
export { MetricVisualization } from '@/visualization/MetricVisualization';
export type { MetricVisualizationProps } from '@/visualization/MetricVisualization';
export { CustomizePanel } from '@/visualization/CustomizePanel';
export * from '@/visualization/config';
export * from '@/visualization/presets';
export * from '@/visualization/registry';
export * from '@/visualization/types';
export { useChartVisualization } from '@/visualization/store';
export type { ChartVisualization, ChartVisualizationState } from '@/visualization/store';
export { formatParts, formatValue } from '@/visualization/format';
export { summarize, splitSegments } from '@/visualization/series';
