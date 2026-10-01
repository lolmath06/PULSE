import { describe, expect, it } from 'vitest';
import {
  DEFAULT_LANGUAGE_PREFERENCE,
  FALLBACK_LOCALE,
  LOCALES,
  LOCALE_CODES,
  directionOf,
  isLanguagePreference,
  matchLocale,
  parseLocaleTag,
  resolveLocale,
  resolvePreference,
  systemLanguages,
} from '@/i18n/locales';

describe('locale registry', () => {
  it('lists every PULSE language once, named in itself', () => {
    expect(LOCALES.map((locale) => [locale.code, locale.nativeName])).toEqual([
      ['en', 'English'],
      ['fr', 'Français'],
      ['es', 'Español'],
      ['de', 'Deutsch'],
      ['it', 'Italiano'],
      ['pt-BR', 'Português (Brasil)'],
      ['nl', 'Nederlands'],
      ['pl', 'Polski'],
      ['ru', 'Русский'],
      ['tr', 'Türkçe'],
      ['id', 'Bahasa Indonesia'],
      ['hi', 'हिन्दी'],
      ['ja', '日本語'],
      ['ko', '한국어'],
      ['zh-CN', '简体中文'],
      ['zh-TW', '繁體中文'],
    ]);
    expect(new Set(LOCALE_CODES).size).toBe(LOCALE_CODES.length);
  });

  it('defaults to following the system, with English as the fallback', () => {
    expect(DEFAULT_LANGUAGE_PREFERENCE).toBe('system');
    expect(FALLBACK_LOCALE).toBe('en');
  });

  it('accepts only System or a registered code as a preference', () => {
    expect(isLanguagePreference('system')).toBe(true);
    expect(isLanguagePreference('pt-BR')).toBe(true);
    expect(isLanguagePreference('pt')).toBe(false);
    expect(isLanguagePreference('fr-FR')).toBe(false);
    expect(isLanguagePreference(null)).toBe(false);
    expect(isLanguagePreference(42)).toBe(false);
  });

  it('writes every shipped language left to right', () => {
    for (const code of LOCALE_CODES) expect(directionOf(code)).toBe('ltr');
  });
});

describe('system locale resolution', () => {
  it.each([
    ['fr-FR', 'fr'],
    ['fr-CA', 'fr'],
    ['en-US', 'en'],
    ['en-GB', 'en'],
    ['es-MX', 'es'],
    ['de-CH', 'de'],
    ['pt-BR', 'pt-BR'],
    ['pt-PT', 'pt-BR'],
    ['zh-CN', 'zh-CN'],
    ['zh-SG', 'zh-CN'],
    ['zh-Hans', 'zh-CN'],
    ['zh', 'zh-CN'],
    ['zh-TW', 'zh-TW'],
    ['zh-HK', 'zh-TW'],
    ['zh-MO', 'zh-TW'],
    ['zh-Hant', 'zh-TW'],
    ['zh-Hant-CN', 'zh-TW'],
    ['ja-JP', 'ja'],
    ['ko-KR', 'ko'],
    ['ru-RU', 'ru'],
    ['pl-PL', 'pl'],
    ['nl-BE', 'nl'],
    ['tr-TR', 'tr'],
    ['hi-IN', 'hi'],
    ['id-ID', 'id'],
    ['in-ID', 'id'],
    ['it-CH', 'it'],
    ['FR_ca.UTF-8', 'fr'],
    ['de_DE@euro', 'de'],
  ])('%s resolves to %s', (tag, expected) => {
    expect(resolveLocale([tag])).toBe(expected);
  });

  it.each([['sv-SE'], ['ar-EG'], ['he'], ['xx'], [''], ['not a tag'], ['C'], ['POSIX']])(
    'unsupported %j falls back to English',
    (tag) => {
      expect(matchLocale(tag)).toBeNull();
      expect(resolveLocale([tag])).toBe('en');
    },
  );

  it('takes the first preferred language PULSE can serve', () => {
    expect(resolveLocale(['sv-SE', 'de-AT', 'fr-FR'])).toBe('de');
    expect(resolveLocale([])).toBe('en');
  });

  it('parses script and region whatever their case', () => {
    expect(parseLocaleTag('ZH-hant-tw')).toEqual({ language: 'zh', script: 'Hant', region: 'TW' });
    expect(parseLocaleTag('es-419')).toEqual({ language: 'es', script: null, region: '419' });
  });

  it('reads the environment languages, falling back to the single language', () => {
    expect(systemLanguages({ languages: ['ja-JP', 'en-US'], language: 'ja-JP' })).toEqual([
      'ja-JP',
      'en-US',
    ]);
    expect(systemLanguages({ languages: [], language: 'ko-KR' })).toEqual(['ko-KR']);
    expect(systemLanguages({ languages: [], language: '' })).toEqual([]);
  });

  it('resolves System against the environment and keeps an explicit choice', () => {
    expect(resolvePreference('system', ['fr-CA'])).toBe('fr');
    expect(resolvePreference('system', ['sv'])).toBe('en');
    expect(resolvePreference('ja', ['fr-CA'])).toBe('ja');
  });
});
