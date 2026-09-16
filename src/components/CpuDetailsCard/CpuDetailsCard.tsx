import type { MetricSample } from '@/types/metrics';
import {
  CPU_COUNT_LOGICAL,
  CPU_COUNT_PACKAGE,
  CPU_COUNT_PHYSICAL,
  CPU_FREQUENCY_CURRENT_KEY,
  CPU_FREQUENCY_MAX_KEY,
  CPU_USAGE_LOGICAL_KEY,
} from '@/types/wellknown';
import { useCpuDetails } from '@/hooks/useCpuDetails';
import type { LogicalProcessor } from '@/utils/cpu';
import { numberOf, processorMetric, sampleOf } from '@/utils/cpu';
import { describeAvailability } from '@/utils/metrics';
import { formatHertz, formatPercent, formatSampleTime } from '@/utils/units';

/**
 * Real per-processor CPU detail, sampled on demand.
 *
 * Every number comes from the backend. Nothing is fabricated: a processor
 * whose frequency cannot be read shows `—` with the reason in its tooltip,
 * never `0 GHz`, and a usage that has no delta yet says so rather than
 * claiming the core is idle.
 *
 * The processor list is **discovered from the catalog**, so this component
 * contains no list of CPUs and works unchanged from one logical processor to
 * a hundred and twenty-eight.
 */
export function CpuDetailsCard() {
  const { status, processors, samples, message, refreshing, refresh } = useCpuDetails();

  const count = (metric: typeof CPU_COUNT_LOGICAL) => numberOf(samples, metric);

  const logical = count(CPU_COUNT_LOGICAL);
  const physical = count(CPU_COUNT_PHYSICAL);
  const packages = count(CPU_COUNT_PACKAGE);

  const timestamp = samples.values().next().value?.timestamp;

  return (
    <div className="card" aria-label="CPU details">
      <h2 className="card__title">CPU details</h2>

      {status === 'loading' && <p className="card__muted">Reading CPU topology…</p>}

      {status === 'error' && (
        <p className="card__muted" title={message}>
          Backend unavailable. Run PULSE with <code>pnpm app:dev</code> to reach the Rust layer.
        </p>
      )}

      {status === 'ready' && (
        <>
          <dl className="kv">
            <div className="kv__row">
              <dt>Physical cores</dt>
              <dd>
                <CountValue value={physical} sample={sampleOf(samples, CPU_COUNT_PHYSICAL)} />
              </dd>
            </div>
            <div className="kv__row">
              <dt>Logical processors</dt>
              <dd>
                <CountValue value={logical} sample={sampleOf(samples, CPU_COUNT_LOGICAL)} />
              </dd>
            </div>
            <div className="kv__row">
              <dt>Packages</dt>
              <dd>
                <CountValue value={packages} sample={sampleOf(samples, CPU_COUNT_PACKAGE)} />
              </dd>
            </div>
          </dl>

          {processors.length > 0 ? (
            <ul className="cpu-grid" aria-label="Logical processors">
              {processors.map((processor) => (
                <ProcessorRow key={processor.ordinal} processor={processor} samples={samples} />
              ))}
            </ul>
          ) : (
            <p className="card__note">
              No individual logical processor is exposed on this machine.
            </p>
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
              aria-label="Refresh CPU details"
            >
              {refreshing ? 'Refreshing…' : 'Refresh'}
            </button>
          </div>

          <p className="card__note">
            One physical core can carry several logical processors, so usage is reported per logical
            processor. Frequencies are what the operating system reports.
          </p>
        </>
      )}
    </div>
  );
}

/** One compact row: label, usage, current frequency. */
function ProcessorRow({
  processor,
  samples,
}: {
  readonly processor: LogicalProcessor;
  readonly samples: ReadonlyMap<string, MetricSample>;
}) {
  const usageMetric = processorMetric(processor.ordinal, CPU_USAGE_LOGICAL_KEY);
  const currentMetric = processorMetric(processor.ordinal, CPU_FREQUENCY_CURRENT_KEY);
  const maxMetric = processorMetric(processor.ordinal, CPU_FREQUENCY_MAX_KEY);

  const usage = numberOf(samples, usageMetric);
  const current = numberOf(samples, currentMetric);
  const max = numberOf(samples, maxMetric);

  // The bar is a share of this processor's own maximum-scale gauge, not of the
  // machine, so a single pinned core reads as full.
  const width = usage === null ? 0 : Math.max(0, Math.min(100, usage));

  return (
    <li className="cpu-grid__row">
      <span className="cpu-grid__name">{processor.label}</span>

      <span className="cpu-grid__meter" aria-hidden="true">
        <span className="cpu-grid__meter-fill" style={{ width: `${width}%` }} />
      </span>

      <span className="cpu-grid__usage">
        {usage !== null ? (
          formatPercent(usage)
        ) : (
          <Unavailable sample={sampleOf(samples, usageMetric)} short="—" />
        )}
      </span>

      <span
        className="cpu-grid__frequency"
        title={
          max !== null
            ? `Maximum reported for ${processor.label}: ${formatHertz(max)}`
            : describeAvailability(
                sampleOf(samples, maxMetric)?.availability ?? {
                  status: 'notRegistered',
                  reason: 'no maximum frequency is published for this processor',
                },
              )
        }
      >
        {current !== null ? (
          formatHertz(current)
        ) : (
          <Unavailable sample={sampleOf(samples, currentMetric)} short="—" />
        )}
      </span>
    </li>
  );
}

/** Renders a topology count, or the reason it is missing. */
function CountValue({
  value,
  sample,
}: {
  readonly value: number | null;
  readonly sample: MetricSample | undefined;
}) {
  if (value !== null) return <>{value}</>;

  return <Unavailable sample={sample} short="Not reported" />;
}

/**
 * Explains an absent value instead of showing a zero.
 *
 * The distinction the backend took care to make — waiting for a delta, an
 * unsupported sensor, a permission problem — survives into the tooltip, while
 * the cell itself stays narrow enough for a 128-row grid.
 */
function Unavailable({
  sample,
  short,
}: {
  readonly sample: MetricSample | undefined;
  readonly short: string;
}) {
  const reason = sample ? describeAvailability(sample.availability) : 'Not reported by the backend';

  return (
    <span className="value--unavailable" title={reason}>
      {short}
    </span>
  );
}
