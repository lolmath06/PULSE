import { useMemo } from 'react';
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
  const { catalog, status } = useMetricCatalog();
  const { series, unavailable } = useMemo(() => thermalSeries(catalog), [catalog]);

  return (
    <HistoryPanel
      chartId="thermal"
      title="Thermal history"
      meta={THERMAL_META}
      series={series}
      defaults={THERMAL_DEFAULTS}
      replacement={
        status === 'ready' && series.length === 0 ? (
          <p className="card__muted">No temperature sensor is readable on this machine.</p>
        ) : undefined
      }
      footnote={unavailable.length > 0 ? `Not charted — ${unavailable.join(' · ')}` : undefined}
    />
  );
}
