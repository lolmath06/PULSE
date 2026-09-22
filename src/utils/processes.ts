import type { ApplicationEntry, ProcessEntry, ProcessField, ProcessState } from '@/types/processes';

/**
 * Sorting, filtering and describing a process table.
 *
 * Pure and free of React, so the ordering and search rules are tested directly
 * rather than through a rendered component — and so a machine with four
 * hundred processes exercises the same code as a fixture with three.
 *
 * # Nothing here invents a number
 *
 * Every cell comes from the backend as a `ProcessField`: a value, or `null`
 * with the reason. A missing CPU share is never rendered as `0 %`, and a
 * measured zero is never rendered as `—`. The two are different facts and the
 * table shows which is which.
 */

/** How many processes the table shows before *Show all*. */
export const TOP_PROCESS_COUNT = 20;

/** The columns a process table can be ordered by. */
export type ProcessSortColumn = 'cpu' | 'memory' | 'read' | 'write' | 'name';

/** The columns an application table can be ordered by. Identical set. */
export type ApplicationSortColumn = ProcessSortColumn;

/** The column the tables open on: what is using the machine, most first. */
export const DEFAULT_SORT: ProcessSortColumn = 'cpu';

/** A field's value, or `null` when it was not measured. */
export function fieldValue(field: ProcessField<number>): number | null {
  return field.value;
}

/**
 * The sort weight of a field.
 *
 * Unmeasured values sort **last**, whatever the column, rather than being
 * treated as zero. A process whose CPU is still waiting for a baseline is not
 * "the least busy process on the machine"; it is a process PULSE has not
 * measured yet, and putting it above genuinely idle processes would be a claim
 * PULSE cannot make.
 */
function weight(field: ProcessField<number>): number {
  return field.value ?? Number.NEGATIVE_INFINITY;
}

/**
 * Orders processes.
 *
 * Numeric columns descend — the question is always "what is using the most" —
 * and `name` ascends. Ties break on display name and then PID, which is what
 * keeps the table from trembling: several hundred processes sit at exactly
 * `0 %`, and without a deterministic order behind the sorted column they would
 * reshuffle on every *Refresh*.
 *
 * Returns a new array; the input is never mutated.
 */
export function sortProcesses(
  processes: readonly ProcessEntry[],
  column: ProcessSortColumn,
): ProcessEntry[] {
  const pick = (process: ProcessEntry): ProcessField<number> => {
    switch (column) {
      case 'memory':
        return process.residentMemoryBytes;
      case 'read':
        return process.readBytesPerSecond;
      case 'write':
        return process.writeBytesPerSecond;
      default:
        return process.cpuPercent;
    }
  };

  const tieBreak = (left: ProcessEntry, right: ProcessEntry): number =>
    left.name.localeCompare(right.name) || left.pid - right.pid;

  return [...processes].sort((left, right) => {
    if (column === 'name') {
      return tieBreak(left, right);
    }
    return weight(pick(right)) - weight(pick(left)) || tieBreak(left, right);
  });
}

/**
 * Orders applications, by the same rules, tie-breaking on name then key.
 */
export function sortApplications(
  applications: readonly ApplicationEntry[],
  column: ApplicationSortColumn,
): ApplicationEntry[] {
  const pick = (application: ApplicationEntry): ProcessField<number> => {
    switch (column) {
      case 'memory':
        return application.residentMemoryBytes;
      case 'read':
        return application.readBytesPerSecond;
      case 'write':
        return application.writeBytesPerSecond;
      default:
        return application.cpuPercent;
    }
  };

  const tieBreak = (left: ApplicationEntry, right: ApplicationEntry): number =>
    left.displayName.localeCompare(right.displayName) || left.key.localeCompare(right.key);

  return [...applications].sort((left, right) => {
    if (column === 'name') {
      return tieBreak(left, right);
    }
    return weight(pick(right)) - weight(pick(left)) || tieBreak(left, right);
  });
}

/**
 * Whether a process matches a search.
 *
 * Matches the process name, the PID and the application it was grouped into.
 * Case-insensitive, substring, and entirely local: searching a few hundred
 * rows the frontend already holds does not need a backend round trip, and
 * asking for one would make every keystroke a process-table walk.
 */
export function processMatches(
  process: ProcessEntry,
  applicationName: string | undefined,
  query: string,
): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === '') return true;

  return (
    process.name.toLowerCase().includes(needle) ||
    String(process.pid).includes(needle) ||
    (applicationName ?? '').toLowerCase().includes(needle)
  );
}

/** Whether an application matches a search: its name, or any member's name. */
export function applicationMatches(application: ApplicationEntry, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === '') return true;

  return application.displayName.toLowerCase().includes(needle);
}

/** Maps every process to the display name of the application it belongs to. */
export function applicationNames(
  applications: readonly ApplicationEntry[],
): ReadonlyMap<string, string> {
  return new Map(applications.map((application) => [application.key, application.displayName]));
}

/** The user-facing label of a process state. */
export function formatProcessState(state: ProcessState): string {
  switch (state) {
    case 'running':
      return 'Running';
    case 'sleepingOrWaiting':
      return 'Sleeping';
    case 'stopped':
      return 'Stopped';
    case 'zombie':
      return 'Zombie';
    default:
      return '—';
  }
}

/**
 * Whether the whole table is still waiting for its first interval.
 *
 * True when no process has a CPU share **and** at least one is waiting for a
 * baseline — as opposed to a machine where every CPU reading was refused,
 * which is a different message.
 */
export function isWaitingForBaseline(processes: readonly ProcessEntry[]): boolean {
  if (processes.length === 0) return false;

  return (
    processes.every((process) => process.cpuPercent.value === null) &&
    processes.some((process) => process.cpuPercent.availability.status === 'temporarilyUnavailable')
  );
}
