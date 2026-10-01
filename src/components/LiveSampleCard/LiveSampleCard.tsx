import type { MetricSample } from '@/types/metrics';
import { Trans, useTranslation } from 'react-i18next';
import { t } from '@/i18n/i18n';
import {
  CPU_USAGE_TOTAL,
  LIVE_SAMPLE_METRICS,
  MEMORY_AVAILABLE,
  MEMORY_TOTAL,
  MEMORY_USAGE_PERCENT,
  MEMORY_USED,
  metricRefId,
} from '@/types/wellknown';
import { useMetricSamples } from '@/hooks/useMetricSamples';
import { describeAvailability } from '@/utils/metrics';
import {
  binaryUnitFor,
  formatBytes,
  formatBytesScaledTo,
  formatPercent,
  formatSampleTime,
} from '@/utils/units';

/**
 * Real CPU and memory values, sampled on demand.
 *
 * Every number here comes from the backend. Nothing is fabricated: a metric
 * that cannot be read shows why it cannot be read, never a zero. CPU in
 * particular is measured between two samples, so the first reading after
 * startup may legitimately say it is waiting — that is honest, and one Refresh
 * resolves it.
 */
export function LiveSampleCard() {
  useTranslation();
  const { status, samples, message, refreshing, refresh } = useMetricSamples(LIVE_SAMPLE_METRICS);

  const numberOf = (id: string): number | null => {
    const sample = samples.get(id);
    if (!sample || sample.value === null || sample.value.type !== 'number') return null;
    return sample.value.value;
  };

  const sampleOf = (id: string): MetricSample | undefined => samples.get(id);

  const cpu = sampleOf(metricRefId(CPU_USAGE_TOTAL));
  const total = numberOf(metricRefId(MEMORY_TOTAL));
  const used = numberOf(metricRefId(MEMORY_USED));
  const available = numberOf(metricRefId(MEMORY_AVAILABLE));
  const usagePercent = numberOf(metricRefId(MEMORY_USAGE_PERCENT));

  const memorySample = sampleOf(metricRefId(MEMORY_TOTAL));

  // Every sample carries its own timestamp; they come from one request, so any
  // of them dates the whole card.
  const timestamp = samples.values().next().value?.timestamp;

  return (
    <div className="card" aria-label={t('cards.live.title')}>
      <h2 className="card__title">{t('cards.live.title')}</h2>

      {status === 'loading' && <p className="card__muted">{t('cards.live.loading')}</p>}

      {status === 'error' && (
        <p className="card__muted" title={message}>
          <Trans i18nKey="overview.backendUnavailable" components={{ code: <code /> }} />
        </p>
      )}

      {status === 'ready' && (
        <>
          <dl className="kv">
            <div className="kv__row">
              <dt>{t('metrics.catalog.cpu.usage.total.name')}</dt>
              <dd>{renderValue(cpu, (value) => formatPercent(value))}</dd>
            </div>

            <div className="kv__row">
              <dt>{t('presets.text.memory')}</dt>
              <dd>
                {used !== null && total !== null ? (
                  `${formatBytesScaledTo(used, total)} / ${formatBytes(total)}`
                ) : (
                  <UnavailableValue sample={memorySample} />
                )}
              </dd>
            </div>

            <div className="kv__row">
              <dt>{t('metrics.catalog.storage.volume.capacity.available.name')}</dt>
              <dd>
                {available !== null && total !== null
                  ? `${formatBytesScaledTo(available, total)} ${binaryUnitFor(total)}`
                  : renderValue(sampleOf(metricRefId(MEMORY_AVAILABLE)), formatBytes)}
              </dd>
            </div>

            <div className="kv__row">
              <dt>{t('metrics.catalog.memory.usage.percent.name')}</dt>
              <dd>
                {renderValue(sampleOf(metricRefId(MEMORY_USAGE_PERCENT)), (value) =>
                  formatPercent(value),
                )}
              </dd>
            </div>
          </dl>

          <div className="card__footer">
            <span className="card__muted">
              {typeof timestamp === 'number'
                ? t('cards.updated', { time: formatSampleTime(timestamp) })
                : ''}
            </span>
            <button
              type="button"
              className="button"
              onClick={refresh}
              disabled={refreshing}
              aria-label={t('cards.live.refresh')}
            >
              {refreshing ? t('common.refreshing') : t('common.refresh')}
            </button>
          </div>

          {usagePercent !== null && total !== null && used !== null && available !== null && (
            <p className="card__note">{t('cards.live.reconcile')}</p>
          )}
        </>
      )}
    </div>
  );
}

/** Renders a sample's number, or the reason it has none. */
function renderValue(sample: MetricSample | undefined, format: (value: number) => string) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return format(sample.value.value);
  }

  return <UnavailableValue sample={sample} />;
}

/**
 * Explains an absent value instead of showing a zero.
 *
 * The distinction the backend took care to make — waiting for a delta, needing
 * elevation, a driver hiccup — survives all the way to the screen.
 */
function UnavailableValue({ sample }: { readonly sample: MetricSample | undefined }) {
  if (!sample) {
    return <span className="value--unavailable">{t('cards.notReported')}</span>;
  }

  const label =
    sample.availability.status === 'temporarilyUnavailable'
      ? t('cards.live.waitingNext')
      : describeAvailability(sample.availability);

  return (
    <span className="value--unavailable" title={describeAvailability(sample.availability)}>
      {label}
    </span>
  );
}
