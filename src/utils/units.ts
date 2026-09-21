/**
 * Display formatting for canonical metric values.
 *
 * The backend always sends canonical units — bytes, percent, Unix epoch
 * milliseconds (see `docs/metrics/model.md`). Everything in this file is
 * **presentation only**: it never changes a value that is stored, compared or
 * sent anywhere. Converting at the edge is what keeps history and thresholds
 * comparable across machines.
 */

const BINARY_UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'] as const;
const BINARY_STEP = 1024;

/**
 * Formats a byte count using binary prefixes, e.g. `31.3 GiB`.
 *
 * Binary rather than decimal because that is what both `/proc/meminfo` and
 * Windows report, and what every other system tool shows for RAM — a machine
 * advertised as 32 GB shows as 31.3 GiB, and disagreeing with `free` or Task
 * Manager would look like a bug.
 *
 * Non-finite and negative inputs return a placeholder rather than `NaN GiB`.
 */
export function formatBytes(bytes: number, fractionDigits = 1): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '—';
  if (bytes < BINARY_STEP) return `${Math.round(bytes)} B`;

  let value = bytes;
  let unitIndex = 0;

  while (value >= BINARY_STEP && unitIndex < BINARY_UNITS.length - 1) {
    value /= BINARY_STEP;
    unitIndex += 1;
  }

  return `${value.toFixed(fractionDigits)} ${BINARY_UNITS[unitIndex]}`;
}

/**
 * Formats a byte count without its unit, for pairing like `11.8 / 31.2 GiB`.
 *
 * Scaled against `reference` so both halves of the pair use the same unit —
 * showing `12000 MiB / 31.2 GiB` would be useless.
 */
export function formatBytesScaledTo(bytes: number, reference: number): string {
  if (!Number.isFinite(bytes) || bytes < 0 || !Number.isFinite(reference) || reference < 0) {
    return '—';
  }

  let scale = 1;
  let unitIndex = 0;

  while (reference / scale >= BINARY_STEP && unitIndex < BINARY_UNITS.length - 1) {
    scale *= BINARY_STEP;
    unitIndex += 1;
  }

  const value = bytes / scale;
  return unitIndex === 0 ? `${Math.round(value)}` : value.toFixed(1);
}

/** The binary unit a byte count would be displayed in, e.g. `GiB`. */
export function binaryUnitFor(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '';

  let value = bytes;
  let unitIndex = 0;

  while (value >= BINARY_STEP && unitIndex < BINARY_UNITS.length - 1) {
    value /= BINARY_STEP;
    unitIndex += 1;
  }

  return BINARY_UNITS[unitIndex] ?? '';
}

/**
 * Formats a 0–100 percentage, e.g. `37.8 %`.
 *
 * Out-of-range and non-finite values return a placeholder: the backend clamps
 * already, so anything else here means something is wrong and showing `—` is
 * more honest than `NaN %`.
 */
export function formatPercent(percent: number, fractionDigits = 1): string {
  if (!Number.isFinite(percent) || percent < 0 || percent > 100) return '—';
  return `${percent.toFixed(fractionDigits)} %`;
}

/**
 * Formats a Unix epoch milliseconds timestamp as a local wall-clock time.
 *
 * Only the time of day: samples are always recent, so the date adds noise.
 */
export function formatSampleTime(timestampMs: number): string {
  if (!Number.isFinite(timestampMs) || timestampMs <= 0) return '—';

  return new Date(timestampMs).toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

const HZ_PER_MHZ = 1_000_000;
const HZ_PER_GHZ = 1_000_000_000;

/**
 * Formats a frequency given in hertz, e.g. `3.20 GHz` or `800 MHz`.
 *
 * **The backend always sends hertz** (see `docs/metrics/cpu-advanced.md`);
 * kHz, MHz and GHz exist only here, at the edge. That is what keeps a saved
 * threshold comparable between a 800 MHz idle core and a 5.4 GHz boosting one.
 *
 * Below 1 GHz the value is shown in megahertz with no decimals, because a CPU
 * idling at `800 MHz` reads better than `0.80 GHz`; at or above 1 GHz it is
 * shown in gigahertz with two decimals, which is the precision people quote
 * clock speeds to.
 *
 * Non-finite, negative and zero inputs return the placeholder. **Zero is not a
 * frequency**: the backend reports an unknown clock as unavailable rather than
 * as `0`, so a zero arriving here is a fault, and `—` is more honest than
 * `0 GHz` — which the user would read as a claim that the core has stopped.
 */
export function formatHertz(hertz: number): string {
  if (!Number.isFinite(hertz) || hertz <= 0) return '—';

  if (hertz < HZ_PER_GHZ) {
    const megahertz = Math.round(hertz / HZ_PER_MHZ);
    // A reading that would round to "0 MHz" is below anything a CPU runs at,
    // so it is a fault rather than a slow clock, and gets the placeholder for
    // the same reason a literal zero does.
    return megahertz > 0 ? `${megahertz} MHz` : '—';
  }

  return `${(hertz / HZ_PER_GHZ).toFixed(2)} GHz`;
}

/**
 * Formats a temperature given in degrees Celsius, e.g. `47 °C` or `47.5 °C`.
 *
 * **The backend always sends Celsius** (see `docs/metrics/thermals.md`): a
 * `hwmon` node reports millidegrees and a vendor library whole degrees, and
 * both are converted at the platform edge so nothing here has to know.
 *
 * Whole degrees by default. Sensors report to roughly ±1 °C, so `47.3 °C` shows
 * a precision the hardware does not have, and a value that jitters in its
 * decimal on every refresh reads as noise rather than information. A caller
 * that genuinely wants half-degrees can ask for one digit.
 *
 * **Zero is a real temperature here**, unlike a frequency or a byte count — a
 * machine in a cold room genuinely reports it, and so do sensors that read
 * below freezing. Only non-finite values and physically impossible ones (below
 * absolute zero) get the placeholder.
 */
export function formatCelsius(celsius: number, fractionDigits = 0): string {
  if (!Number.isFinite(celsius) || celsius < -273.15) return '—';

  return `${celsius.toFixed(fractionDigits)} °C`;
}

/**
 * Formats a fan speed in revolutions per minute, e.g. `2,187 RPM`.
 *
 * **Zero is a reading, not an absence.** A GPU below its zero-RPM threshold and
 * a quiet desktop fan genuinely turn at 0 RPM, and the user wants to see that.
 * A sensor that reported nothing at all never reaches this function: the
 * backend publishes it as unavailable and the interface shows `—` with the
 * reason. Conflating the two is what makes a monitoring tool untrustworthy.
 *
 * A negative value is refused — a fan does not turn backwards, so the reading
 * was not a fan speed.
 */
export function formatRpm(rpm: number): string {
  if (!Number.isFinite(rpm) || rpm < 0) return '—';

  return `${Math.round(rpm).toLocaleString()} RPM`;
}
