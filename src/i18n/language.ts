import { readSectionNormalized, subscribeUiConfig } from '@/config/uiConfig';
import { normalizeSettings, setLanguagePreference, useDesktopSettings } from '@/overlay/settings';
import type { LanguagePreference, LocaleCode } from '@/i18n/locales';
import { resolvePreference, systemLanguages } from '@/i18n/locales';
import { activeLocale, initI18n, setActiveLocale } from '@/i18n/i18n';

/**
 * The language preference, stored in the shared `settings` section.
 *
 * Every window loads the shared configuration before its first render, so
 * each one starts in the saved language; a change made in any window reaches
 * the others as an ordinary `ui-config-changed` event and is applied at once,
 * without a restart.
 */

export function readLanguagePreference(): LanguagePreference {
  return readSectionNormalized('settings', normalizeSettings).language;
}

export function useLanguagePreference(): LanguagePreference {
  return useDesktopSettings().language;
}

/** Saves the preference; the language changes in every window. */
export function chooseLanguage(preference: LanguagePreference) {
  setLanguagePreference(preference);
  syncLanguage();
}

let systemSource: () => readonly string[] = () => systemLanguages();

/** Re-resolves the preference and applies it if the result changed. */
export function syncLanguage(): LocaleCode {
  const locale = resolvePreference(readLanguagePreference(), systemSource());
  if (locale !== activeLocale()) setActiveLocale(locale);
  return locale;
}

let stop: (() => void) | null = null;

/**
 * Initialises i18n in the saved language and keeps it in sync: with the
 * configuration (this window's writes and every other window's) and, in
 * System mode, with the environment's own language changes.
 *
 * Call once per window, after `initUiConfig` and before the first render.
 */
export function startLanguageSync(options: { system?: () => readonly string[] } = {}): LocaleCode {
  stop?.();
  systemSource = options.system ?? (() => systemLanguages());
  const locale = resolvePreference(readLanguagePreference(), systemSource());
  initI18n(locale);
  setActiveLocale(locale);

  const unsubscribe = subscribeUiConfig(() => {
    syncLanguage();
  });
  const onSystemChange = () => {
    syncLanguage();
  };
  globalThis.addEventListener?.('languagechange', onSystemChange);
  stop = () => {
    unsubscribe();
    globalThis.removeEventListener?.('languagechange', onSystemChange);
    stop = null;
  };
  return locale;
}

/** Stops syncing. Tests only. */
export function stopLanguageSyncForTesting() {
  stop?.();
}
