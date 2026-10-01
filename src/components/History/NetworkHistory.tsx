import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { NETWORK_RECEIVE_BYTES_KEY } from '@/types/wellknown';
import { useMetricCatalog } from '@/hooks/useMetricCatalog';
import { useMetricHistory } from '@/hooks/useMetricHistory';
import {
  discoverNetworkInterfaces,
  isPrimaryKind,
  networkMetric,
  orderInterfaces,
} from '@/utils/network';
import { NETWORK_META, THROUGHPUT_DEFAULTS } from '@/components/History/chartDefaults';
import { HistoryPanel } from '@/components/History/HistoryPanel';
import { SourceSelect } from '@/components/History/SourceSelect';
import { defaultInterface, networkSeries } from '@/components/History/series';

/** Download and upload of one interface; the default is `defaultInterface`. */
export function NetworkHistory() {
  const { t } = useTranslation();
  const { catalog, status } = useMetricCatalog();
  const interfaces = useMemo(() => discoverNetworkInterfaces(catalog), [catalog]);
  const [selected, setSelected] = useState<string | null>(null);

  // Which hardware interfaces carried traffic in the last 15 minutes, read
  // once from history to choose a sensible default.
  const primaryRefs = useMemo(
    () =>
      interfaces
        .filter((entry) => isPrimaryKind(entry.kind))
        .map((entry) => networkMetric(entry.sourceId, NETWORK_RECEIVE_BYTES_KEY)),
    [interfaces],
  );
  const probe = useMetricHistory(primaryRefs, '15m', selected === null && primaryRefs.length > 0);
  const recentTraffic = useMemo(() => {
    const traffic = new Map<string, number>();
    for (const series of probe.response?.series ?? []) {
      if (series.latest) {
        const total = series.points.reduce((sum, point) => sum + point.v, 0);
        traffic.set(series.metric.sourceId, total);
      }
    }
    return traffic;
  }, [probe.response]);

  const current =
    interfaces.find((entry) => entry.sourceId === selected) ??
    defaultInterface(interfaces, recentTraffic);
  const series = useMemo(() => (current ? networkSeries(current.sourceId) : []), [current]);
  const ordered = useMemo(
    () => orderInterfaces(interfaces, (sourceId) => recentTraffic.has(sourceId)),
    [interfaces, recentTraffic],
  );

  return (
    <HistoryPanel
      chartId="network"
      title={t('history.network.title')}
      meta={NETWORK_META}
      series={series}
      defaults={THROUGHPUT_DEFAULTS}
      replacement={
        status === 'ready' && !current ? (
          <p className="card__muted">{t('history.network.none')}</p>
        ) : undefined
      }
      controls={
        <SourceSelect
          label={t('history.network.interface')}
          value={current?.sourceId ?? ''}
          onChange={setSelected}
          options={ordered.map((entry) => ({
            value: entry.sourceId,
            label: `${entry.label} · ${t(`cards.network.kinds.${entry.kind}`)}`,
          }))}
        />
      }
    />
  );
}
