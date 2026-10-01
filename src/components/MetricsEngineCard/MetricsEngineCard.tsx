import { Trans, useTranslation } from 'react-i18next';
import { useMetricsEngineStatus } from '@/hooks/useMetricsEngineStatus';
import { isSchemaCompatible } from '@/services/metrics';
import { formatEngineState } from '@/utils/metrics';
import { METRICS_SCHEMA_VERSION } from '@/types/metrics';

/**
 * Architecture check for the metrics engine.
 *
 * This is **not** a monitoring widget. PULSE collects no hardware data yet, so
 * the card reports only what the engine says about itself: contract version,
 * provider count, metric count. No fabricated CPU load, no invented
 * temperature — an empty engine is shown as empty.
 */
export function MetricsEngineCard() {
  const { t } = useTranslation();
  const state = useMetricsEngineStatus();

  return (
    <div className="card" aria-label={t('cards.engine.title')}>
      <h2 className="card__title">{t('cards.engine.title')}</h2>

      {state.status === 'loading' && <p className="card__muted">{t('cards.engine.loading')}</p>}

      {state.status === 'error' && (
        <p className="card__muted" title={state.message}>
          <Trans i18nKey="cards.engine.unavailable" components={{ code: <code /> }} />
        </p>
      )}

      {state.status === 'ready' && (
        <>
          <dl className="kv">
            <div className="kv__row">
              <dt>{t('cards.engine.status')}</dt>
              <dd>{formatEngineState(state.engine.state)}</dd>
            </div>
            <div className="kv__row">
              <dt>{t('cards.engine.schema')}</dt>
              <dd>v{state.engine.schemaVersion}</dd>
            </div>
            <div className="kv__row">
              <dt>{t('cards.engine.providers')}</dt>
              <dd>{state.engine.providerCount}</dd>
            </div>
            <div className="kv__row">
              <dt>{t('cards.engine.metrics')}</dt>
              <dd>
                {state.engine.metricCount}
                {state.engine.metricCount > 0 &&
                  ` ${t('cards.engine.available', { count: state.engine.availableMetricCount })}`}
              </dd>
            </div>
          </dl>

          {!isSchemaCompatible(state.engine) && (
            <p className="card__warning">
              {t('cards.engine.schemaMismatch', {
                backend: state.engine.schemaVersion,
                expected: METRICS_SCHEMA_VERSION,
              })}
            </p>
          )}

          {state.engine.state === 'empty' && (
            <p className="card__note">{t('cards.engine.empty')}</p>
          )}
        </>
      )}
    </div>
  );
}
