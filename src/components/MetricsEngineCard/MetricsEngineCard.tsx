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
  const state = useMetricsEngineStatus();

  return (
    <div className="card" aria-label="Metrics engine">
      <h2 className="card__title">Metrics engine</h2>

      {state.status === 'loading' && <p className="card__muted">Querying engine…</p>}

      {state.status === 'error' && (
        <p className="card__muted" title={state.message}>
          Engine unavailable. Run PULSE with <code>pnpm app:dev</code> to reach the Rust layer.
        </p>
      )}

      {state.status === 'ready' && (
        <>
          <dl className="kv">
            <div className="kv__row">
              <dt>Status</dt>
              <dd>{formatEngineState(state.engine.state)}</dd>
            </div>
            <div className="kv__row">
              <dt>Schema</dt>
              <dd>v{state.engine.schemaVersion}</dd>
            </div>
            <div className="kv__row">
              <dt>Providers</dt>
              <dd>{state.engine.providerCount}</dd>
            </div>
            <div className="kv__row">
              <dt>Metrics</dt>
              <dd>
                {state.engine.metricCount}
                {state.engine.metricCount > 0 &&
                  ` (${state.engine.availableMetricCount} available)`}
              </dd>
            </div>
          </dl>

          {!isSchemaCompatible(state.engine) && (
            <p className="card__warning">
              Backend speaks metrics schema v{state.engine.schemaVersion}; this interface expects v
              {METRICS_SCHEMA_VERSION}.
            </p>
          )}

          {state.engine.state === 'empty' && (
            <p className="card__note">
              No providers registered yet — PULSE collects no hardware metrics in this phase.
            </p>
          )}
        </>
      )}
    </div>
  );
}
