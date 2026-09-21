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

const DECIMAL_HOUR_GROUPING = 1000;

/**
 * Formats a throughput given in bytes per second, e.g. `125 MiB/s`.
 *
 * Binary prefixes, consistently with {@link formatBytes}: a monitor that
 * showed a 2 TiB disk transferring at 125 MB/s would be mixing two
 * conventions in one card.
 *
 * **Zero is a real measurement here**, unlike a frequency: a disk with no I/O
 * during the interval genuinely transferred `0 B/s`, and the user wants to see
 * that rather than a dash. A device with no baseline yet never reaches this
 * function — the backend reports it as unavailable and the interface shows `—`
 * with the reason. Conflating "idle" with "not measured" is what makes a
 * monitor untrustworthy.
 *
 * A negative rate is refused: bytes do not flow backwards, so the reading was
 * not a throughput.
 */
export function formatThroughput(bytesPerSecond: number): string {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond < 0) return '—';
  if (bytesPerSecond < BINARY_STEP) return `${Math.round(bytesPerSecond)} B/s`;

  let value = bytesPerSecond;
  let unitIndex = 0;

  while (value >= BINARY_STEP && unitIndex < BINARY_UNITS.length - 1) {
    value /= BINARY_STEP;
    unitIndex += 1;
  }

  // One decimal below 100, none above: `1.5 GiB/s` is worth the digit and
  // `847.3 MiB/s` is false precision on a figure that moves every refresh.
  const digits = value < 100 ? 1 : 0;
  return `${value.toFixed(digits)} ${BINARY_UNITS[unitIndex]}/s`;
}

/**
 * Formats completed operations per second, e.g. `940 IOPS`.
 *
 * **Zero is a real measurement**, for the same reason as throughput: no
 * operation completed during the interval.
 *
 * Whole operations: a disk does not complete 940.7 of them, and the fractional
 * part is an artefact of dividing by an interval that is not exactly one
 * second. Below 10 the figure is shown to one decimal, because the difference
 * between 0.4 and 2 IOPS is real and rounding both to `0` and `2` would hide
 * a trickle of activity entirely.
 */
export function formatIops(operationsPerSecond: number): string {
  if (!Number.isFinite(operationsPerSecond) || operationsPerSecond < 0) return '—';

  if (operationsPerSecond > 0 && operationsPerSecond < 10) {
    return `${operationsPerSecond.toFixed(1)} IOPS`;
  }

  return `${Math.round(operationsPerSecond).toLocaleString()} IOPS`;
}

/**
 * Formats a latency given in milliseconds, e.g. `0.73 ms`, `3.2 ms`, `18 ms`.
 *
 * Precision scales with magnitude, because the hardware's does. An NVMe read
 * answering in 0.73 ms needs two decimals to be distinguishable from 0.71; a
 * spinning disk at 18 ms does not, and `18.24 ms` would claim a resolution the
 * operating system's millisecond accounting does not have.
 *
 * **Zero is a real measurement**: operations completed, and the OS accounted
 * less than its rounding unit of time to them, which is normal on a fast SSD.
 * An interval in which *no* operation completed never reaches this function —
 * there is no mean to take, so the backend reports no value at all.
 */
export function formatLatency(milliseconds: number): string {
  if (!Number.isFinite(milliseconds) || milliseconds < 0) return '—';

  if (milliseconds < 1) return `${milliseconds.toFixed(2)} ms`;
  if (milliseconds < 10) return `${milliseconds.toFixed(1)} ms`;

  return `${Math.round(milliseconds).toLocaleString()} ms`;
}

/**
 * Formats a duration given in whole hours, e.g. `421 h`.
 *
 * Kept in hours rather than converted to days or years. An NVMe controller
 * counts power-on time in whole hours, and rendering 8 760 of them as "1 year"
 * would round away the figure a user compares against a warranty.
 *
 * Grouped above a thousand, because `12 847 h` is readable and `12847 h` is
 * not.
 */
export function formatHours(hours: number): string {
  if (!Number.isFinite(hours) || hours < 0) return '—';

  const whole = Math.round(hours);
  return whole >= DECIMAL_HOUR_GROUPING ? `${whole.toLocaleString()} h` : `${whole} h`;
}

/**
 * Formats a plain count, e.g. `0` or `1 284`.
 *
 * Used for the storage health counters — unsafe shutdowns, media errors —
 * where **zero is the answer a user hopes for** and must be shown as a
 * measurement rather than a dash.
 */
export function formatCount(count: number): string {
  if (!Number.isFinite(count) || count < 0) return '—';

  return Math.round(count).toLocaleString();
}

const DECIMAL_BIT_UNITS = ['bit/s', 'Kbit/s', 'Mbit/s', 'Gbit/s', 'Tbit/s'] as const;
const DECIMAL_STEP = 1000;

/**
 * Formats a link capacity given in bits per second, e.g. `1.0 Gbit/s`.
 *
 * # Decimal prefixes, unlike every other rate in PULSE
 *
 * Networking counts in powers of ten and always has: a "gigabit" link is
 * 1 000 000 000 bits per second, not 1 073 741 824, and every switch, driver
 * and datasheet agrees. Rendering it with binary prefixes would show a
 * perfectly ordinary gigabit link as `0.93 Gibit/s`, which matches nothing the
 * user has ever seen.
 *
 * This is the one place PULSE deliberately uses a different convention from
 * {@link formatBytes} and {@link formatThroughput}, and the reason the two
 * units are kept apart in the contract: **link capacity is bits, observed
 * traffic is bytes.** Mixing them is a factor-of-eight error that looks
 * entirely plausible on screen.
 *
 * Zero and negative values return the placeholder. **Zero is not a link
 * speed**: the platforms use it to mean "nothing negotiated", and the backend
 * already reports that as unavailable, so a zero arriving here is a fault.
 */
export function formatBitsPerSecond(bitsPerSecond: number): string {
  if (!Number.isFinite(bitsPerSecond) || bitsPerSecond <= 0) return '—';

  let value = bitsPerSecond;
  let unitIndex = 0;

  while (value >= DECIMAL_STEP && unitIndex < DECIMAL_BIT_UNITS.length - 1) {
    value /= DECIMAL_STEP;
    unitIndex += 1;
  }

  // Whole bits and kilobits; one decimal above, because the difference between
  // 866.7 Mbit/s and 867 Mbit/s is a real Wi-Fi rate distinction.
  const digits = unitIndex <= 1 ? 0 : 1;
  return `${value.toFixed(digits)} ${DECIMAL_BIT_UNITS[unitIndex]}`;
}

/**
 * Formats a packet rate, e.g. `8.4k/s` or `142/s`.
 *
 * Abbreviated above a thousand because packet rates reach six figures on a
 * busy link and `142,857/s` is harder to read at a glance than `143k/s` —
 * and the precision is not information, since the figure moves on every
 * refresh.
 *
 * **Zero is a real measurement**: no packet arrived during the interval.
 */
export function formatPacketRate(packetsPerSecond: number): string {
  if (!Number.isFinite(packetsPerSecond) || packetsPerSecond < 0) return '—';

  if (packetsPerSecond >= 1_000_000) {
    return `${(packetsPerSecond / 1_000_000).toFixed(1)}M/s`;
  }
  if (packetsPerSecond >= 1_000) {
    return `${(packetsPerSecond / 1_000).toFixed(1)}k/s`;
  }

  return `${Math.round(packetsPerSecond)}/s`;
}

/**
 * Formats a signal strength in dBm, e.g. `-68 dBm`.
 *
 * Whole decibels. A radio's reading moves by a decibel or two between
 * consecutive samples anyway, so `-68.4 dBm` shows a precision the
 * measurement does not have and jitters in its decimal on every refresh.
 *
 * **Positive values are refused.** A received Wi-Fi signal is negative — 0 dBm
 * is a milliwatt arriving at the antenna, which does not happen — so a
 * positive figure here is a misread rather than an extremely strong link.
 */
export function formatDbm(dbm: number): string {
  if (!Number.isFinite(dbm) || dbm > 0) return '—';

  return `${Math.round(dbm)} dBm`;
}
