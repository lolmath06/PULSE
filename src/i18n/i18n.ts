import i18next from 'i18next';
import type { i18n as I18n, TFunction } from 'i18next';
import { initReactI18next } from 'react-i18next';
import type { LocaleCode } from '@/i18n/locales';
import { FALLBACK_LOCALE, LOCALE_CODES, directionOf } from '@/i18n/locales';
import { RESOURCES } from '@/i18n/resources';

/**
 * The one i18next instance every window uses.
 *
 * All translations are bundled with the app and initialised synchronously,
 * so the first render is already in the right language — no flash of
 * English, no network request, ever.
 */
export const i18n: I18n = i18next.createInstance();

let initialised = false;

/** Initialises (once) and switches to `locale`. Synchronous. */
export function initI18n(locale: LocaleCode = FALLBACK_LOCALE): I18n {
  if (!initialised) {
    initialised = true;
    void i18n.use(initReactI18next).init({
      resources: RESOURCES,
      lng: locale,
      fallbackLng: FALLBACK_LOCALE,
      supportedLngs: [...LOCALE_CODES],
      // `pt-BR` and `zh-TW` are complete locales, not regional overlays.
      load: 'currentOnly',
      nonExplicitSupportedLngs: false,
      initAsync: false,
      returnNull: false,
      returnEmptyString: false,
      // React already escapes; double escaping would show `&amp;`.
      interpolation: { escapeValue: false },
      react: { useSuspense: false },
    });
  } else if (i18n.language !== locale) {
    void i18n.changeLanguage(locale);
  }
  applyDocumentLocale(locale);
  return i18n;
}

/** Changes the active language at runtime. Every subscribed view re-renders. */
export function setActiveLocale(locale: LocaleCode) {
  if (!initialised) {
    initI18n(locale);
    return;
  }
  if (i18n.language !== locale) void i18n.changeLanguage(locale);
  applyDocumentLocale(locale);
}

/** The resolved language in force — never the `system` sentinel. */
export function activeLocale(): LocaleCode {
  return (i18n.language as LocaleCode | undefined) ?? FALLBACK_LOCALE;
}

/** `<html lang>` and `<html dir>` follow the resolved language. */
export function applyDocumentLocale(
  locale: LocaleCode,
  root: HTMLElement | undefined = globalThis.document?.documentElement,
) {
  if (!root) return;
  root.lang = locale;
  root.dir = directionOf(locale);
}

/** `t` outside React — for code that genuinely runs outside a component. */
export const t: TFunction = ((...args: Parameters<TFunction>) =>
  (i18n.t as (...a: Parameters<TFunction>) => string)(...args)) as TFunction;

/** Makes sure the instance can answer, even before a window initialised it. */
function ensureInitialised() {
  if (!initialised) initI18n(FALLBACK_LOCALE);
}

/** English text for a key, whatever the active language — for persisted fallbacks. */
export function englishText(key: string, options?: Record<string, unknown>): string {
  ensureInitialised();
  return i18n.getFixedT(FALLBACK_LOCALE)(key, options ?? {});
}

/** Whether `key` is a string in English (the reference locale). */
export function hasKey(key: string): boolean {
  ensureInitialised();
  return typeof i18n.getResource(FALLBACK_LOCALE, 'translation', key) === 'string';
}
