import { useCallback, useEffect, useState } from 'react';
import type { HistoryStatus } from '@/types/history';
import { getHistoryStatus, onHistorySample } from '@/services/history';
import { formatBytes, formatSampleTime } from '@/utils/units';

/**
 * The recorder itself: is history being written, where, how often, and how
 * fast. Row counts and file sizes are fetched only on request — counting rows
 * scans the tables.
 */
export function HistoryStatusCard() {
  const [status, setStatus] = useState<HistoryStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loadingDetails, setLoadingDetails] = useState(false);

  const load = useCallback((includeDatabase: boolean) => {
    return getHistoryStatus(includeDatabase)
      .then((next) => {
        setStatus((previous) =>
          includeDatabase || !previous?.database ? next : { ...next, database: previous.database },
        );
        setError(null);
      })
      .catch((reason: unknown) =>
        setError(reason instanceof Error ? reason.message : String(reason)),
      );
  }, []);

  useEffect(() => {
    void load(false);
    return onHistorySample(() => void load(false));
  }, [load]);

  const database = status?.database;

  return (
    <div className="card" aria-label="History recorder">
      <h2 className="card__title">History recorder</h2>

      {error && !status && (
        <p className="card__muted" title={error}>
          Backend unavailable. Run PULSE with <code>pnpm app:dev</code> to reach the Rust layer.
        </p>
      )}
      {!error && !status && <p className="card__muted">Querying history…</p>}

      {status && (
        <>
          {status.state === 'unavailable' ? (
            <p className="card__warning" role="status">
              {`History unavailable: ${status.reason ?? 'unknown reason'}. Live monitoring is unaffected.`}
            </p>
          ) : null}
          <dl className="kv">
            <div className="kv__row">
              <dt>State</dt>
              <dd>{status.state === 'recording' ? 'Recording' : 'Unavailable'}</dd>
            </div>
            <div className="kv__row">
              <dt>Cadence</dt>
              <dd>{`Every ${status.cadenceMs / 1000} s`}</dd>
            </div>
            <div className="kv__row">
              <dt>Historized metrics</dt>
              <dd>{status.historizedMetricCount}</dd>
            </div>
            <div className="kv__row">
              <dt>Batches this session</dt>
              <dd>{status.batchesThisSession}</dd>
            </div>
            {status.lastBatch && (
              <div className="kv__row">
                <dt>Last batch</dt>
                <dd>{`${formatSampleTime(status.lastBatch.timestampMs)} · ${status.lastBatch.rowCount} rows`}</dd>
              </div>
            )}
            {status.timings && (
              <div className="kv__row">
                <dt>Sample / write</dt>
                <dd>
                  {`${(status.timings.sampleMedianUs / 1000).toFixed(1)} ms / ${(status.timings.insertMedianUs / 1000).toFixed(2)} ms median`}
                </dd>
              </div>
            )}
            {status.databasePath && (
              <div className="kv__row">
                <dt>Database</dt>
                <dd className="mono history-status__path">{status.databasePath}</dd>
              </div>
            )}
            {database && (
              <>
                <div className="kv__row">
                  <dt>Size</dt>
                  <dd>{`${formatBytes(database.fileBytes)} + ${formatBytes(database.walBytes)} WAL`}</dd>
                </div>
                <div className="kv__row">
                  <dt>Rows</dt>
                  <dd>{`${database.rawRows.toLocaleString()} raw · ${database.aggregateRows.toLocaleString()} aggregated · ${database.seriesCount} series`}</dd>
                </div>
                <div className="kv__row">
                  <dt>Schema / journal</dt>
                  <dd>{`v${database.schemaVersion} · ${database.journalMode.toUpperCase()} · synchronous ${database.synchronous === 1 ? 'NORMAL' : database.synchronous}`}</dd>
                </div>
              </>
            )}
          </dl>
          {status.lastError && <p className="card__note">{`Last error: ${status.lastError}`}</p>}
          {status.state === 'recording' && (
            <div className="card__footer">
              <span className="card__muted">
                Written by one background scheduler; Refresh buttons never record.
              </span>
              <button
                type="button"
                className="button"
                disabled={loadingDetails}
                onClick={() => {
                  setLoadingDetails(true);
                  void load(true).finally(() => setLoadingDetails(false));
                }}
              >
                {loadingDetails ? 'Counting…' : 'Database details'}
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}
