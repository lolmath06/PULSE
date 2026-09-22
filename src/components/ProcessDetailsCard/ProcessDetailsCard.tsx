import { useCallback, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent } from 'react';
import type {
  ApplicationEntry,
  ProcessDetails,
  ProcessEntry,
  ProcessField,
} from '@/types/processes';
import { useProcessDetails } from '@/hooks/useProcessDetails';
import { useProcessActions } from '@/hooks/useProcessActions';
import type { ActionTarget } from '@/hooks/useProcessActions';
import {
  DEFAULT_APPLICATION_SORT,
  DEFAULT_PROCESS_SORT,
  TOP_PROCESS_COUNT,
  applicationMatches,
  applicationNames,
  descendantCount,
  formatProcessState,
  matchesCategory,
  nextSort,
  processMatches,
  sortApplications,
  sortProcesses,
} from '@/utils/processes';
import type {
  ApplicationSortColumn,
  CategoryFilter,
  ProcessSortColumn,
  SortState,
} from '@/utils/processes';
import { describeAvailability } from '@/utils/metrics';
import {
  formatBytes,
  formatCount,
  formatPercent,
  formatSampleTime,
  formatThroughput,
} from '@/utils/units';
import { ProcessInspector } from '@/components/ProcessInspector/ProcessInspector';
import {
  ApplicationContextMenu,
  ProcessContextMenu,
} from '@/components/ProcessContextMenu/ProcessContextMenu';
import { ConfirmDialog } from '@/components/ConfirmDialog/ConfirmDialog';
import { NiceDialog } from '@/components/ConfirmDialog/NiceDialog';
import { AffinityDialog } from '@/components/AffinityDialog/AffinityDialog';

type MenuState =
  | {
      readonly kind: 'process';
      readonly entry: ProcessEntry;
      readonly x: number;
      readonly y: number;
    }
  | {
      readonly kind: 'application';
      readonly application: ApplicationEntry;
      readonly x: number;
      readonly y: number;
    };

type DialogState =
  | { readonly kind: 'nice'; readonly entry: ProcessTargetInfo; readonly details: ProcessDetails }
  | {
      readonly kind: 'affinity';
      readonly entry: ProcessTargetInfo;
      readonly details: ProcessDetails;
    };

interface ProcessTargetInfo {
  readonly instanceId: string;
  readonly pid: number;
  readonly name: string;
}

interface Selection {
  readonly instanceId: string;
  readonly name: string;
  /** Set when opened from an application row. */
  readonly context?: string;
}

const CATEGORY_FILTERS: readonly { readonly value: CategoryFilter; readonly label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'user', label: 'User' },
  { value: 'system', label: 'System' },
  { value: 'kernel', label: 'Kernel' },
];

/**
 * What is running on this machine, what it is using — and, since Phase 9,
 * what each process *is* and what the user can do about it.
 *
 * # Observation and control stay separate
 *
 * The table comes from one snapshot per Refresh. Selecting a row opens the
 * inspector, which reads that one process lazily. Actions go through their
 * own commands, always name the selected *instance* (PID + start token), are
 * confirmed when destructive, and are followed by exactly one Refresh.
 * Nothing here runs on a timer.
 *
 * # Sorting
 *
 * Every numeric header opens largest-first and every click on the active
 * header reverses it; Name opens A → Z. Unmeasured values stay at the bottom
 * in both directions. The choice survives Refresh.
 */
export function ProcessDetailsCard() {
  const { status, snapshot, message, refreshing, refresh } = useProcessDetails();
  const actions = useProcessActions(refresh);
  const [view, setView] = useState<'applications' | 'processes'>('applications');
  const [processSort, setProcessSort] =
    useState<SortState<ProcessSortColumn>>(DEFAULT_PROCESS_SORT);
  const [applicationSort, setApplicationSort] =
    useState<SortState<ApplicationSortColumn>>(DEFAULT_APPLICATION_SORT);
  const [category, setCategory] = useState<CategoryFilter>('all');
  const [query, setQuery] = useState('');
  const [showAll, setShowAll] = useState(false);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [dialog, setDialog] = useState<DialogState | null>(null);
  const menuOrigin = useRef<HTMLElement | null>(null);

  const allProcesses = useMemo(() => snapshot?.processes ?? [], [snapshot?.processes]);

  const names = useMemo(
    () => applicationNames(snapshot?.applications ?? []),
    [snapshot?.applications],
  );

  const processes = useMemo(() => {
    const rows = allProcesses.filter(
      (process) =>
        matchesCategory(process.classification, category) &&
        processMatches(process, names.get(process.applicationKey), query),
    );
    return sortProcesses(rows, processSort);
  }, [allProcesses, names, query, processSort, category]);

  const applications = useMemo(() => {
    const rows = (snapshot?.applications ?? []).filter(
      (application) =>
        matchesCategory(application.classification, category) &&
        applicationMatches(application, query),
    );
    return sortApplications(rows, applicationSort);
  }, [snapshot?.applications, query, applicationSort, category]);

  const byInstance = useMemo(
    () => new Map(allProcesses.map((process) => [process.instanceId, process])),
    [allProcesses],
  );

  // A menu whose target vanished in a Refresh must not stay open: its
  // actions would name a process that no longer exists.
  const activeMenu =
    menu?.kind === 'process' && !byInstance.has(menu.entry.instanceId) ? null : menu;

  const closeMenu = useCallback(() => {
    setMenu(null);
    menuOrigin.current?.focus();
  }, []);

  const target = useCallback(
    (info: ProcessTargetInfo, details: ProcessDetails | null): ActionTarget => ({
      instanceId: info.instanceId,
      pid: info.pid,
      name: info.name,
      isSelf: details?.isSelf ?? false,
      forceKillSupported: details?.forceKillSupported ?? false,
      descendants: descendantCount(allProcesses, info.pid),
    }),
    [allProcesses],
  );

  const visibleProcesses = showAll ? processes : processes.slice(0, TOP_PROCESS_COUNT);
  const hidden = processes.length - visibleProcesses.length;
  const selectedEntry = selection ? (byInstance.get(selection.instanceId) ?? null) : null;

  const openProcessMenu = (entry: ProcessEntry, x: number, y: number, origin: HTMLElement) => {
    menuOrigin.current = origin;
    setMenu({ kind: 'process', entry, x, y });
  };

  const openApplicationMenu = (
    application: ApplicationEntry,
    x: number,
    y: number,
    origin: HTMLElement,
  ) => {
    menuOrigin.current = origin;
    setMenu({ kind: 'application', application, x, y });
  };

  const inspectApplication = (application: ApplicationEntry) => {
    const members = allProcesses
      .filter((process) => process.applicationKey === application.key)
      .sort(
        (left, right) =>
          (right.residentMemoryBytes.value ?? -1) - (left.residentMemoryBytes.value ?? -1),
      );
    const first = members[0];
    if (first) {
      setSelection({
        instanceId: first.instanceId,
        name: first.name,
        context: `1 of ${application.processCount} processes of ${application.displayName}`,
      });
    }
  };

  return (
    <div className="card" aria-label="Process details">
      <h2 className="card__title">Process details</h2>

      {status === 'loading' && <p className="card__muted">Walking the process table…</p>}

      {status === 'error' && (
        <p className="card__muted" title={message}>
          Backend unavailable. Run PULSE with <code>pnpm app:dev</code> to reach the Rust layer.
        </p>
      )}

      {status === 'ready' && snapshot !== null && (
        <>
          <dl className="kv">
            <div className="kv__row">
              <dt>Processes</dt>
              <dd>{formatCount(snapshot.counts.total)}</dd>
            </div>
            <div className="kv__row">
              <dt>Running</dt>
              <dd>{formatCount(snapshot.counts.running)}</dd>
            </div>
            <div className="kv__row">
              <dt>Threads</dt>
              <dd>{formatCount(snapshot.counts.threads)}</dd>
            </div>
          </dl>

          {snapshot.unsupportedReason !== null && (
            <p className="card__note">{snapshot.unsupportedReason}</p>
          )}

          {actions.notice !== null && (
            <div className={`process-notice process-notice--${actions.notice.tone}`} role="status">
              <span>{actions.notice.text}</span>
              <button
                type="button"
                className="process-notice__dismiss"
                aria-label="Dismiss"
                onClick={actions.dismissNotice}
              >
                ×
              </button>
            </div>
          )}

          <div className="process-controls">
            <div className="process-views" role="group" aria-label="Process view">
              <button
                type="button"
                className={viewClass(view === 'applications')}
                aria-pressed={view === 'applications'}
                onClick={() => setView('applications')}
              >
                Applications
              </button>
              <button
                type="button"
                className={viewClass(view === 'processes')}
                aria-pressed={view === 'processes'}
                onClick={() => setView('processes')}
              >
                Processes
              </button>
            </div>

            <div className="process-views" role="group" aria-label="Process category">
              {CATEGORY_FILTERS.map((filter) => (
                <button
                  key={filter.value}
                  type="button"
                  className={viewClass(category === filter.value)}
                  aria-pressed={category === filter.value}
                  onClick={() => setCategory(filter.value)}
                >
                  {filter.label}
                </button>
              ))}
            </div>

            <input
              type="search"
              className="process-search"
              placeholder="Search by name or PID"
              aria-label="Search processes by name or PID"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>

          {view === 'applications' ? (
            <ApplicationTable
              applications={applications}
              sort={applicationSort}
              onSort={(column) => setApplicationSort((current) => nextSort(current, column))}
              onMenu={openApplicationMenu}
            />
          ) : (
            <>
              <ProcessTable
                processes={visibleProcesses}
                sort={processSort}
                selectedId={selection?.instanceId ?? null}
                onSort={(column) => setProcessSort((current) => nextSort(current, column))}
                onSelect={(entry) =>
                  setSelection({ instanceId: entry.instanceId, name: entry.name })
                }
                onMenu={openProcessMenu}
              />
              {hidden > 0 && (
                <button
                  type="button"
                  className="button button--quiet"
                  onClick={() => setShowAll(true)}
                >
                  Show all {processes.length} processes
                </button>
              )}
            </>
          )}

          <div className="card__footer">
            <span className="card__muted">
              {`Updated ${formatSampleTime(snapshot.takenAt)} · collected in ${snapshot.durationMs} ms`}
            </span>
            <button
              type="button"
              className="button"
              onClick={refresh}
              disabled={refreshing}
              aria-label="Refresh process details"
            >
              {refreshing ? 'Refreshing…' : 'Refresh'}
            </button>
          </div>

          <p className="card__note">
            CPU is a share of this machine&rsquo;s <strong>entire</strong> capacity, so one thread
            saturating one logical processor is a small percentage, and the column adds up to
            roughly what the system CPU gauge shows. Read and Write are the platform&rsquo;s
            per-process I/O counters (see the documentation for how Fedora and Windows differ).
            Rates are measured <em>between</em> two snapshots, so they appear only after a refresh.
            Click a process to inspect it; right-click (or Shift+F10) for actions. Every action
            targets that exact process instance, is confirmed when destructive, and never runs on
            its own. Command lines are deliberately not collected.
          </p>
        </>
      )}

      {selection !== null && (
        <ProcessInspector
          key={selection.instanceId}
          instanceId={selection.instanceId}
          entry={selectedEntry}
          fallbackName={selection.name}
          revision={actions.revision}
          busy={actions.busy}
          context={selection.context}
          target={(details) =>
            target(
              {
                instanceId: selection.instanceId,
                pid: details?.pid ?? selectedEntry?.pid ?? 0,
                name: details?.name ?? selection.name,
              },
              details,
            )
          }
          handlers={{
            run: actions.run,
            copy: actions.copy,
            searchOnline: actions.searchOnline,
            lookupHash: actions.lookupHash,
            customNice: (details) =>
              setDialog({
                kind: 'nice',
                entry: { instanceId: details.instanceId, pid: details.pid, name: details.name },
                details,
              }),
            affinity: (details) =>
              setDialog({
                kind: 'affinity',
                entry: { instanceId: details.instanceId, pid: details.pid, name: details.name },
                details,
              }),
          }}
          onClose={() => setSelection(null)}
        />
      )}

      {activeMenu?.kind === 'process' && (
        <ProcessContextMenu
          entry={activeMenu.entry}
          x={activeMenu.x}
          y={activeMenu.y}
          target={(details) => target(activeMenu.entry, details)}
          handlers={{
            inspect: (entry) => setSelection({ instanceId: entry.instanceId, name: entry.name }),
            run: actions.run,
            copy: actions.copy,
            copyHash: actions.copyHash,
            searchOnline: actions.searchOnline,
            customNice: (entry, details) => setDialog({ kind: 'nice', entry, details }),
            affinity: (entry, details) => setDialog({ kind: 'affinity', entry, details }),
          }}
          onClose={closeMenu}
        />
      )}

      {activeMenu?.kind === 'application' && (
        <ApplicationContextMenu
          application={activeMenu.application}
          x={activeMenu.x}
          y={activeMenu.y}
          onInspect={inspectApplication}
          onShowProcesses={(application) => {
            setView('processes');
            setQuery(application.displayName);
          }}
          onSearch={actions.searchOnline}
          onClose={closeMenu}
        />
      )}

      {actions.pending !== null && (
        <ConfirmDialog
          title={actions.pending.title}
          body={actions.pending.body}
          confirmLabel={actions.pending.confirmLabel}
          tone={actions.pending.tone}
          onConfirm={actions.confirm}
          onCancel={actions.cancel}
        />
      )}

      {dialog?.kind === 'nice' && (
        <NiceDialog
          name={dialog.entry.name}
          pid={dialog.entry.pid}
          current={
            dialog.details.priority.value?.kind === 'nice'
              ? dialog.details.priority.value.value
              : null
          }
          onCancel={() => setDialog(null)}
          onApply={(value) => {
            setDialog(null);
            actions.run(
              { kind: 'priority', priority: { kind: 'nice', value } },
              target(dialog.entry, dialog.details),
            );
          }}
        />
      )}

      {dialog?.kind === 'affinity' && dialog.details.affinity.value !== null && (
        <AffinityDialog
          name={dialog.entry.name}
          pid={dialog.entry.pid}
          affinity={dialog.details.affinity.value}
          onCancel={() => setDialog(null)}
          onApply={(cpus) => {
            setDialog(null);
            actions.run({ kind: 'affinity', cpus }, target(dialog.entry, dialog.details));
          }}
        />
      )}
    </div>
  );
}

function viewClass(active: boolean): string {
  return active ? 'process-views__tab process-views__tab--active' : 'process-views__tab';
}

/** Whether a key press asks for the context menu: Shift+F10 or the Menu key. */
function isMenuKey(event: ReactKeyboardEvent): boolean {
  return event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10');
}

/** Where a keyboard-opened menu appears: under the row's start. */
function rowAnchor(element: HTMLElement): { x: number; y: number } {
  const rect = element.getBoundingClientRect();
  return { x: rect.left + 24, y: rect.bottom };
}

/**
 * A sortable column header, showing ↑ or ↓ on the active column.
 *
 * The glyph follows PULSE's convention, not the comparator's: ↓ is the order a
 * column opens in — largest first for numbers, A → Z for text — and ↑ its
 * reverse. `aria-sort` stays the literal data order (A → Z is `ascending`).
 */
function SortHeader<Column extends string>({
  label,
  column,
  sort,
  onSort,
  numeric = true,
  className,
}: {
  readonly label: string;
  readonly column: Column;
  readonly sort: SortState<Column>;
  readonly onSort: (column: Column) => void;
  readonly numeric?: boolean;
  readonly className?: string;
}) {
  const active = sort.column === column;
  const pointsDown = numeric ? sort.direction === 'desc' : sort.direction === 'asc';
  const arrow = active ? (pointsDown ? '↓' : '↑') : '';
  const classes = [numeric ? 'process-table__numeric' : '', className ?? '']
    .filter(Boolean)
    .join(' ');

  return (
    <th
      scope="col"
      className={classes || undefined}
      aria-sort={active ? (sort.direction === 'asc' ? 'ascending' : 'descending') : 'none'}
    >
      <button
        type="button"
        className={active ? 'process-sort process-sort--active' : 'process-sort'}
        onClick={() => onSort(column)}
      >
        {label}
        <span className="process-sort__arrow" aria-hidden="true">
          {arrow}
        </span>
      </button>
    </th>
  );
}

/**
 * One measured cell.
 *
 * The one place a missing value becomes a dash, so it can never happen by
 * accident: a value is formatted, and `null` becomes `—` carrying the
 * backend's own reason as its tooltip.
 */
function Cell({
  field,
  format,
  className,
}: {
  readonly field: ProcessField<number>;
  readonly format: (value: number) => string;
  readonly className?: string;
}) {
  const base = className ? `process-table__numeric ${className}` : 'process-table__numeric';
  if (field.value === null) {
    return (
      <td className={`${base} value--unavailable`} title={describeAvailability(field.availability)}>
        —
      </td>
    );
  }

  return <td className={base}>{format(field.value)}</td>;
}

function ApplicationTable({
  applications,
  sort,
  onSort,
  onMenu,
}: {
  readonly applications: readonly ApplicationEntry[];
  readonly sort: SortState<ApplicationSortColumn>;
  readonly onSort: (column: ApplicationSortColumn) => void;
  readonly onMenu: (
    application: ApplicationEntry,
    x: number,
    y: number,
    origin: HTMLElement,
  ) => void;
}) {
  if (applications.length === 0) {
    return <p className="card__muted">No application matches this search.</p>;
  }

  return (
    <table className="process-table" aria-label="Applications">
      <thead>
        <tr>
          <SortHeader
            label="Application"
            column="name"
            sort={sort}
            onSort={onSort}
            numeric={false}
          />
          <SortHeader label="Processes" column="processes" sort={sort} onSort={onSort} />
          <SortHeader label="CPU" column="cpu" sort={sort} onSort={onSort} />
          <SortHeader label="Memory" column="memory" sort={sort} onSort={onSort} />
          <SortHeader label="Read" column="read" sort={sort} onSort={onSort} />
          <SortHeader label="Write" column="write" sort={sort} onSort={onSort} />
        </tr>
      </thead>
      <tbody>
        {applications.map((application) => (
          <tr
            key={application.key}
            className="process-table__row"
            tabIndex={0}
            onContextMenu={(event: ReactMouseEvent<HTMLTableRowElement>) => {
              event.preventDefault();
              onMenu(application, event.clientX, event.clientY, event.currentTarget);
            }}
            onKeyDown={(event) => {
              if (isMenuKey(event)) {
                event.preventDefault();
                const { x, y } = rowAnchor(event.currentTarget);
                onMenu(application, x, y, event.currentTarget);
              }
            }}
          >
            <th scope="row" className="process-table__name">
              <span
                className="process-table__label"
                title={
                  application.identity === 'name'
                    ? 'Grouped by process name: PULSE could not read these processes’ executables, ' +
                      'so two unrelated programs with the same name would be counted together.'
                    : undefined
                }
              >
                {application.displayName}
              </span>
            </th>
            <td className="process-table__numeric">{formatCount(application.processCount)}</td>
            <Cell field={application.cpuPercent} format={(value) => formatPercent(value)} />
            <Cell field={application.residentMemoryBytes} format={(value) => formatBytes(value)} />
            <Cell field={application.readBytesPerSecond} format={formatThroughput} />
            <Cell field={application.writeBytesPerSecond} format={formatThroughput} />
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function ProcessTable({
  processes,
  sort,
  selectedId,
  onSort,
  onSelect,
  onMenu,
}: {
  readonly processes: readonly ProcessEntry[];
  readonly sort: SortState<ProcessSortColumn>;
  readonly selectedId: string | null;
  readonly onSort: (column: ProcessSortColumn) => void;
  readonly onSelect: (entry: ProcessEntry) => void;
  readonly onMenu: (entry: ProcessEntry, x: number, y: number, origin: HTMLElement) => void;
}) {
  if (processes.length === 0) {
    return <p className="card__muted">No process matches this search.</p>;
  }

  return (
    <table className="process-table" aria-label="Processes">
      <thead>
        <tr>
          <SortHeader label="Process" column="name" sort={sort} onSort={onSort} numeric={false} />
          <SortHeader label="PID" column="pid" sort={sort} onSort={onSort} />
          <th scope="col" className="process-table__secondary">
            State
          </th>
          <SortHeader label="CPU" column="cpu" sort={sort} onSort={onSort} />
          <SortHeader label="Memory" column="memory" sort={sort} onSort={onSort} />
          <SortHeader
            label="Threads"
            column="threads"
            sort={sort}
            onSort={onSort}
            className="process-table__secondary"
          />
          <SortHeader label="Read" column="read" sort={sort} onSort={onSort} />
          <SortHeader label="Write" column="write" sort={sort} onSort={onSort} />
        </tr>
      </thead>
      <tbody>
        {processes.map((process) => (
          <tr
            key={process.instanceId}
            className={
              process.instanceId === selectedId
                ? 'process-table__row process-table__row--selected'
                : 'process-table__row'
            }
            tabIndex={0}
            aria-selected={process.instanceId === selectedId}
            onClick={() => onSelect(process)}
            onContextMenu={(event: ReactMouseEvent<HTMLTableRowElement>) => {
              event.preventDefault();
              onMenu(process, event.clientX, event.clientY, event.currentTarget);
            }}
            onKeyDown={(event) => {
              if (isMenuKey(event)) {
                event.preventDefault();
                const { x, y } = rowAnchor(event.currentTarget);
                onMenu(process, x, y, event.currentTarget);
              } else if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                onSelect(process);
              }
            }}
          >
            <th scope="row" className="process-table__name">
              <span
                className="process-table__label"
                title={process.executablePath.value ?? undefined}
              >
                {process.name}
              </span>
            </th>
            <td className="process-table__numeric">{process.pid}</td>
            <td
              className="process-table__secondary"
              title={
                process.state === 'other'
                  ? describeAvailability(process.stateAvailability)
                  : undefined
              }
            >
              {formatProcessState(process.state)}
            </td>
            <Cell field={process.cpuPercent} format={(value) => formatPercent(value)} />
            <Cell field={process.residentMemoryBytes} format={(value) => formatBytes(value)} />
            <Cell
              field={process.threadCount}
              format={formatCount}
              className="process-table__secondary"
            />
            <Cell field={process.readBytesPerSecond} format={formatThroughput} />
            <Cell field={process.writeBytesPerSecond} format={formatThroughput} />
          </tr>
        ))}
      </tbody>
    </table>
  );
}
