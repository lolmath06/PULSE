import { useMemo, useState } from 'react';
import { useMetricCatalog } from '@/hooks/useMetricCatalog';
import { disambiguateStorageLabels, discoverStorageDevices } from '@/utils/storage';
import { STORAGE_META, THROUGHPUT_DEFAULTS } from '@/components/History/chartDefaults';
import { HistoryPanel } from '@/components/History/HistoryPanel';
import { SourceSelect } from '@/components/History/SourceSelect';
import { storageSeries } from '@/components/History/series';

/** Read and write throughput of one storage device, device by device. */
export function StorageHistory() {
  const { catalog, status } = useMetricCatalog();
  const devices = useMemo(() => discoverStorageDevices(catalog), [catalog]);
  const labels = useMemo(() => disambiguateStorageLabels(devices), [devices]);
  const [selected, setSelected] = useState<string | null>(null);

  const device = devices.find((entry) => entry.sourceId === selected) ?? devices[0];
  const series = useMemo(() => (device ? storageSeries(device.sourceId) : []), [device]);

  return (
    <HistoryPanel
      chartId="storage"
      title="Storage history"
      meta={STORAGE_META}
      series={series}
      defaults={THROUGHPUT_DEFAULTS}
      replacement={
        status === 'ready' && !device ? (
          <p className="card__muted">No storage device was detected.</p>
        ) : undefined
      }
      controls={
        <SourceSelect
          label="Storage device"
          value={device?.sourceId ?? ''}
          onChange={setSelected}
          options={devices.map((entry, index) => ({
            value: entry.sourceId,
            label: labels[index] ?? entry.label,
          }))}
        />
      }
    />
  );
}
