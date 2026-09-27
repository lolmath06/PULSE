import { useEffect, useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import type { MetricRef } from '@/types/metrics';
import type { HistoryRange } from '@/types/history';
import { HISTORY_RANGES } from '@/types/history';
import { metricRefId } from '@/types/wellknown';
import { sampleMetrics } from '@/services/metrics';
import { useInViewport } from '@/hooks/useInViewport';
import { useMetricHistory } from '@/hooks/useMetricHistory';
import type { HistorySeriesSpec } from '@/utils/history';
import { toVisualizationData } from '@/utils/history';
import type { DeepPartial, VisualizationConfig } from '@/visualization/config';
import { CustomizePanel } from '@/visualization/CustomizePanel';
import { MetricVisualization } from '@/visualization/MetricVisualization';
import { useChartVisualization } from '@/visualization/store';
import type { ChartVisualization } from '@/visualization/store';
import type { VisualizationData, VisualizationMeta } from '@/visualization/types';

/**
 * A history section: title, range, Customize, and one visualization.
 *
 * Generic over the metric — the CPU, memory, thermal, GPU, storage, network
 * and process sections are all this component with different series, meta and
 * defaults. None of them draws anything itself.
 */
export interface HistoryPanelProps {
  /** Stable id the visualization choices are saved under, e.g. `cpu.total`. */
  readonly chartId: string;
  readonly title: string;
  readonly meta: VisualizationMeta;
  readonly series: readonly HistorySeriesSpec[];
  /** Module constant: the chart's first look. */
  readonly defaults: DeepPartial<VisualizationConfig>;
  /** Selectors shown next to the range, e.g. a device picker. */
  readonly controls?: ReactNode;
  readonly footnote?: ReactNode;
  /** Shown in place of the chart, e.g. `Telemetry unavailable`. */
  readonly replacement?: ReactNode;
  /** A metric whose latest value becomes the secondary line. */
  readonly secondary?: {
    readonly ref: MetricRef;
    readonly format: (value: number) => string;
  };
  /** Renders something other than the standard visualization. */
  readonly children?: (context: {
    readonly chart: ChartVisualization;
    readonly visible: boolean;
  }) => ReactNode;
}

export function RangeSelector({
  value,
  onChange,
}: {
  readonly value: HistoryRange;
  readonly onChange: (range: HistoryRange) => void;
}) {
  return (
    <div className="segmented" role="group" aria-label="Time range">
      {HISTORY_RANGES.map((range) => (
        <button
          key={range}
          type="button"
          className={`segmented__option${range === value ? ' segmented__option--active' : ''}`}
          aria-pressed={range === value}
          onClick={() => onChange(range)}
        >
          {range}
        </button>
      ))}
    </div>
  );
}

/**
 * When history itself cannot be read, one live sample — taken once, never
 * polled — lets the value, bar and gauge renderers still show *now*.
 */
function useLiveFallback(refs: readonly MetricRef[], wanted: boolean) {
  const [latest, setLatest] = useState<ReadonlyMap<string, { t: number; v: number }>>(new Map());
  const key = refs.map(metricRefId).join('|');

  useEffect(() => {
    if (!wanted || refs.length === 0) return;
    let cancelled = false;
    sampleMetrics(refs)
      .then((samples) => {
        if (cancelled) return;
        const map = new Map<string, { t: number; v: number }>();
        for (const sample of samples) {
          if (sample.value?.type === 'number') {
            map.set(metricRefId(sample.metric), { t: sample.timestamp, v: sample.value.value });
          }
        }
        setLatest(map);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
    // `key` stands for `refs`, whose identity changes on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, wanted]);

  return latest;
}

export function HistoryPanel({
  chartId,
  title,
  meta,
  series,
  defaults,
  controls,
  footnote,
  replacement,
  secondary,
  children,
}: HistoryPanelProps) {
  const chart = useChartVisualization(chartId, defaults);
  const [panelRef, visible] = useInViewport<HTMLElement>();
  const [customizing, setCustomizing] = useState(false);

  const refs = useMemo(
    () => [...series.map((spec) => spec.ref), ...(secondary ? [secondary.ref] : [])],
    [series, secondary],
  );
  const history = useMetricHistory(refs, chart.range, visible && !replacement);
  const live = useLiveFallback(
    series.map((spec) => spec.ref),
    history.status === 'unavailable',
  );

  const data: VisualizationData = useMemo(() => {
    const base = toVisualizationData(history.response, series, history.status, history.reason);
    if (history.status !== 'unavailable') return base;
    return {
      ...base,
      series: base.series.map((entry) => ({ ...entry, latest: live.get(entry.id) ?? null })),
    };
  }, [history, series, live]);

  const secondaryText = useMemo(() => {
    if (!secondary || !history.response) return undefined;
    const found = history.response.series.find(
      (entry) => metricRefId(entry.metric) === metricRefId(secondary.ref),
    );
    return found?.latest ? secondary.format(found.latest.v) : undefined;
  }, [secondary, history.response]);

  const effectiveMeta = useMemo(
    () => (secondaryText ? { ...meta, secondary: secondaryText } : meta),
    [meta, secondaryText],
  );

  return (
    <section ref={panelRef} className="card history-panel" aria-label={`${title} history`}>
      <header className="history-panel__header">
        <h2 className="card__title history-panel__title">{title}</h2>
        <div className="history-panel__tools">
          {controls}
          <RangeSelector value={chart.range} onChange={chart.setRange} />
          <button
            type="button"
            className="button button--quiet"
            aria-expanded={customizing}
            onClick={() => setCustomizing((open) => !open)}
          >
            Customize
          </button>
        </div>
      </header>

      {replacement ??
        (children ? (
          children({ chart, visible })
        ) : (
          <MetricVisualization data={data} meta={effectiveMeta} config={chart.config} />
        ))}

      {footnote && <p className="card__note">{footnote}</p>}

      {customizing && (
        <CustomizePanel
          title={title}
          chart={chart}
          data={data}
          meta={effectiveMeta}
          onClose={() => setCustomizing(false)}
        />
      )}
    </section>
  );
}
