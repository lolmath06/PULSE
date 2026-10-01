/**
 * The languages PULSE speaks, and how a system locale maps onto one of them.
 *
 * Adding a language is three steps: a JSON file under `src/i18n/locales/`, an
 * entry in {@link LOCALES} (its BCP 47 code, its own name for itself, its
 * writing direction), and the import in `resources.ts`. The tests check that
 * every registered locale has a file with every key of the English one.
 *
 * Everything here is pure and deterministic: no network, no remote lookup.
 */

export type TextDirection = 'ltr' | 'rtl';

export interface LocaleInfo {
  /** BCP 47 tag, as PULSE stores it. Also the resource file's name. */
  readonly code: string;
  /** The language's name in that language — what the selector shows. */
  readonly nativeName: string;
  readonly dir: TextDirection;
}

export const LOCALES = [
  { code: 'en', nativeName: 'English', dir: 'ltr' },
  { code: 'fr', nativeName: 'Français', dir: 'ltr' },
  { code: 'es', nativeName: 'Español', dir: 'ltr' },
  { code: 'de', nativeName: 'Deutsch', dir: 'ltr' },
  { code: 'it', nativeName: 'Italiano', dir: 'ltr' },
  { code: 'pt-BR', nativeName: 'Português (Brasil)', dir: 'ltr' },
  { code: 'nl', nativeName: 'Nederlands', dir: 'ltr' },
  { code: 'pl', nativeName: 'Polski', dir: 'ltr' },
  { code: 'ru', nativeName: 'Русский', dir: 'ltr' },
  { code: 'tr', nativeName: 'Türkçe', dir: 'ltr' },
  { code: 'id', nativeName: 'Bahasa Indonesia', dir: 'ltr' },
  { code: 'hi', nativeName: 'हिन्दी', dir: 'ltr' },
  { code: 'ja', nativeName: '日本語', dir: 'ltr' },
  { code: 'ko', nativeName: '한국어', dir: 'ltr' },
  { code: 'zh-CN', nativeName: '简体中文', dir: 'ltr' },
  { code: 'zh-TW', nativeName: '繁體中文', dir: 'ltr' },
] as const satisfies readonly LocaleInfo[];

export type LocaleCode = (typeof LOCALES)[number]['code'];

/** English: the source language and the fallback for everything. */
export const FALLBACK_LOCALE: LocaleCode = 'en';

/** What the user chose: a language, or `system` to follow the environment. */
export type LanguagePreference = 'system' | LocaleCode;

export const DEFAULT_LANGUAGE_PREFERENCE: LanguagePreference = 'system';

export const LOCALE_CODES: readonly LocaleCode[] = LOCALES.map((locale) => locale.code);

export function isLocaleCode(value: unknown): value is LocaleCode {
  return typeof value === 'string' && (LOCALE_CODES as readonly string[]).includes(value);
}

export function isLanguagePreference(value: unknown): value is LanguagePreference {
  return value === 'system' || isLocaleCode(value);
}

export function localeInfo(code: LocaleCode): LocaleInfo {
  return LOCALES.find((locale) => locale.code === code) ?? LOCALES[0];
}

/** The writing direction of a resolved locale — for `<html dir>`. */
export function directionOf(code: LocaleCode): TextDirection {
  return localeInfo(code).dir;
}

interface ParsedTag {
  readonly language: string;
  readonly script: string | null;
  readonly region: string | null;
}

/**
 * Splits a BCP 47 tag into language, script and region. Accepts the `_`
 * separator POSIX locales use (`fr_CA.UTF-8`) and drops encodings and
 * modifiers. Returns `null` for anything that is not a language tag.
 */
export function parseLocaleTag(tag: string): ParsedTag | null {
  const cleaned = tag.trim().split(/[.@]/)[0]!.replace(/_/g, '-');
  if (!/^[A-Za-z]{2,8}(-[A-Za-z0-9]{1,8})*$/.test(cleaned)) return null;
  const [language, ...rest] = cleaned.split('-');
  let script: string | null = null;
  let region: string | null = null;
  for (const part of rest) {
    if (script === null && region === null && /^[A-Za-z]{4}$/.test(part)) {
      script = part.charAt(0).toUpperCase() + part.slice(1).toLowerCase();
    } else if (region === null && /^([A-Za-z]{2}|[0-9]{3})$/.test(part)) {
      region = part.toUpperCase();
    }
  }
  return { language: language!.toLowerCase(), script, region };
}

/** Regions whose written Chinese is Traditional by default. */
const TRADITIONAL_CHINESE_REGIONS = new Set(['TW', 'HK', 'MO']);

/** The supported locale one tag maps to, or `null` when PULSE has none. */
export function matchLocale(tag: string): LocaleCode | null {
  const parsed = parseLocaleTag(tag);
  if (!parsed) return null;
  const { language, script, region } = parsed;

  if (language === 'zh') {
    if (script === 'Hant') return 'zh-TW';
    if (script === 'Hans') return 'zh-CN';
    return region && TRADITIONAL_CHINESE_REGIONS.has(region) ? 'zh-TW' : 'zh-CN';
  }
  // Brazilian Portuguese is the one Portuguese PULSE ships; it reads far
  // better to a Portuguese speaker anywhere than English does.
  if (language === 'pt') return 'pt-BR';
  // `in` is the legacy ISO 639 code for Indonesian, still sent by some systems.
  if (language === 'in') return 'id';
  return isLocaleCode(language) ? language : null;
}

/**
 * The best supported locale for a list of preferred tags, in order — the
 * first tag PULSE can serve wins; English when none matches.
 */
export function resolveLocale(preferred: readonly string[]): LocaleCode {
  for (const tag of preferred) {
    const match = matchLocale(tag);
    if (match) return match;
  }
  return FALLBACK_LOCALE;
}

/**
 * The environment's preferred languages.
 *
 * In Tauri this is the webview's `navigator.languages`: WebView2 on Windows
 * takes it from the Windows display language, WebKitGTK on Linux from the
 * session's `LANGUAGE` / `LC_ALL` / `LC_MESSAGES` / `LANG`. Both are the
 * system locale the user configured, read locally.
 */
export function systemLanguages(
  source: Pick<Navigator, 'language' | 'languages'> | undefined = globalThis.navigator,
): readonly string[] {
  if (!source) return [];
  const list = Array.isArray(source.languages) ? source.languages : [];
  return list.length > 0 ? list : source.language ? [source.language] : [];
}

/** A preference made concrete: `system` resolved against the environment. */
export function resolvePreference(
  preference: LanguagePreference,
  system: readonly string[] = systemLanguages(),
): LocaleCode {
  return preference === 'system' ? resolveLocale(system) : preference;
}
