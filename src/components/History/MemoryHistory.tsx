import { useTranslation } from 'react-i18next';
import { MEMORY_USAGE_PERCENT, MEMORY_USED } from '@/types/wellknown';
import { t } from '@/i18n/i18n';
import { formatBytes } from '@/utils/units';
import type { HistorySeriesSpec } from '@/utils/history';
import { MEMORY_DEFAULTS, MEMORY_META } from '@/components/History/chartDefaults';
import { HistoryPanel } from '@/components/History/HistoryPanel';

const SERIES: readonly HistorySeriesSpec[] = [
  { ref: MEMORY_USAGE_PERCENT, label: 'Memory', labelKey: 'presets.text.memory' },
];

/** Formats the used-bytes line; module constant so the panel's memo holds. */
const SECONDARY = {
  ref: MEMORY_USED,
  format: (bytes: number) => t('history.memory.used', { value: formatBytes(bytes) }),
};

/**
 * Memory history: the usage percentage over time, with the bytes in use as a
 * secondary line — on the same 0–100 axis, never a second, unrelated one.
 */
export function MemoryHistory() {
  useTranslation();
  return (
    <HistoryPanel
      chartId="memory"
      title={t('history.memory.title')}
      meta={MEMORY_META}
      series={SERIES}
      defaults={MEMORY_DEFAULTS}
      secondary={SECONDARY}
    />
  );
}
