import { listen } from '@tauri-apps/api/event';
import type { MetricRef } from '@/types/metrics';
import type {
  BatchRecorded,
  HistoryQueryResult,
  HistoryRange,
  HistoryStatus,
} from '@/types/history';
import { HISTORY_EVENT } from '@/types/history';
import { invokeCommand, isTauriRuntime } from '@/services/tauri';

/**
 * The history boundary.
 *
 * Read-only by design: there is no command to write a sample. The backend's
 * one scheduler records history; the UI only asks what was recorded.
 */

/** Asks for `metrics` over `range`. The backend bounds the point count. */
export function getMetricHistory(
  metrics: readonly MetricRef[],
  range: HistoryRange,
): Promise<HistoryQueryResult> {
  return invokeCommand<HistoryQueryResult>('get_metric_history', { metrics, range });
}

/** Reports the recorder. `includeDatabase` counts rows, so ask sparingly. */
export function getHistoryStatus(includeDatabase = false): Promise<HistoryStatus> {
  return invokeCommand<HistoryStatus>('get_history_status', { includeDatabase });
}

type Listener = (event: BatchRecorded) => void;

const listeners = new Set<Listener>();
let detach: Promise<() => void> | null = null;

/**
 * Calls `listener` after every batch the backend records.
 *
 * **One** Tauri listener serves every chart, however many are mounted: it is
 * attached with the first subscriber and detached with the last. This is the
 * only "clock" the history UI has — there is no interval anywhere in it.
 *
 * Returns the unsubscribe function.
 */
export function onHistorySample(listener: Listener): () => void {
  listeners.add(listener);

  if (!detach && isTauriRuntime()) {
    detach = listen<BatchRecorded>(HISTORY_EVENT, (event) => {
      for (const current of [...listeners]) current(event.payload);
    });
  }

  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && detach) {
      const pending = detach;
      detach = null;
      void pending.then((unlisten) => unlisten()).catch(() => undefined);
    }
  };
}

/** Delivers an event to current subscribers. For tests and the dev browser. */
export function emitHistorySampleForTesting(event: BatchRecorded): void {
  for (const current of [...listeners]) current(event);
}

/** How many subscribers are attached. For tests. */
export function historySubscriberCount(): number {
  return listeners.size;
}
