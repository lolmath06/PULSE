import type { MetricRef } from '@/types/metrics';
import { metricRefId } from '@/types/wellknown';
import { useMetricHistory } from '@/hooks/useMetricHistory';
import { describeAvailability } from '@/utils/metrics';
import { toVisualizationData } from '@/utils/history';
import { LIVE_CADENCE_MS, useLiveSeries } from '@/live/liveFeed';
import type { LivePoint } from '@/live/liveFeed';
import { rendererInfo } from '@/visualization/registry';
import type {
  VisualizationData,
  VisualizationMeta,
  VisualizationSeries,
} from '@/visualization/types';
import type { WidgetInstance } from '@/dashboard/model';
import type { ResolvedBinding } from '@/dashboard/bindings';
import { metaFor } from '@/dashboard/metricInfo';
import { widgetTitleText } from '@/dashboard/geometry';
import { t } from '@/i18n/i18n';

/**
 * Where a widget's numbers come from.
 *
 * - **live** — the shared 1-second feed (`src/live/liveFeed.ts`): current
 *   values and the last five minutes, in memory, never persisted.
 * - **history** — the SQLite history, at the resolution of the chosen range.
 *
 * `auto` picks live for everything that shows *now* (value, bar, gauge,
 * groups) and for sparklines — a thirty-second trend should not wait for
 * five-second history — and history for line and area charts, whose range can
 * be a week.
 */
export function effectiveDataMode(widget: WidgetInstance): 'live' | 'history' {
  if (widget.dataMode !== 'auto') return widget.dataMode;
  if (widget.kind === 'group' || widget.kind === 'summary') return 'live';
  const renderer = widget.visual.config.renderer;
  return renderer === 'sparkline' || rendererInfo(renderer).family === 'instant'
    ? 'live'
    : 'history';
}

export interface WidgetData {
  readonly data: VisualizationData;
  readonly meta: VisualizationMeta;
  /** Points per resolved binding, for group rows. */
  readonly live: ReadonlyMap<string, readonly LivePoint[]>;
  readonly mode: 'live' | 'history';
}

/** Live points → a visualization series. `null` readings stay gaps. */
export function liveSeriesOf(
  id: string,
  label: string,
  points: readonly LivePoint[],
): VisualizationSeries {
  const kept = points
    .filter((point): point is { t: number; v: number } => point.v !== null)
    .map((point) => ({ t: point.t, v: point.v }));
  const last = points[points.length - 1];
  return {
    id,
    label,
    points: kept,
    // The current value is the newest reading — unless it was unavailable,
    // in which case there is no current value, not an old one.
    latest: last && last.v !== null ? { t: last.t, v: last.v } : null,
  };
}

export function useWidgetData(
  widget: WidgetInstance,
  resolved: readonly ResolvedBinding[],
  enabled: boolean,
): WidgetData {
  const mode = effectiveDataMode(widget);
  const usable = resolved.filter(
    (entry): entry is Extract<ResolvedBinding, { ok: true }> => entry.ok,
  );
  const refs: MetricRef[] = usable.map((entry) => entry.ref);

  const history = useMetricHistory(refs, widget.visual.range, enabled && mode === 'history');
  const live = useLiveSeries(refs, enabled && mode === 'live');

  const first = usable[0];
  const meta = first
    ? metaFor(first.binding.key, widgetTitleText(widget) ?? first.label, first.definition)
    : metaFor(
        resolved[0]?.binding.key ?? 'cpu.usage.total',
        widgetTitleText(widget) ?? resolved[0]?.label ?? '—',
      );

  // Recomputed per render on purpose: renders happen once per live tick or
  // history answer, and the transformation is a few hundred points.
  const data: VisualizationData = (() => {
    if (usable.length === 0) {
      const reason = resolved.find((entry) => !entry.ok);
      return {
        status: 'unavailable',
        message: reason && !reason.ok ? reason.reason : t('metrics.noneSelected'),
        series: [],
        gapThresholdMs: 15_000,
      };
    }
    // A metric the backend says it cannot read is explained, not drawn as 0.
    const unreadable = usable.filter(
      (entry) => entry.definition.availability.status !== 'available',
    );
    if (unreadable.length === usable.length) {
      return {
        status: 'unavailable',
        message: describeAvailability(unreadable[0]!.definition.availability),
        series: usable.map((entry) => ({
          id: metricRefId(entry.ref),
          label: entry.label,
          points: [],
        })),
        gapThresholdMs: 15_000,
      };
    }

    if (mode === 'history') {
      return toVisualizationData(
        history.response,
        usable.map((entry) => ({ ref: entry.ref, label: entry.label })),
        history.status,
        history.reason,
      );
    }

    const refusedReason = usable
      .map((entry) => live.refused.get(metricRefId(entry.ref)))
      .find(Boolean);
    if (refusedReason && usable.every((entry) => live.refused.has(metricRefId(entry.ref)))) {
      return { status: 'unavailable', message: refusedReason, series: [], gapThresholdMs: 15_000 };
    }
    const series = usable.map((entry) =>
      liveSeriesOf(
        metricRefId(entry.ref),
        entry.label,
        live.points.get(metricRefId(entry.ref)) ?? [],
      ),
    );
    const newest = series.reduce((most, entry) => Math.max(most, entry.latest?.t ?? 0), 0);
    return {
      status: series.some((entry) => entry.points.length > 0) ? 'ready' : 'loading',
      series,
      gapThresholdMs: 3 * LIVE_CADENCE_MS,
      window: newest > 0 ? { fromMs: newest - 5 * 60_000, toMs: newest } : null,
    };
  })();

  return { data, meta, live: live.points, mode };
}
