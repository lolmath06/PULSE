import { t } from '@/i18n/i18n';
import type {
  ApplicationEntry,
  PackageRef,
  ProcessAffinity,
  ProcessClassification,
  ProcessEntry,
  ProcessField,
  ProcessPriority,
  ProcessState,
  Provenance,
  TrustStatus,
  WindowsPriorityClass,
} from '@/types/processes';

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
export type ProcessSortColumn = 'name' | 'pid' | 'cpu' | 'memory' | 'threads' | 'read' | 'write';

/** The columns an application table can be ordered by. */
export type ApplicationSortColumn = 'name' | 'processes' | 'cpu' | 'memory' | 'read' | 'write';

export type SortDirection = 'asc' | 'desc';

/** Which column, and which way. Kept across Refresh by the table's owner. */
export interface SortState<Column extends string> {
  readonly column: Column;
  readonly direction: SortDirection;
}

/** The order the tables open on: what is using the machine, most first. */
export const DEFAULT_PROCESS_SORT: SortState<ProcessSortColumn> = {
  column: 'cpu',
  direction: 'desc',
};
export const DEFAULT_APPLICATION_SORT: SortState<ApplicationSortColumn> = {
  column: 'cpu',
  direction: 'desc',
};

/**
 * The direction a column opens in when first chosen.
 *
 * Text reads A → Z; every numeric column opens largest first, because the
 * question a number column answers is "what is using the most".
 */
export function defaultDirection(column: ProcessSortColumn | ApplicationSortColumn): SortDirection {
  return column === 'name' ? 'asc' : 'desc';
}

/**
 * The sort after a header click — the Windows Task Manager behaviour.
 *
 * Clicking the **active** column flips its direction; clicking another column
 * selects it in that column's default direction. This is the function whose
 * absence made Phase 8's headers look sortable while never reversing.
 */
export function nextSort<Column extends ProcessSortColumn | ApplicationSortColumn>(
  current: SortState<Column>,
  column: Column,
): SortState<Column> {
  if (current.column === column) {
    return { column, direction: current.direction === 'asc' ? 'desc' : 'asc' };
  }
  return { column, direction: defaultDirection(column) };
}

/**
 * Compares two measurements in a direction.
 *
 * An unmeasured value (`null`) sorts **after** every measured one in both
 * directions. Flipping to ascending must put the idle processes first, not
 * the ones PULSE could not read — those are not "the smallest", they are
 * unknown, and treating them as zero would be a claim PULSE cannot make.
 */
function compareMeasured(
  left: number | null,
  right: number | null,
  direction: SortDirection,
): number {
  if (left === null && right === null) return 0;
  if (left === null) return 1;
  if (right === null) return -1;
  return direction === 'asc' ? left - right : right - left;
}

function compareText(left: string, right: string, direction: SortDirection): number {
  const order = left.localeCompare(right);
  return direction === 'asc' ? order : -order;
}

/**
 * Orders processes.
 *
 * Ties break on name then PID, always ascending, so several hundred rows at
 * exactly `0 %` keep a stable order across Refresh instead of trembling.
 * Returns a new array; the input is never mutated.
 */
export function sortProcesses(
  processes: readonly ProcessEntry[],
  sort: SortState<ProcessSortColumn>,
): ProcessEntry[] {
  const measured = (process: ProcessEntry): number | null => {
    switch (sort.column) {
      case 'pid':
        return process.pid;
      case 'memory':
        return process.residentMemoryBytes.value;
      case 'threads':
        return process.threadCount.value;
      case 'read':
        return process.readBytesPerSecond.value;
      case 'write':
        return process.writeBytesPerSecond.value;
      default:
        return process.cpuPercent.value;
    }
  };

  const tieBreak = (left: ProcessEntry, right: ProcessEntry): number =>
    left.name.localeCompare(right.name) || left.pid - right.pid;

  return [...processes].sort((left, right) => {
    if (sort.column === 'name') {
      return compareText(left.name, right.name, sort.direction) || left.pid - right.pid;
    }
    return (
      compareMeasured(measured(left), measured(right), sort.direction) || tieBreak(left, right)
    );
  });
}

/** Orders applications by the same rules, tie-breaking on name then key. */
export function sortApplications(
  applications: readonly ApplicationEntry[],
  sort: SortState<ApplicationSortColumn>,
): ApplicationEntry[] {
  const measured = (application: ApplicationEntry): number | null => {
    switch (sort.column) {
      case 'processes':
        return application.processCount;
      case 'memory':
        return application.residentMemoryBytes.value;
      case 'read':
        return application.readBytesPerSecond.value;
      case 'write':
        return application.writeBytesPerSecond.value;
      default:
        return application.cpuPercent.value;
    }
  };

  const tieBreak = (left: ApplicationEntry, right: ApplicationEntry): number =>
    left.displayName.localeCompare(right.displayName) || left.key.localeCompare(right.key);

  return [...applications].sort((left, right) => {
    if (sort.column === 'name') {
      return (
        compareText(left.displayName, right.displayName, sort.direction) ||
        left.key.localeCompare(right.key)
      );
    }
    return (
      compareMeasured(measured(left), measured(right), sort.direction) || tieBreak(left, right)
    );
  });
}

/** A field's value, or `null` when it was not measured. */
export function fieldValue(field: ProcessField<number>): number | null {
  return field.value;
}

// --- category filter ----------------------------------------------------------

/** The light filter above the tables. Nothing is ever hidden permanently. */
export type CategoryFilter = 'all' | 'user' | 'system' | 'kernel';

export function matchesCategory(
  classification: ProcessClassification,
  filter: CategoryFilter,
): boolean {
  switch (filter) {
    case 'user':
      return classification === 'userApplication';
    case 'system':
      return classification === 'systemProcess';
    case 'kernel':
      return classification === 'kernelThread';
    default:
      return true;
  }
}

export function formatCategory(classification: ProcessClassification): string {
  switch (classification) {
    case 'userApplication':
      return t('processes.category.user');
    case 'systemProcess':
      return t('processes.category.system');
    case 'kernelThread':
      return t('processes.category.kernel');
    default:
      return t('processes.category.unknown');
  }
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
      return t('processes.state.running');
    case 'sleepingOrWaiting':
      return t('processes.state.sleeping');
    case 'stopped':
      return t('processes.state.stopped');
    case 'zombie':
      return t('processes.state.zombie');
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

// --- inspector helpers ------------------------------------------------------------

/** The last component of a path, on either platform's separator. */
export function basename(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

/**
 * The terms *Search online* sends, from program-describing fields only.
 *
 * Name, executable **file name** and — when known — product, publisher or
 * package. Never a directory, never a PID, never an owner, never arguments.
 * The backend checks again and refuses anything path-like.
 */
export function searchTerms(input: {
  readonly name: string;
  readonly executablePath?: string | null;
  readonly productName?: string | null;
  readonly publisher?: string | null;
  readonly packageName?: string | null;
}): string[] {
  const terms: string[] = [];
  const add = (term: string | null | undefined) => {
    const clean = term?.trim();
    if (clean && !terms.some((existing) => existing.toLowerCase() === clean.toLowerCase())) {
      terms.push(clean);
    }
  };

  add(input.name);
  if (input.executablePath) add(basename(input.executablePath));
  add(input.productName);
  add(input.publisher ?? input.packageName);

  return terms.slice(0, 4);
}

/** A priority level's name, by id: `processes.priority.<id>`. */
export type PriorityLevel = 'realtime' | 'high' | 'aboveNormal' | 'normal' | 'belowNormal' | 'low';

export function priorityLabel(level: PriorityLevel): string {
  return t(`processes.priority.${level}`);
}

/** Linux nice presets: level → nice value. Mirrors `NICE_PRESETS` in Rust. */
export const NICE_PRESETS: readonly { readonly level: PriorityLevel; readonly value: number }[] = [
  { level: 'high', value: -10 },
  { level: 'aboveNormal', value: -5 },
  { level: 'normal', value: 0 },
  { level: 'belowNormal', value: 5 },
  { level: 'low', value: 19 },
];

export const NICE_MIN = -20;
export const NICE_MAX = 19;

/** Windows priority classes, most favoured last as Task Manager lists them. */
export const WINDOWS_PRIORITY_CLASSES: readonly {
  readonly level: PriorityLevel;
  readonly value: WindowsPriorityClass;
}[] = [
  { level: 'realtime', value: 'realtime' },
  { level: 'high', value: 'high' },
  { level: 'aboveNormal', value: 'aboveNormal' },
  { level: 'normal', value: 'normal' },
  { level: 'belowNormal', value: 'belowNormal' },
  { level: 'low', value: 'idle' },
];

/** A priority as the user reads it: the real system value, plus its preset. */
export function formatPriority(priority: ProcessPriority): string {
  if (priority.kind === 'nice') {
    const preset = NICE_PRESETS.find((entry) => entry.value === priority.value);
    return preset
      ? `nice ${priority.value} (${priorityLabel(preset.level)})`
      : `nice ${priority.value}`;
  }
  const level = WINDOWS_PRIORITY_CLASSES.find((entry) => entry.value === priority.class)?.level;
  return level ? priorityLabel(level) : priority.class;
}

/** Compresses a CPU list into ranges: `[0,1,2,3,8]` → `0–3, 8`. */
export function formatCpuList(cpus: readonly number[]): string {
  if (cpus.length === 0) return t('processes.affinityNone');
  const sorted = [...cpus].sort((a, b) => a - b);
  const ranges: string[] = [];
  let start = sorted[0] as number;
  let previous = start;
  for (const cpu of sorted.slice(1)) {
    if (cpu === previous + 1) {
      previous = cpu;
      continue;
    }
    ranges.push(start === previous ? `${start}` : `${start}–${previous}`);
    start = cpu;
    previous = cpu;
  }
  ranges.push(start === previous ? `${start}` : `${start}–${previous}`);
  return ranges.join(', ');
}

export function formatAffinity(affinity: ProcessAffinity): string {
  return t('processes.affinitySummary', {
    list: formatCpuList(affinity.cpus),
    count: affinity.cpus.length,
    total: affinity.available.length,
  });
}

export function formatTrust(trust: TrustStatus): string {
  switch (trust) {
    case 'trusted':
    case 'signedButUntrusted':
    case 'invalid':
    case 'unsigned':
    case 'permissionDenied':
      return t(`processes.trust.${trust}`);
    default:
      return t('processes.trust.unavailable');
  }
}

export function formatPackage(pkg: PackageRef): string {
  return `${pkg.name}-${pkg.version}.${pkg.arch}`;
}

/** One line of provenance for the inspector. Evidence, never a verdict. */
export function describeProvenance(provenance: Provenance): string {
  switch (provenance.kind) {
    case 'rpmPackage':
      return provenance.packages.map(formatPackage).join(', ');
    case 'notPackaged':
      switch (provenance.location) {
        case 'userHome':
          return t('processes.provenance.userHome');
        case 'localInstall':
          return t('processes.provenance.localInstall');
        default:
          return t('processes.provenance.notDetected');
      }
    case 'signature':
      return formatTrust(provenance.signature.trust);
    default:
      return provenance.reason;
  }
}

/**
 * How many descendants of `pid` the snapshot shows — the approximate count
 * the *End process tree* confirmation quotes. The backend re-plans from a
 * fresh table at the moment of the click.
 */
export function descendantCount(processes: readonly ProcessEntry[], pid: number): number {
  const children = new Map<number, number[]>();
  for (const process of processes) {
    if (process.parentPid !== null && process.parentPid !== process.pid) {
      const list = children.get(process.parentPid) ?? [];
      list.push(process.pid);
      children.set(process.parentPid, list);
    }
  }

  const seen = new Set<number>([pid]);
  const stack = [pid];
  let count = 0;
  while (stack.length > 0 && count < 4096) {
    const current = stack.pop() as number;
    for (const child of children.get(current) ?? []) {
      if (!seen.has(child)) {
        seen.add(child);
        stack.push(child);
        count += 1;
      }
    }
  }
  return count;
}

/** The field's value, or `null` — for fields whose type is not numeric. */
export function valueOf<T>(field: ProcessField<T>): T | null {
  return field.value;
}
