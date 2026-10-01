import { useCallback, useEffect, useState } from 'react';
import { Trans, useTranslation } from 'react-i18next';
import { formatFixed, formatInteger } from '@/i18n/format';
import type { HistoryStatus } from '@/types/history';
import { getHistoryStatus, onHistorySample } from '@/services/history';
import { formatBytes, formatSampleTime } from '@/utils/units';

/**
 * The recorder itself: is history being written, where, how often, and how
 * fast. Row counts and file sizes are fetched only on request — counting rows
 * scans the tables.
 */
export function HistoryStatusCard() {
  const { t } = useTranslation();
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
    <div className="card" aria-label={t('history.status.title')}>
      <h2 className="card__title">{t('history.status.title')}</h2>

      {error && !status && (
        <p className="card__muted" title={error}>
          <Trans i18nKey="overview.backendUnavailable" components={{ code: <code /> }} />
        </p>
      )}
      {!error && !status && <p className="card__muted">{t('history.status.loading')}</p>}

      {status && (
        <>
          {status.state === 'unavailable' ? (
            <p className="card__warning" role="status">
              {t('history.status.unavailable', {
                reason: status.reason ?? t('history.status.unknownReason'),
              })}
            </p>
          ) : null}
          <dl className="kv">
            <div className="kv__row">
              <dt>{t('history.status.state')}</dt>
              <dd>
                {status.state === 'recording'
                  ? t('history.status.recording')
                  : t('history.status.stateUnavailable')}
              </dd>
            </div>
            <div className="kv__row">
              <dt>{t('history.status.cadence')}</dt>
              <dd>
                {t('history.status.every', {
                  value: formatNumberSeconds(status.cadenceMs),
                })}
              </dd>
            </div>
            <div className="kv__row">
              <dt>{t('history.status.historized')}</dt>
              <dd>{status.historizedMetricCount}</dd>
            </div>
            <div className="kv__row">
              <dt>{t('history.status.batches')}</dt>
              <dd>{status.batchesThisSession}</dd>
            </div>
            {status.lastBatch && (
              <div className="kv__row">
                <dt>{t('history.status.lastBatch')}</dt>
                <dd>
                  {t('history.status.lastBatchValue', {
                    time: formatSampleTime(status.lastBatch.timestampMs),
                    count: status.lastBatch.rowCount,
                  })}
                </dd>
              </div>
            )}
            {status.timings && (
              <div className="kv__row">
                <dt>{t('history.status.sampleWrite')}</dt>
                <dd>
                  {t('history.status.median', {
                    sample: formatFixed(status.timings.sampleMedianUs / 1000, 1),
                    write: formatFixed(status.timings.insertMedianUs / 1000, 2),
                  })}
                </dd>
              </div>
            )}
            {status.databasePath && (
              <div className="kv__row">
                <dt>{t('history.status.database')}</dt>
                <dd className="mono history-status__path">{status.databasePath}</dd>
              </div>
            )}
            {database && (
              <>
                <div className="kv__row">
                  <dt>{t('history.status.size')}</dt>
                  <dd>{`${formatBytes(database.fileBytes)} + ${formatBytes(database.walBytes)} WAL`}</dd>
                </div>
                <div className="kv__row">
                  <dt>{t('history.status.rows')}</dt>
                  <dd>
                    {t('history.status.rowsValue', {
                      raw: formatInteger(database.rawRows),
                      aggregated: formatInteger(database.aggregateRows),
                      series: database.seriesCount,
                    })}
                  </dd>
                </div>
                <div className="kv__row">
                  <dt>{t('history.status.schemaJournal')}</dt>
                  <dd>{`v${database.schemaVersion} · ${database.journalMode.toUpperCase()} · synchronous ${database.synchronous === 1 ? 'NORMAL' : database.synchronous}`}</dd>
                </div>
              </>
            )}
          </dl>
          {status.lastError && (
            <p className="card__note">
              {t('history.status.lastError', { error: status.lastError })}
            </p>
          )}
          {status.state === 'recording' && (
            <div className="card__footer">
              <span className="card__muted">{t('history.status.scheduler')}</span>
              <button
                type="button"
                className="button"
                disabled={loadingDetails}
                onClick={() => {
                  setLoadingDetails(true);
                  void load(true).finally(() => setLoadingDetails(false));
                }}
              >
                {loadingDetails ? t('history.status.counting') : t('history.status.details')}
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

/** A cadence in seconds, as the active locale writes the number. */
function formatNumberSeconds(ms: number): string {
  const seconds = ms / 1000;
  return formatFixed(seconds, Number.isInteger(seconds) ? 0 : 1);
}
