import { useMemo, useState } from 'react';
import { CPU_USAGE_LOGICAL_KEY } from '@/types/wellknown';
import type { HistoryRange } from '@/types/history';
import { useMetricCatalog } from '@/hooks/useMetricCatalog';
import { useMetricHistory } from '@/hooks/useMetricHistory';
import { discoverLogicalProcessors, processorMetric } from '@/utils/cpu';
import type { LogicalProcessor } from '@/utils/cpu';
import type { HistorySeriesSpec } from '@/utils/history';
import { toVisualizationData } from '@/utils/history';
import type { VisualizationConfig } from '@/visualization/config';
import { mergeConfig } from '@/visualization/config';
import { MetricVisualization } from '@/visualization/MetricVisualization';
import { summarize } from '@/visualization/series';
import { CPU_DEFAULTS, CPU_META, LOGICAL_CELL_STYLE } from '@/components/History/chartDefaults';
import { HistoryPanel } from '@/components/History/HistoryPanel';
import { CPU_TOTAL_SERIES } from '@/components/History/series';

/**
 * CPU history.
 *
 * # CPU Total is `cpu.usage.total`, full stop
 *
 * The main chart plots the backend's own machine-wide metric. It is **not**
 * an average of the logical processors recomputed here: the backend already
 * measures the aggregate from the kernel's (or Windows') own totals, which is
 * exact, while a mean of rounded per-processor percentages sampled a moment
 * apart is not — and on Windows the per-processor counters can be unavailable
 * while the total is fine.
 *
 * # Logical processors, without 32 colours
 *
 * The per-processor view is small multiples: one tiny sparkline each, the
 * most active first, the first eight by default. Never 32 overlaid lines.
 */

/** How many processors the per-processor view shows before *Show all*. */
export const LOGICAL_VIEW_LIMIT = 8;

type View = 'total' | 'logical';

export function CpuHistory() {
  const [view, setView] = useState<View>('total');

  const toggle = (
    <div className="segmented" role="group" aria-label="CPU view">
      {(
        [
          ['total', 'Total'],
          ['logical', 'Logical processors'],
        ] as const
      ).map(([value, text]) => (
        <button
          key={value}
          type="button"
          className={`segmented__option${view === value ? ' segmented__option--active' : ''}`}
          aria-pressed={view === value}
          onClick={() => setView(value)}
        >
          {text}
        </button>
      ))}
    </div>
  );

  return (
    <HistoryPanel
      chartId="cpu.total"
      title="CPU history"
      meta={CPU_META}
      series={CPU_TOTAL_SERIES}
      defaults={CPU_DEFAULTS}
      controls={toggle}
      footnote={
        view === 'total'
          ? 'CPU Total is the machine-wide usage the backend measures across every logical processor — not an average rebuilt from them.'
          : 'The most active logical processors over the selected window. CPU Total stays the reference.'
      }
    >
      {view === 'logical'
        ? ({ chart, visible }) => (
            <LogicalProcessorGrid config={chart.config} range={chart.range} enabled={visible} />
          )
        : undefined}
    </HistoryPanel>
  );
}

function LogicalProcessorGrid({
  config,
  range,
  enabled,
}: {
  readonly config: VisualizationConfig;
  readonly range: HistoryRange;
  readonly enabled: boolean;
}) {
  const { catalog, status: catalogStatus } = useMetricCatalog();
  const [showAll, setShowAll] = useState(false);

  const processors: LogicalProcessor[] = useMemo(
    () => discoverLogicalProcessors(catalog),
    [catalog],
  );
  const specs: HistorySeriesSpec[] = useMemo(
    () =>
      processors.map((processor) => ({
        ref: processorMetric(processor.ordinal, CPU_USAGE_LOGICAL_KEY),
        label: processor.label,
      })),
    [processors],
  );
  const history = useMetricHistory(
    specs.map((spec) => spec.ref),
    range,
    enabled && specs.length > 0,
  );
  const data = useMemo(
    () => toVisualizationData(history.response, specs, history.status, history.reason),
    [history, specs],
  );

  const ranked = useMemo(
    () =>
      data.series
        .map((series) => ({ series, average: summarize(series).average ?? -1 }))
        .sort((left, right) => right.average - left.average),
    [data.series],
  );
  const cellConfig = useMemo(() => mergeConfig(config, LOGICAL_CELL_STYLE), [config]);

  if (catalogStatus === 'loading') return <p className="card__muted">Reading processors…</p>;
  if (processors.length === 0) {
    return (
      <p className="card__note">No individual logical processor is exposed on this machine.</p>
    );
  }

  const shown = showAll ? ranked : ranked.slice(0, LOGICAL_VIEW_LIMIT);

  return (
    <>
      <ul className="history-multiples" aria-label="Logical processor history">
        {shown.map(({ series }) => (
          <li key={series.id} className="history-multiples__cell">
            <MetricVisualization
              data={{ ...data, series: [series] }}
              meta={{ ...CPU_META, label: series.label }}
              config={cellConfig}
              height={56}
            />
          </li>
        ))}
      </ul>
      {ranked.length > LOGICAL_VIEW_LIMIT && (
        <button
          type="button"
          className="button button--quiet"
          onClick={() => setShowAll((all) => !all)}
        >
          {showAll
            ? `Show the ${LOGICAL_VIEW_LIMIT} most active`
            : `Show all ${ranked.length} processors`}
        </button>
      )}
    </>
  );
}
