import type { MetricUnit } from '@/types/metrics';

/**
 * Value and time formatting for visualizations.
 *
 * Every renderer — the big number, the axis, the tooltip, the bar label —
 * goes through here, so one metric reads the same in every representation.
 * Values arrive in canonical units (bytes, hertz, °C, percent) and are only
 * scaled at this edge.
 */

export interface FormattedValue {
  /** The number, e.g. `27.4`. */
  readonly value: string;
  /** Its unit, e.g. `%` or `MiB/s`. Empty for unitless counts. */
  readonly unit: string;
}

const PLACEHOLDER: FormattedValue = { value: '—', unit: '' };

/** The precision each unit reads best at. */
export const DEFAULT_DECIMALS: Readonly<Record<MetricUnit, number>> = {
  percent: 1,
  ratio: 2,
  celsius: 0,
  hertz: 2,
  bytes: 1,
  bytesPerSecond: 1,
  operationsPerSecond: 0,
  packetsPerSecond: 0,
  bitsPerSecond: 1,
  watts: 1,
  volts: 2,
  rpm: 0,
  milliseconds: 1,
  seconds: 0,
  decibelMilliwatts: 0,
  hours: 0,
  count: 0,
  none: 1,
};

const SIMPLE_UNITS: Partial<Record<MetricUnit, string>> = {
  percent: '%',
  celsius: '°C',
  operationsPerSecond: 'op/s',
  packetsPerSecond: 'pkt/s',
  watts: 'W',
  volts: 'V',
  rpm: 'RPM',
  milliseconds: 'ms',
  seconds: 's',
  decibelMilliwatts: 'dBm',
  hours: 'h',
};

const BINARY = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'];
const DECIMAL_BITS = ['bit/s', 'kbit/s', 'Mbit/s', 'Gbit/s', 'Tbit/s'];

function grouped(value: number, decimals: number): string {
  return value.toLocaleString('en-US', {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  });
}

/**
 * Splits a value into its number and unit, at `decimals` (or the unit's
 * default). Non-finite values give a placeholder, never `NaN`.
 */
export function formatParts(
  value: number,
  unit: MetricUnit,
  decimals?: number | null,
): FormattedValue {
  if (!Number.isFinite(value)) return PLACEHOLDER;
  const digits = decimals ?? DEFAULT_DECIMALS[unit];

  switch (unit) {
    case 'bytes':
    case 'bytesPerSecond': {
      const suffix = unit === 'bytesPerSecond' ? '/s' : '';
      let scaled = Math.abs(value);
      let index = 0;
      while (scaled >= 1024 && index < BINARY.length - 1) {
        scaled /= 1024;
        index += 1;
      }
      const sign = value < 0 ? '-' : '';
      return {
        value: sign + (index === 0 ? String(Math.round(scaled)) : scaled.toFixed(digits)),
        unit: `${BINARY[index]}${suffix}`,
      };
    }
    case 'bitsPerSecond': {
      let scaled = Math.abs(value);
      let index = 0;
      while (scaled >= 1000 && index < DECIMAL_BITS.length - 1) {
        scaled /= 1000;
        index += 1;
      }
      return {
        value:
          (value < 0 ? '-' : '') +
          (index === 0 ? String(Math.round(scaled)) : scaled.toFixed(digits)),
        unit: DECIMAL_BITS[index] ?? 'bit/s',
      };
    }
    case 'hertz':
      return value >= 1e9
        ? { value: (value / 1e9).toFixed(digits), unit: 'GHz' }
        : { value: String(Math.round(value / 1e6)), unit: 'MHz' };
    case 'count':
      return { value: grouped(value, digits), unit: '' };
    case 'ratio':
    case 'none':
      return { value: value.toFixed(digits), unit: '' };
    default:
      return { value: value.toFixed(digits), unit: SIMPLE_UNITS[unit] ?? '' };
  }
}

/** `27.4 %`, `12.8 MiB/s`, or `27.4` when the unit is hidden. */
export function formatValue(
  value: number,
  unit: MetricUnit,
  decimals?: number | null,
  showUnit = true,
): string {
  const parts = formatParts(value, unit, decimals);
  return showUnit && parts.unit ? `${parts.value} ${parts.unit}` : parts.value;
}

/**
 * An axis label: the same formatting, one digit less, because an axis
 * labels a scale rather than reporting a reading.
 */
export function formatAxisValue(value: number, unit: MetricUnit, span: number): string {
  const decimals =
    unit === 'bytes' || unit === 'bytesPerSecond' || unit === 'bitsPerSecond'
      ? 1
      : span < 2
        ? 2
        : span < 20
          ? 1
          : 0;
  return formatValue(value, unit, Math.min(decimals, DEFAULT_DECIMALS[unit] + 1));
}

const HOUR_MS = 3_600_000;

/**
 * A timestamp as **local** wall-clock text. History stores UTC; the time
 * zone is applied only here, at display.
 */
export function formatTime(t: number, spanMs: number): string {
  if (!Number.isFinite(t)) return '—';
  const date = new Date(t);
  if (spanMs > 36 * HOUR_MS) {
    return date.toLocaleString(undefined, { weekday: 'short', hour: '2-digit', minute: '2-digit' });
  }
  return date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}

/** A tooltip timestamp: seconds included, the date too for long windows. */
export function formatTooltipTime(t: number, spanMs: number): string {
  if (!Number.isFinite(t)) return '—';
  const date = new Date(t);
  const time = date.toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
  if (spanMs > 24 * HOUR_MS) {
    return `${date.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' })} ${time}`;
  }
  return time;
}
