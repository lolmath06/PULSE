import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  initUiConfig,
  memoryBackend,
  readSection,
  resetUiConfigForTesting,
} from '@/config/uiConfig';
import { normalizeSettings } from '@/overlay/settings';
import { activeLocale, i18n, setActiveLocale } from '@/i18n/i18n';
import {
  chooseLanguage,
  readLanguagePreference,
  startLanguageSync,
  stopLanguageSyncForTesting,
  syncLanguage,
} from '@/i18n/language';
import { LOCALE_CODES } from '@/i18n/locales';
import { formatDateTime, formatFixed, formatInteger, formatRelativeTime } from '@/i18n/format';
import { englishOf, storedText } from '@/i18n/text';
import {
  createDashboardFromTemplate,
  findTemplate,
  templateNameKey,
} from '@/presets/dashboardTemplates';
import { dashboardName, defaultSection, renameDashboard } from '@/dashboard/dashboards';
import { metricName, sourceLabel, matchesMetricQuery } from '@/i18n/metrics';
import type { MetricDefinition } from '@/types/metrics';

const savedLanguage = () => normalizeSettings(readSection('settings')).language;
let system: string[] = ['en-US'];

beforeEach(() => {
  system = ['en-US'];
  resetUiConfigForTesting();
});

afterEach(() => {
  stopLanguageSyncForTesting();
  resetUiConfigForTesting();
});

describe('active language', () => {
  it.each(LOCALE_CODES)('loads %s and answers in it', (code) => {
    setActiveLocale(code);
    expect(activeLocale()).toBe(code);
    expect(i18n.t('language.label')).not.toBe('language.label');
    expect(i18n.t('language.label').length).toBeGreaterThan(0);
    expect(document.documentElement.lang).toBe(code);
    expect(document.documentElement.dir).toBe('ltr');
  });

  it('changes at runtime, without a reload', () => {
    expect(i18n.t('language.label')).toBe('Language');
    setActiveLocale('fr');
    expect(i18n.t('language.label')).toBe('Langue');
    setActiveLocale('ja');
    expect(i18n.t('language.label')).toBe('言語');
    expect(document.documentElement.lang).toBe('ja');
  });

  it('falls back to English for a key a catalog lacks', () => {
    i18n.addResource('en', 'translation', 'test.onlyInEnglish', 'Only in English');
    setActiveLocale('de');
    expect(i18n.t('test.onlyInEnglish')).toBe('Only in English');
    // A key nobody has comes back as the key, never as an empty string.
    expect(i18n.t('test.nowhere')).toBe('test.nowhere');
  });

  it('interpolates values in every language', () => {
    for (const code of LOCALE_CODES) {
      setActiveLocale(code);
      expect(i18n.t('language.system', { language: 'Deutsch' })).toContain('Deutsch');
    }
  });

  it('chooses the plural form each language needs', () => {
    setActiveLocale('en');
    expect(i18n.t('common.widgetCount', { count: 1 })).toBe('1 widget');
    expect(i18n.t('common.widgetCount', { count: 5 })).toBe('5 widgets');
    setActiveLocale('ru');
    expect(i18n.t('common.widgetCount', { count: 1 })).toBe('1 виджет');
    expect(i18n.t('common.widgetCount', { count: 3 })).toBe('3 виджета');
    expect(i18n.t('common.widgetCount', { count: 5 })).toBe('5 виджетов');
    setActiveLocale('pl');
    expect(i18n.t('common.widgetCount', { count: 22 })).toBe('22 widżety');
    expect(i18n.t('common.widgetCount', { count: 25 })).toBe('25 widżetów');
    setActiveLocale('ja');
    expect(i18n.t('common.widgetCount', { count: 1 })).toBe('1 個のウィジェット');
  });
});

describe('language preference', () => {
  it('defaults to System, resolved against the environment', () => {
    system = ['fr-CA', 'en-US'];
    expect(readLanguagePreference()).toBe('system');
    expect(startLanguageSync({ system: () => system })).toBe('fr');
    expect(activeLocale()).toBe('fr');
    expect(document.documentElement.lang).toBe('fr');
  });

  it('applies and saves an explicit choice', () => {
    startLanguageSync({ system: () => system });
    chooseLanguage('de');
    expect(activeLocale()).toBe('de');
    expect(savedLanguage()).toBe('de');
  });

  it('survives a restart', async () => {
    const backend = memoryBackend();
    await initUiConfig(backend);
    startLanguageSync({ system: () => system });
    chooseLanguage('ko');
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(normalizeSettings(backend.saved().settings).language).toBe('ko');

    // Relaunch: a fresh window on the same saved file, in another environment.
    stopLanguageSyncForTesting();
    setActiveLocale('en');
    resetUiConfigForTesting();
    await initUiConfig(backend);
    system = ['es-MX'];
    expect(startLanguageSync({ system: () => system })).toBe('ko');
  });

  it('persists System as System, not as the language it resolved to', async () => {
    const backend = memoryBackend({ settings: { language: 'fr' } });
    await initUiConfig(backend);
    startLanguageSync({ system: () => system });
    expect(activeLocale()).toBe('fr');
    chooseLanguage('system');
    expect(activeLocale()).toBe('en');
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(normalizeSettings(backend.saved().settings).language).toBe('system');

    stopLanguageSyncForTesting();
    resetUiConfigForTesting();
    await initUiConfig(backend);
    system = ['zh-HK'];
    expect(startLanguageSync({ system: () => system })).toBe('zh-TW');
  });

  it('follows another window’s change at once', async () => {
    const backend = memoryBackend();
    await initUiConfig(backend);
    startLanguageSync({ system: () => system });
    expect(activeLocale()).toBe('en');
    backend.peer('settings', { ...normalizeSettings(readSection('settings')), language: 'it' });
    expect(activeLocale()).toBe('it');
    expect(document.documentElement.lang).toBe('it');
  });

  it('follows the environment’s own change in System mode only', () => {
    startLanguageSync({ system: () => system });
    system = ['pt-BR'];
    globalThis.dispatchEvent(new Event('languagechange'));
    expect(activeLocale()).toBe('pt-BR');

    chooseLanguage('nl');
    system = ['tr-TR'];
    globalThis.dispatchEvent(new Event('languagechange'));
    expect(activeLocale()).toBe('nl');
    expect(syncLanguage()).toBe('nl');
  });

  it('ignores an unknown stored value and follows the system', () => {
    resetUiConfigForTesting({ settings: { language: 'klingon' } });
    system = ['hi-IN'];
    expect(startLanguageSync({ system: () => system })).toBe('hi');
  });
});

describe('formatting follows the PULSE language, not the system', () => {
  it('formats numbers per locale', () => {
    expect(formatFixed(12.5, 1, { locale: 'en' })).toBe('12.5');
    expect(formatFixed(12.5, 1, { locale: 'fr' })).toBe('12,5');
    expect(formatFixed(12.5, 1, { locale: 'de' })).toBe('12,5');
    expect(formatFixed(-0, 1, { locale: 'en' })).toBe('0.0');
    expect(formatInteger(12345, 'en')).toBe('12,345');
    expect(formatInteger(12345, 'de')).toBe('12.345');
    expect(formatInteger(12345, 'fr')).toMatch(/^12\s345$/u);
    expect(formatInteger(1234567, 'hi')).toBe('12,34,567');
  });

  it('uses the active locale by default', () => {
    setActiveLocale('de');
    expect(formatFixed(3.25, 2)).toBe('3,25');
    setActiveLocale('ja');
    expect(formatFixed(3.25, 2)).toBe('3.25');
  });

  it('formats dates and relative times per locale', () => {
    const when = Date.UTC(2026, 0, 15, 12, 0, 0);
    const options: Intl.DateTimeFormatOptions = { month: 'long', timeZone: 'UTC' };
    expect(formatDateTime(when, options, 'en')).toBe('January');
    expect(formatDateTime(when, options, 'fr')).toBe('janvier');
    expect(formatDateTime(when, options, 'de')).toBe('Januar');
    expect(formatDateTime(when, options, 'ja')).toBe('1月');
    expect(formatRelativeTime(-1, 'day', 'fr')).toBe('hier');
    expect(formatRelativeTime(-1, 'day', 'en')).toBe('yesterday');
  });
});

describe('built-in text and identifiers', () => {
  it('stores English with a key and displays the active language', () => {
    const template = findTemplate('gaming') ?? findTemplate('performance');
    expect(template).toBeDefined();
    setActiveLocale('fr');
    const { section, id } = createDashboardFromTemplate(defaultSection(), template!);
    const dashboard = section.items.find((item) => item.id === id)!;

    // Persisted: English text plus the key, never the French.
    expect(dashboard.nameKey).toBe(templateNameKey(template!.id));
    expect(dashboard.name).toBe(englishOf({ key: templateNameKey(template!.id) }));
    // Shown: French, and the stable ids do not depend on the language.
    expect(dashboardName(dashboard)).toBe(i18n.getFixedT('fr')(templateNameKey(template!.id)));
    expect(dashboard.origin).toEqual({ template: template!.id, version: expect.any(Number) });

    setActiveLocale('ja');
    expect(dashboardName(dashboard)).toBe(i18n.getFixedT('ja')(templateNameKey(template!.id)));
  });

  it('keeps the user’s own words exactly, in every language', () => {
    const section = defaultSection();
    const renamed = renameDashboard(section, section.items[0]!.id, 'Mon tableau');
    setActiveLocale('de');
    expect(dashboardName(renamed.items[0]!)).toBe('Mon tableau');
    expect(storedText(null, 'Mon tableau')).toBe('Mon tableau');
  });

  it('creates the same ids and metric keys whatever the language', () => {
    const shape = () => findTemplate('gaming') ?? findTemplate('performance')!;
    setActiveLocale('en');
    const english = shape()
      .widgets()
      .map((widget) => [widget.kind, widget.bindings.map((b) => b.key)]);
    setActiveLocale('zh-CN');
    const chinese = shape()
      .widgets()
      .map((widget) => [widget.kind, widget.bindings.map((b) => b.key)]);
    expect(chinese).toEqual(english);
  });

  it('translates metric names by key, keeps hardware names, and searches in both', () => {
    const definition = {
      metric: { key: 'cpu.usage.total', sourceId: 'cpu:system' },
      displayName: 'CPU Total',
      description: 'Share of time',
      sourceLabel: 'System',
    } as unknown as MetricDefinition;
    const gpu = {
      metric: { key: 'gpu.usage', sourceId: 'gpu:nvidia-0' },
      displayName: 'GPU usage',
      description: '',
      sourceLabel: 'NVIDIA GeForce RTX 4070',
    } as unknown as MetricDefinition;

    setActiveLocale('fr');
    expect(metricName(definition)).toBe(i18n.t('metrics.catalog.cpu.usage.total.name'));
    expect(sourceLabel(gpu)).toBe('NVIDIA GeForce RTX 4070');
    // An unknown metric keeps the backend's name.
    expect(metricName({ metric: { key: 'future.metric' }, displayName: 'Future' } as never)).toBe(
      'Future',
    );
    // The French name and the English name both find it.
    const french = i18n.t('metrics.catalog.cpu.usage.total.name').split(/\s+/)[0]!;
    expect(matchesMetricQuery(definition, french)).toBe(true);
    expect(matchesMetricQuery(definition, 'total')).toBe(true);
    expect(matchesMetricQuery(definition, 'cpu.usage')).toBe(true);
    expect(matchesMetricQuery(definition, 'zzz')).toBe(false);
  });
});
