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
