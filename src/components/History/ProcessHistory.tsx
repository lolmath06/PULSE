import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  PROCESS_COUNT_RUNNING,
  PROCESS_COUNT_TOTAL,
  PROCESS_THREAD_COUNT_TOTAL,
} from '@/types/wellknown';
import type { HistorySeriesSpec } from '@/utils/history';
import type { HistoryMeta } from '@/components/History/chartDefaults';
import { COUNT_DEFAULTS } from '@/components/History/chartDefaults';
import { HistoryPanel } from '@/components/History/HistoryPanel';

const PROCESS_SERIES: readonly HistorySeriesSpec[] = [
  { ref: PROCESS_COUNT_TOTAL, label: 'Processes', labelKey: 'presets.text.processes' },
  { ref: PROCESS_COUNT_RUNNING, label: 'Running', labelKey: 'presets.text.running' },
];
const THREAD_SERIES: readonly HistorySeriesSpec[] = [
  { ref: PROCESS_THREAD_COUNT_TOTAL, label: 'Threads', labelKey: 'presets.text.threads' },
];

const PROCESS_META: HistoryMeta = {
  label: 'Processes',
  labelKey: 'presets.text.processes',
  unit: 'count',
  bounds: null,
  decimals: 0,
};
const THREAD_META: HistoryMeta = {
  label: 'Threads',
  labelKey: 'presets.text.threads',
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
  const { t } = useTranslation();
  const [view, setView] = useState<'processes' | 'threads'>('processes');
  const series = view === 'processes' ? PROCESS_SERIES : THREAD_SERIES;
  const meta = view === 'processes' ? PROCESS_META : THREAD_META;

  const toggle = useMemo(
    () => (
      <div className="segmented" role="group" aria-label={t('history.processes.count')}>
        {(
          [
            ['processes', t('presets.text.processes')],
            ['threads', t('presets.text.threads')],
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
    [view, t],
  );

  return (
    <HistoryPanel
      chartId="processes"
      title={t('history.processes.title')}
      meta={meta}
      series={series}
      defaults={COUNT_DEFAULTS}
      controls={toggle}
      footnote={t('history.processes.footnote')}
    />
  );
}
