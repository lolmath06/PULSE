import { MEMORY_USAGE_PERCENT, MEMORY_USED } from '@/types/wellknown';
import { formatBytes } from '@/utils/units';
import type { HistorySeriesSpec } from '@/utils/history';
import { MEMORY_DEFAULTS, MEMORY_META } from '@/components/History/chartDefaults';
import { HistoryPanel } from '@/components/History/HistoryPanel';

const SERIES: readonly HistorySeriesSpec[] = [{ ref: MEMORY_USAGE_PERCENT, label: 'Memory' }];

/** Formats the used-bytes line; module constant so the panel's memo holds. */
const SECONDARY = {
  ref: MEMORY_USED,
  format: (bytes: number) => `${formatBytes(bytes)} used`,
};

/**
 * Memory history: the usage percentage over time, with the bytes in use as a
 * secondary line — on the same 0–100 axis, never a second, unrelated one.
 */
export function MemoryHistory() {
  return (
    <HistoryPanel
      chartId="memory"
      title="Memory history"
      meta={MEMORY_META}
      series={SERIES}
      defaults={MEMORY_DEFAULTS}
      secondary={SECONDARY}
    />
  );
}
