import type {
  FileHash,
  ProcessActionResult,
  ProcessAffinity,
  ProcessDetails,
  ProcessPriority,
  ProcessQuery,
  ProcessSnapshot,
  Provenance,
} from '@/types/processes';
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

// --- inspector ----------------------------------------------------------------
//
// Every call names the `process:<pid>-<token>` identity from a snapshot. The
// backend re-validates PID and start token before reading or acting, so a
// recycled PID answers `staleProcess` instead of describing — or being acted
// on as — someone else.

/** The inspector's cheap facts. Lazy: only when a row is selected. */
export function getProcessDetails(instanceId: string): Promise<ProcessQuery<ProcessDetails>> {
  return invokeCommand('get_process_details', { instanceId });
}

/** The Fedora package or Windows signature. Separate because it can be slow. */
export function getProcessProvenance(instanceId: string): Promise<ProcessQuery<Provenance>> {
  return invokeCommand('get_process_provenance', { instanceId });
}

/** SHA-256 of the executable. Only ever on an explicit click. */
export function computeProcessSha256(instanceId: string): Promise<ProcessQuery<FileHash>> {
  return invokeCommand('compute_process_sha256', { instanceId });
}

export function getProcessPriority(instanceId: string): Promise<ProcessQuery<ProcessPriority>> {
  return invokeCommand('get_process_priority', { instanceId });
}

export function getProcessAffinity(instanceId: string): Promise<ProcessQuery<ProcessAffinity>> {
  return invokeCommand('get_process_affinity', { instanceId });
}

// --- controls -------------------------------------------------------------------
//
// Explicit user actions only. Nothing in PULSE calls these from a timer, a
// metric or a heuristic.

export function suspendProcess(instanceId: string): Promise<ProcessActionResult> {
  return invokeCommand('suspend_process', { instanceId });
}

export function resumeProcess(instanceId: string): Promise<ProcessActionResult> {
  return invokeCommand('resume_process', { instanceId });
}

/** *End process*; `force` is the separate Linux *Force kill* (SIGKILL). */
export function terminateProcess(instanceId: string, force: boolean): Promise<ProcessActionResult> {
  return invokeCommand('terminate_process', { instanceId, force });
}

export function terminateProcessTree(instanceId: string): Promise<ProcessActionResult> {
  return invokeCommand('terminate_process_tree', { instanceId });
}

export function setProcessPriority(
  instanceId: string,
  priority: ProcessPriority,
  confirmRealtime: boolean,
): Promise<ProcessActionResult> {
  return invokeCommand('set_process_priority', { instanceId, priority, confirmRealtime });
}

export function setProcessAffinity(
  instanceId: string,
  cpus: readonly number[],
): Promise<ProcessActionResult> {
  return invokeCommand('set_process_affinity', { instanceId, cpus });
}

// --- the only outbound doors -----------------------------------------------------

/** Reveals the executable of exactly this instance in the file manager. */
export function openProcessLocation(instanceId: string): Promise<ProcessActionResult> {
  return invokeCommand('open_process_location', { instanceId });
}

/**
 * Opens the default browser on a web search for program-describing terms.
 *
 * Nothing is sent before the click. The backend refuses paths, the home
 * directory and the user name, and percent-encodes the rest.
 */
export function openWebSearch(terms: readonly string[]): Promise<ProcessActionResult> {
  return invokeCommand('open_web_search', { terms });
}

/**
 * Opens the page for a SHA-256 digest: a web search or VirusTotal's file page.
 * Only the 64 hex characters are sent — never the file.
 */
export function openHashLookup(
  sha256: string,
  target: 'web' | 'virusTotal',
): Promise<ProcessActionResult> {
  return invokeCommand('open_hash_lookup', { sha256, target });
}
