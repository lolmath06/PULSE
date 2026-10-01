import { activeLocale } from '@/i18n/i18n';

/**
 * Number, date and time formatting in the **active PULSE language**.
 *
 * Never the operating system's locale when the user picked another language
 * in PULSE, and never a hard-coded `en-US`: a French interface writes
 * `12,5 MiB/s` and a German one `12,5`, while the value itself — always in
 * its canonical unit — is untouched. Units stay the international symbols
 * (`MiB`, `°C`, `GHz`): they are part of the measurement, not prose.
 *
 * Formatters are cached per locale and options: `Intl` objects are costly to
 * build and these run for every value on every frame.
 */

const numberCache = new Map<string, Intl.NumberFormat>();
const dateCache = new Map<string, Intl.DateTimeFormat>();
const relativeCache = new Map<string, Intl.RelativeTimeFormat>();

function cached<T>(cache: Map<string, T>, key: string, make: () => T): T {
  let hit = cache.get(key);
  if (!hit) {
    hit = make();
    cache.set(key, hit);
  }
  return hit;
}

export function numberFormat(
  options: Intl.NumberFormatOptions = {},
  locale: string = activeLocale(),
): Intl.NumberFormat {
  return cached(
    numberCache,
    `${locale}|${JSON.stringify(options)}`,
    () => new Intl.NumberFormat(locale, options),
  );
}

/** A number in the active locale, with `Intl.NumberFormat` options. */
export function formatNumber(
  value: number,
  options: Intl.NumberFormatOptions = {},
  locale: string = activeLocale(),
): string {
  return numberFormat(options, locale).format(value);
}

/**
 * Exactly `decimals` fraction digits — the locale's `toFixed`. Grouping is
 * off unless asked for, so a reading keeps the compact shape it had.
 */
export function formatFixed(
  value: number,
  decimals: number,
  options: { readonly grouping?: boolean; readonly locale?: string } = {},
): string {
  const digits = Math.max(0, Math.min(20, Math.trunc(decimals)));
  return formatNumber(
    // `+ 0` turns a negative zero into zero, as `toFixed` always did.
    value + 0,
    {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
      useGrouping: options.grouping ?? false,
    },
    options.locale,
  );
}

/** A whole number with the locale's grouping: `12 345`, `12.345`, `12,345`. */
export function formatInteger(value: number, locale: string = activeLocale()): string {
  return formatNumber(Math.round(value), { maximumFractionDigits: 0 }, locale);
}

export function dateTimeFormat(
  options: Intl.DateTimeFormatOptions,
  locale: string = activeLocale(),
): Intl.DateTimeFormat {
  return cached(
    dateCache,
    `${locale}|${JSON.stringify(options)}`,
    () => new Intl.DateTimeFormat(locale, options),
  );
}

/** A date and/or time in the active locale. History stores UTC; this shows local time. */
export function formatDateTime(
  value: number | Date,
  options: Intl.DateTimeFormatOptions = { dateStyle: 'medium', timeStyle: 'medium' },
  locale: string = activeLocale(),
): string {
  return dateTimeFormat(options, locale).format(value);
}

/** `in 5 minutes`, `il y a 3 heures`, `3 小时前`. */
export function formatRelativeTime(
  value: number,
  unit: Intl.RelativeTimeFormatUnit,
  locale: string = activeLocale(),
): string {
  return cached(
    relativeCache,
    locale,
    () => new Intl.RelativeTimeFormat(locale, { numeric: 'auto' }),
  ).format(value, unit);
}
