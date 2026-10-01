import type { RendererKind, VisualizationConfig } from '@/visualization/config';
import { scaleError } from '@/visualization/config';
import type { Domain } from '@/visualization/scale';
import { valueExtent } from '@/visualization/series';
import type { VisualizationMeta, VisualizationSeries } from '@/visualization/types';
import { t } from '@/i18n/i18n';

/**
 * Every renderer PULSE ships, and what each needs.
 *
 * Adding a renderer is three steps: a kind in `config.ts`, an entry here, and
 * its component in `MetricVisualization`. Nothing else — no chart, panel or
 * page — has to learn about it; the Customize panel lists what is here.
 */
/** Label and description: `visualization.renderers.<kind>.label` / `.description`. */
export interface RendererInfo {
  readonly kind: RendererKind;
  /** `timeseries` draws history; `instant` draws the current value. */
  readonly family: 'timeseries' | 'instant';
}

export const RENDERERS: readonly RendererInfo[] = [
  { kind: 'line', family: 'timeseries' },
  { kind: 'area', family: 'timeseries' },
  { kind: 'sparkline', family: 'timeseries' },
  { kind: 'value', family: 'instant' },
  { kind: 'bar', family: 'instant' },
  { kind: 'gauge', family: 'instant' },
];

export function rendererInfo(kind: RendererKind): RendererInfo {
  return RENDERERS.find((entry) => entry.kind === kind) ?? RENDERERS[0]!;
}

export type RendererSupport =
  { readonly ok: true } | { readonly ok: false; readonly reason: string };

/**
 * The bounds a gauge may use: a valid fixed scale the user set, else the
 * metric's natural bounds, else **none**. A temperature has no natural
 * maximum, and PULSE does not pretend 100 °C is one.
 */
export function gaugeBounds(meta: VisualizationMeta, config: VisualizationConfig): Domain | null {
  if (config.scale.mode === 'fixed' && scaleError(config.scale.min, config.scale.max) === null) {
    return { min: config.scale.min!, max: config.scale.max! };
  }
  return meta.bounds ?? null;
}

/**
 * The bounds a bar uses. Like a gauge, but a metric without bounds is still
 * drawable: against the largest value in the window (`auto`), which the bar
 * says in its label.
 */
export function barBounds(
  meta: VisualizationMeta,
  config: VisualizationConfig,
  series: readonly VisualizationSeries[],
): Domain & { readonly auto: boolean } {
  const known = gaugeBounds(meta, config);
  if (known) return { ...known, auto: false };
  const extent = valueExtent(series);
  const max = extent && extent.max > 0 ? extent.max : 1;
  return { min: 0, max, auto: true };
}

/** Whether `kind` can honestly draw this metric with this configuration. */
export function rendererSupport(
  kind: RendererKind,
  meta: VisualizationMeta,
  config: VisualizationConfig,
): RendererSupport {
  if (kind === 'gauge' && !gaugeBounds(meta, config)) {
    return {
      ok: false,
      reason: t('visualization.gaugeNeedsBounds'),
    };
  }
  return { ok: true };
}
