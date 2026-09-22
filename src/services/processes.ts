import type { ProcessSnapshot } from '@/types/processes';
import { invokeCommand } from '@/services/tauri';

/**
 * The process boundary.
 *
 * Deliberately separate from `services/metrics`. A process snapshot is several
 * hundred rows that exist until the next refresh; a metric sample is a handful
 * of values against references a dashboard has saved. Routing the first
 * through the second would make every metric request carry the cost of the
 * process table — see `docs/metrics/processes.md`.
 */

/**
 * Walks the process table once.
 *
 * One call per refresh, whatever the machine runs — never one per process.
 * Rates are measured between this snapshot and the previous one, so the first
 * call reports CPU and I/O as waiting rather than as zero.
 */
export function getProcessSnapshot(): Promise<ProcessSnapshot> {
  return invokeCommand<ProcessSnapshot>('get_process_snapshot');
}
