import { useMemo, useState } from 'react';
import { GPU_USAGE_CORE_KEY } from '@/types/wellknown';
import { useMetricCatalog } from '@/hooks/useMetricCatalog';
import { disambiguateLabels, discoverGpus, gpuMetric } from '@/utils/gpu';
import { describeAvailability } from '@/utils/metrics';
import type { HistorySeriesSpec } from '@/utils/history';
import { GPU_DEFAULTS, GPU_META } from '@/components/History/chartDefaults';
import { HistoryPanel } from '@/components/History/HistoryPanel';
import { SourceSelect } from '@/components/History/SourceSelect';

/**
 * GPU usage history, one GPU at a time.
 *
 * A GPU whose usage the platform cannot read says so — *Telemetry
 * unavailable* with the backend's reason — instead of drawing an empty chart
 * that looks like an idle one.
 */
export function GpuHistory() {
  const { catalog, status } = useMetricCatalog();
  const devices = useMemo(() => discoverGpus(catalog), [catalog]);
  const labels = useMemo(() => disambiguateLabels(devices), [devices]);
  const [selected, setSelected] = useState<string | null>(null);

  const device = devices.find((entry) => entry.sourceId === selected) ?? devices[0];
  const definition = device
    ? catalog.find(
        (entry) =>
          entry.metric.key === GPU_USAGE_CORE_KEY && entry.metric.sourceId === device.sourceId,
      )
    : undefined;

  const series: HistorySeriesSpec[] = useMemo(
    () => (device ? [{ ref: gpuMetric(device.sourceId, GPU_USAGE_CORE_KEY), label: 'GPU' }] : []),
    [device],
  );

  let replacement;
  if (status === 'ready' && !device) {
    replacement = <p className="card__muted">No GPU was detected.</p>;
  } else if (definition && definition.availability.status !== 'available') {
    replacement = (
      <p className="card__muted" role="status">
        {`Telemetry unavailable — ${describeAvailability(definition.availability)}`}
      </p>
    );
  }

  return (
    <HistoryPanel
      chartId="gpu"
      title="GPU history"
      meta={GPU_META}
      series={series}
      defaults={GPU_DEFAULTS}
      replacement={replacement}
      controls={
        <SourceSelect
          label="GPU"
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
