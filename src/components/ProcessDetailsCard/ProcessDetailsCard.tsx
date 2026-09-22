import { useMemo, useState } from 'react';
import type { ApplicationEntry, ProcessEntry, ProcessField } from '@/types/processes';
import { useProcessDetails } from '@/hooks/useProcessDetails';
import {
  DEFAULT_SORT,
  TOP_PROCESS_COUNT,
  applicationMatches,
  applicationNames,
  formatProcessState,
  processMatches,
  sortApplications,
  sortProcesses,
} from '@/utils/processes';
import type { ProcessSortColumn } from '@/utils/processes';
import { describeAvailability } from '@/utils/metrics';
import {
  formatBytes,
  formatCount,
  formatPercent,
  formatSampleTime,
  formatThroughput,
} from '@/utils/units';

/**
 * What is running on this machine, and what it is using.
 *
 * # Two views of the same snapshot
 *
 * **Applications** is the default because it answers the question people
 * actually have. Thirteen `firefox` rows say what is running; one Firefox row
 * with thirteen processes says what is using the machine. **Processes** is one
 * row per process, for when that is the question.
 *
 * Both are grouped and sorted from a single snapshot — there is no second
 * backend call, and no per-process request at any point.
 *
 * # CPU is a share of the whole machine
 *
 * `100 %` means every logical processor saturated, so one thread pinning one
 * of thirty-two is `3.1 %`. That convention is what lets this column add up to
 * something the system CPU gauge could plausibly show, on both platforms.
 *
 * # Nothing is invented
 *
 * A cell is a number or a `—` with the reason in its tooltip. Before a second
 * snapshot exists there is no interval to divide by, so CPU, Read and Write
 * are `—`; afterwards a genuine `0 B/s` is shown as `0 B/s`. A process whose
 * I/O counters were refused keeps every other column.
 */
export function ProcessDetailsCard() {
  const { status, snapshot, message, refreshing, refresh } = useProcessDetails();
  const [view, setView] = useState<'applications' | 'processes'>('applications');
  const [sort, setSort] = useState<ProcessSortColumn>(DEFAULT_SORT);
  const [query, setQuery] = useState('');
  const [showAll, setShowAll] = useState(false);

  const names = useMemo(
    () => applicationNames(snapshot?.applications ?? []),
    [snapshot?.applications],
  );

  const processes = useMemo(() => {
    const rows = (snapshot?.processes ?? []).filter((process) =>
      processMatches(process, names.get(process.applicationKey), query),
    );
    return sortProcesses(rows, sort);
  }, [snapshot?.processes, names, query, sort]);

  const applications = useMemo(() => {
    const rows = (snapshot?.applications ?? []).filter((application) =>
      applicationMatches(application, query),
    );
    return sortApplications(rows, sort);
  }, [snapshot?.applications, query, sort]);

  const visibleProcesses = showAll ? processes : processes.slice(0, TOP_PROCESS_COUNT);
  const hidden = processes.length - visibleProcesses.length;

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

            <input
              type="search"
              className="process-search"
              placeholder="Search processes…"
              aria-label="Search processes"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>

          {view === 'applications' ? (
            <ApplicationTable applications={applications} sort={sort} onSort={setSort} />
          ) : (
            <>
              <ProcessTable processes={visibleProcesses} sort={sort} onSort={setSort} />
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
            roughly what the system CPU gauge shows. Read and Write count bytes that actually moved
            to or from storage — not cached reads, and not network traffic. Rates are measured{' '}
            <em>between</em> two snapshots, so they appear only after a refresh. PULSE observes
            processes and never controls them: there is no way to stop, suspend or reprioritise
            anything from here, and command lines are deliberately not collected.
          </p>
        </>
      )}
    </div>
  );
}

function viewClass(active: boolean): string {
  return active ? 'process-views__tab process-views__tab--active' : 'process-views__tab';
}

/** A sortable column header. */
function SortHeader({
  label,
  column,
  sort,
  onSort,
  numeric = true,
}: {
  readonly label: string;
  readonly column: ProcessSortColumn;
  readonly sort: ProcessSortColumn;
  readonly onSort: (column: ProcessSortColumn) => void;
  readonly numeric?: boolean;
}) {
  const active = sort === column;

  return (
    <th scope="col" className={numeric ? 'process-table__numeric' : undefined}>
      <button
        type="button"
        className={active ? 'process-sort process-sort--active' : 'process-sort'}
        aria-pressed={active}
        onClick={() => onSort(column)}
      >
        {label}
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
}: {
  readonly field: ProcessField<number>;
  readonly format: (value: number) => string;
}) {
  if (field.value === null) {
    return (
      <td
        className="process-table__numeric value--unavailable"
        title={describeAvailability(field.availability)}
      >
        —
      </td>
    );
  }

  return <td className="process-table__numeric">{format(field.value)}</td>;
}

function ApplicationTable({
  applications,
  sort,
  onSort,
}: {
  readonly applications: readonly ApplicationEntry[];
  readonly sort: ProcessSortColumn;
  readonly onSort: (column: ProcessSortColumn) => void;
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
          <th scope="col" className="process-table__numeric">
            Processes
          </th>
          <SortHeader label="CPU" column="cpu" sort={sort} onSort={onSort} />
          <SortHeader label="Memory" column="memory" sort={sort} onSort={onSort} />
          <SortHeader label="Read" column="read" sort={sort} onSort={onSort} />
          <SortHeader label="Write" column="write" sort={sort} onSort={onSort} />
        </tr>
      </thead>
      <tbody>
        {applications.map((application) => (
          <tr key={application.key}>
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
  onSort,
}: {
  readonly processes: readonly ProcessEntry[];
  readonly sort: ProcessSortColumn;
  readonly onSort: (column: ProcessSortColumn) => void;
}) {
  if (processes.length === 0) {
    return <p className="card__muted">No process matches this search.</p>;
  }

  return (
    <table className="process-table" aria-label="Processes">
      <thead>
        <tr>
          <SortHeader label="Process" column="name" sort={sort} onSort={onSort} numeric={false} />
          <th scope="col" className="process-table__numeric">
            PID
          </th>
          <th scope="col" className="process-table__secondary">
            State
          </th>
          <SortHeader label="CPU" column="cpu" sort={sort} onSort={onSort} />
          <SortHeader label="Memory" column="memory" sort={sort} onSort={onSort} />
          <th scope="col" className="process-table__numeric process-table__secondary">
            Threads
          </th>
          <SortHeader label="Read" column="read" sort={sort} onSort={onSort} />
          <SortHeader label="Write" column="write" sort={sort} onSort={onSort} />
        </tr>
      </thead>
      <tbody>
        {processes.map((process) => (
          <tr key={process.instanceId}>
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
            <td className="process-table__numeric process-table__secondary">
              {process.threadCount.value === null ? (
                <span
                  className="value--unavailable"
                  title={describeAvailability(process.threadCount.availability)}
                >
                  —
                </span>
              ) : (
                formatCount(process.threadCount.value)
              )}
            </td>
            <Cell field={process.readBytesPerSecond} format={formatThroughput} />
            <Cell field={process.writeBytesPerSecond} format={formatThroughput} />
          </tr>
        ))}
      </tbody>
    </table>
  );
}
