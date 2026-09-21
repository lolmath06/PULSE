import { useState } from 'react';
import type { MetricSample } from '@/types/metrics';
import {
  CPU_COUNT_LOGICAL,
  CPU_COUNT_PACKAGE,
  CPU_COUNT_PHYSICAL,
  CPU_FREQUENCY_CURRENT_KEY,
  CPU_FREQUENCY_MAX_KEY,
  CPU_TEMPERATURE_PACKAGE_KEY,
  CPU_USAGE_LOGICAL_KEY,
} from '@/types/wellknown';
import { useCpuDetails } from '@/hooks/useCpuDetails';
import type { LogicalProcessor } from '@/utils/cpu';
import { numberOf, packageMetric, processorMetric, sampleOf } from '@/utils/cpu';
import { describeAvailability } from '@/utils/metrics';
import { formatCelsius, formatHertz, formatPercent, formatSampleTime } from '@/utils/units';

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
 *
 * # Every processor is visible, and the card never scrolls inside itself
 *
 * Up to [`PROCESSOR_DISPLAY_LIMIT`] processors are all rendered at once and the
 * card grows to fit them. A nested scrollbar was worse than the problem it
 * solved: on a 32-thread machine it hid exactly four rows behind a second
 * scroll context inside a page that already scrolls.
 *
 * Beyond the limit the list is collapsed to the first `PROCESSOR_DISPLAY_LIMIT`
 * rows with an explicit *Show all N processors* control — still no clipping and
 * still no inner scrollbar, just a deliberate choice the user makes.
 */

/**
 * How many logical processors are rendered before the list is collapsed.
 *
 * 64 covers every ordinary desktop and workstation — including this project's
 * 32-thread reference machine — so the common case never sees the control at
 * all. Above it, a 256-thread server would otherwise add a thousand rows to the
 * page on first paint.
 */
export const PROCESSOR_DISPLAY_LIMIT = 64;

export function CpuDetailsCard() {
  const { status, processors, packages, samples, message, refreshing, refresh } = useCpuDetails();
  const [showAllProcessors, setShowAllProcessors] = useState(false);

  const count = (metric: typeof CPU_COUNT_LOGICAL) => numberOf(samples, metric);

  const logical = count(CPU_COUNT_LOGICAL);
  const physical = count(CPU_COUNT_PHYSICAL);
  const packageCount = count(CPU_COUNT_PACKAGE);

  const timestamp = samples.values().next().value?.timestamp;

  const collapsed = processors.length > PROCESSOR_DISPLAY_LIMIT && !showAllProcessors;
  const visibleProcessors = collapsed ? processors.slice(0, PROCESSOR_DISPLAY_LIMIT) : processors;

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
                <CountValue value={packageCount} sample={sampleOf(samples, CPU_COUNT_PACKAGE)} />
              </dd>
            </div>

            {packages.length === 1 ? (
              <div className="kv__row">
                <dt>Package temperature</dt>
                <dd>
                  <PackageTemperature index={packages[0]!.index} samples={samples} />
                </dd>
              </div>
            ) : (
              packages.map((cpuPackage) => (
                <div className="kv__row" key={cpuPackage.sourceId}>
                  <dt>{`${cpuPackage.label} temperature`}</dt>
                  <dd>
                    <PackageTemperature index={cpuPackage.index} samples={samples} />
                  </dd>
                </div>
              ))
            )}
          </dl>

          {processors.length > 0 ? (
            <>
              <ul className="cpu-grid" aria-label="Logical processors">
                {visibleProcessors.map((processor) => (
                  <ProcessorRow key={processor.ordinal} processor={processor} samples={samples} />
                ))}
              </ul>

              {collapsed && (
                <button
                  type="button"
                  className="button cpu-grid__expand"
                  onClick={() => setShowAllProcessors(true)}
                >
                  {`Show all ${processors.length} processors`}
                </button>
              )}
            </>
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
            processor. Frequencies are what the operating system reports. The package temperature is
            the processor&apos;s own sensor, never an average of its cores.
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

/**
 * One package's temperature, or the reason there is none.
 *
 * Whole degrees: a sensor is accurate to about a degree, and a decimal that
 * changes on every refresh shows precision the hardware does not have.
 */
function PackageTemperature({
  index,
  samples,
}: {
  readonly index: number;
  readonly samples: ReadonlyMap<string, MetricSample>;
}) {
  const metric = packageMetric(index, CPU_TEMPERATURE_PACKAGE_KEY);
  const celsius = numberOf(samples, metric);

  if (celsius === null) {
    return <Unavailable sample={sampleOf(samples, metric)} short="—" />;
  }

  return <>{formatCelsius(celsius)}</>;
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
