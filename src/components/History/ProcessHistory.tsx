import { useMemo, useState } from 'react';
import {
  PROCESS_COUNT_RUNNING,
  PROCESS_COUNT_TOTAL,
  PROCESS_THREAD_COUNT_TOTAL,
} from '@/types/wellknown';
import type { HistorySeriesSpec } from '@/utils/history';
import type { VisualizationMeta } from '@/visualization/types';
import { COUNT_DEFAULTS } from '@/components/History/chartDefaults';
import { HistoryPanel } from '@/components/History/HistoryPanel';

const PROCESS_SERIES: readonly HistorySeriesSpec[] = [
  { ref: PROCESS_COUNT_TOTAL, label: 'Processes' },
  { ref: PROCESS_COUNT_RUNNING, label: 'Running' },
];
const THREAD_SERIES: readonly HistorySeriesSpec[] = [
  { ref: PROCESS_THREAD_COUNT_TOTAL, label: 'Threads' },
];

const PROCESS_META: VisualizationMeta = {
  label: 'Processes',
  unit: 'count',
  bounds: null,
  decimals: 0,
};
const THREAD_META: VisualizationMeta = {
  label: 'Threads',
  unit: 'count',
  bounds: null,
  decimals: 0,
};

/**
 * Machine-wide process and thread counts over time.
 *
 * Only the three `process:system` metrics have history. No individual process
 * is ever recorded — see `docs/history/architecture.md`.
 */
export function ProcessHistory() {
  const [view, setView] = useState<'processes' | 'threads'>('processes');
  const series = view === 'processes' ? PROCESS_SERIES : THREAD_SERIES;
  const meta = view === 'processes' ? PROCESS_META : THREAD_META;

  const toggle = useMemo(
    () => (
      <div className="segmented" role="group" aria-label="Process count">
        {(
          [
            ['processes', 'Processes'],
            ['threads', 'Threads'],
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
    ),
    [view],
  );

  return (
    <HistoryPanel
      chartId="processes"
      title="Process count history"
      meta={meta}
      series={series}
      defaults={COUNT_DEFAULTS}
      controls={toggle}
      footnote="Machine-wide counts only. PULSE never records the history of an individual process."
    />
  );
}
