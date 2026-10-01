import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useMetricCatalog } from '@/hooks/useMetricCatalog';
import { THERMAL_DEFAULTS, THERMAL_META } from '@/components/History/chartDefaults';
import { HistoryPanel } from '@/components/History/HistoryPanel';
import { thermalSeries } from '@/components/History/series';

/**
 * CPU package and GPU core temperatures on one chart. The series selection —
 * and why an unavailable sensor is listed rather than drawn — lives in
 * `thermalSeries`.
 */
export function ThermalHistory() {
  const { t, i18n } = useTranslation();
  const { catalog, status } = useMetricCatalog();
  const { series, unavailable } = useMemo(
    () => thermalSeries(catalog),
    // The reasons are worded in the active language.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [catalog, i18n.language],
  );

  return (
    <HistoryPanel
      chartId="thermal"
      title={t('history.thermal.title')}
      meta={THERMAL_META}
      series={series}
      defaults={THERMAL_DEFAULTS}
      replacement={
        status === 'ready' && series.length === 0 ? (
          <p className="card__muted">{t('history.thermal.none')}</p>
        ) : undefined
      }
      footnote={
        unavailable.length > 0
          ? t('history.thermal.notCharted', { list: unavailable.join(' · ') })
          : undefined
      }
    />
  );
}
