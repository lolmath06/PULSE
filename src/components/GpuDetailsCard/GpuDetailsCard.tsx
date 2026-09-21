import type { MetricSample } from '@/types/metrics';
import {
  GPU_COUNT,
  GPU_FREQUENCY_CORE_KEY,
  GPU_FREQUENCY_MEMORY_KEY,
  GPU_MEMORY_TOTAL_KEY,
  GPU_MEMORY_USAGE_PERCENT_KEY,
  GPU_MEMORY_USED_KEY,
  GPU_USAGE_CORE_KEY,
  metricRefId,
} from '@/types/wellknown';
import { useGpuDetails } from '@/hooks/useGpuDetails';
import type { GpuDevice } from '@/utils/gpu';
import { disambiguateLabels, gpuMetric, gpuTelemetryState } from '@/utils/gpu';
import { describeAvailability } from '@/utils/metrics';
import {
  formatBytes,
  formatBytesScaledTo,
  formatHertz,
  formatPercent,
  formatSampleTime,
} from '@/utils/units';

/**
 * Real GPU inventory and telemetry, sampled on demand.
 *
 * Every number comes from the backend. Nothing is fabricated: a GPU whose
 * driver exposes no counters is shown, named and identified, with `—` and a
 * reason on each metric it cannot provide — never `0 %`, `0 GiB` or `0 GHz`,
 * which a user would read as measurements.
 *
 * The device list is **discovered from the catalog**, so this component
 * contains no list of GPUs and works unchanged from zero devices to four.
 */
export function GpuDetailsCard() {
  const { status, devices, samples, message, refreshing, refresh } = useGpuDetails();

  const countSample = samples.get(metricRefId(GPU_COUNT));
  const count = countSample?.value?.type === 'number' ? countSample.value.value : devices.length;

  const labels = disambiguateLabels(devices);
  const timestamp = samples.values().next().value?.timestamp;

  return (
    <div className="card" aria-label="GPU details">
      <h2 className="card__title">GPU details</h2>

      {status === 'loading' && <p className="card__muted">Discovering graphics adapters…</p>}

      {status === 'error' && (
        <p className="card__muted" title={message}>
          Backend unavailable. Run PULSE with <code>pnpm app:dev</code> to reach the Rust layer.
        </p>
      )}

      {status === 'ready' && (
        <>
          <dl className="kv">
            <div className="kv__row">
              <dt>Graphics adapters</dt>
              <dd>{count}</dd>
            </div>
          </dl>

          {devices.length === 0 ? (
            <p className="card__note">
              No hardware graphics adapter was detected. Software renderers are deliberately not
              counted as GPUs.
            </p>
          ) : (
            <ul className="gpu-list" aria-label="Graphics adapters">
              {devices.map((device, index) => (
                <GpuEntry
                  key={device.sourceId}
                  device={device}
                  label={labels[index] ?? device.label}
                  samples={samples}
                />
              ))}
            </ul>
          )}

          <div className="card__footer">
            <span className="card__muted">
              {typeof timestamp === 'number' ? `Updated ${formatSampleTime(timestamp)}` : ''}
            </span>
            <button
              type="button"
              className="button"
              onClick={refresh}
              disabled={refreshing}
              aria-label="Refresh GPU details"
            >
              {refreshing ? 'Refreshing…' : 'Refresh'}
            </button>
          </div>

          <p className="card__note">
            VRAM figures describe each adapter&apos;s dedicated video memory. System memory shared
            with an integrated GPU is not reported as VRAM.
          </p>
        </>
      )}
    </div>
  );
}

/** One adapter: its name, then its six telemetry rows. */
function GpuEntry({
  device,
  label,
  samples,
}: {
  readonly device: GpuDevice;
  readonly label: string;
  readonly samples: ReadonlyMap<string, MetricSample>;
}) {
  const sampleOf = (key: string): MetricSample | undefined =>
    samples.get(metricRefId(gpuMetric(device.sourceId, key)));

  const numberOf = (key: string): number | null => {
    const sample = sampleOf(key);
    if (!sample || sample.value === null || sample.value.type !== 'number') return null;
    return sample.value.value;
  };

  const usage = numberOf(GPU_USAGE_CORE_KEY);
  const total = numberOf(GPU_MEMORY_TOTAL_KEY);
  const used = numberOf(GPU_MEMORY_USED_KEY);
  const memoryPercent = numberOf(GPU_MEMORY_USAGE_PERCENT_KEY);

  const telemetry = gpuTelemetryState(samples, device.sourceId);

  return (
    <li className="gpu-list__item">
      <h3 className="gpu-list__name" title={device.sourceId}>
        {label}
      </h3>

      {!telemetry.performanceAvailable && (
        <PerformanceUnavailableNotice
          reason={telemetry.reason}
          thermalAvailable={telemetry.thermalAvailable}
        />
      )}

      <dl className="kv kv--compact">
        <Row label="GPU usage">
          {usage !== null ? (
            formatPercent(usage)
          ) : (
            <Unavailable sample={sampleOf(GPU_USAGE_CORE_KEY)} />
          )}
        </Row>

        <Row label="VRAM">
          {used !== null && total !== null ? (
            `${formatBytesScaledTo(used, total)} / ${formatBytes(total)}`
          ) : total !== null ? (
            // A capacity with no live usage figure: show the capacity rather
            // than nothing, and explain the missing half.
            <>
              <Unavailable sample={sampleOf(GPU_MEMORY_USED_KEY)} />
              {` / ${formatBytes(total)}`}
            </>
          ) : (
            <Unavailable sample={sampleOf(GPU_MEMORY_TOTAL_KEY)} />
          )}
        </Row>

        <Row label="VRAM usage">
          {memoryPercent !== null ? (
            formatPercent(memoryPercent)
          ) : (
            <Unavailable sample={sampleOf(GPU_MEMORY_USAGE_PERCENT_KEY)} />
          )}
        </Row>

        <Row label="Core clock">
          <Frequency sample={sampleOf(GPU_FREQUENCY_CORE_KEY)} />
        </Row>

        <Row label="Memory clock">
          <Frequency sample={sampleOf(GPU_FREQUENCY_MEMORY_KEY)} />
        </Row>
      </dl>

      {total !== null && used === null && (
        <p className="gpu-list__note">
          Installed VRAM is known; live usage needs a vendor driver this adapter does not provide.
        </p>
      )}
    </li>
  );
}

/**
 * States, in the card itself, that this adapter reports no performance figures.
 *
 * Without it every row reads `—` and the only explanation lives in a tooltip,
 * which a user has no reason to go looking for: the honest conclusion they draw
 * is that PULSE is broken. The wording is deliberately **"performance
 * telemetry"**, never "GPU unavailable" — the adapter is detected, named and
 * identified, and its thermal sensors may well be working, which the notice
 * says when they are.
 */
function PerformanceUnavailableNotice({
  reason,
  thermalAvailable,
}: {
  readonly reason: string | null;
  readonly thermalAvailable: boolean;
}) {
  return (
    <p className="gpu-list__notice" role="note">
      <strong>Performance telemetry unavailable</strong>
      {reason ??
        'This adapter\u2019s driver exposes no utilisation, video memory or clock figures.'}
      {thermalAvailable && ' Thermal sensors remain available.'}
    </p>
  );
}

function Row({ label, children }: { readonly label: string; readonly children: React.ReactNode }) {
  return (
    <div className="kv__row">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

/** A clock in hertz, rendered in GHz/MHz — or the reason it is absent. */
function Frequency({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return <>{formatHertz(sample.value.value)}</>;
  }

  return <Unavailable sample={sample} />;
}

/**
 * Explains an absent value instead of showing a zero.
 *
 * The distinction the backend took care to make — an unsupported sensor, a
 * missing driver, a permission problem, a transient hiccup — survives into the
 * tooltip, while the cell itself stays a plain dash.
 */
function Unavailable({ sample }: { readonly sample: MetricSample | undefined }) {
  const reason = sample ? describeAvailability(sample.availability) : 'Not reported by the backend';

  return (
    <span className="value--unavailable" title={reason}>
      —
    </span>
  );
}
