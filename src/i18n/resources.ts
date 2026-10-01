import type { Resource } from 'i18next';
import type { LocaleCode } from '@/i18n/locales';
import de from '@/i18n/locales/de.json';
import en from '@/i18n/locales/en.json';
import es from '@/i18n/locales/es.json';
import fr from '@/i18n/locales/fr.json';
import hi from '@/i18n/locales/hi.json';
import id from '@/i18n/locales/id.json';
import it from '@/i18n/locales/it.json';
import ja from '@/i18n/locales/ja.json';
import ko from '@/i18n/locales/ko.json';
import nl from '@/i18n/locales/nl.json';
import pl from '@/i18n/locales/pl.json';
import ptBR from '@/i18n/locales/pt-BR.json';
import ru from '@/i18n/locales/ru.json';
import tr from '@/i18n/locales/tr.json';
import zhCN from '@/i18n/locales/zh-CN.json';
import zhTW from '@/i18n/locales/zh-TW.json';

/**
 * Every translation, bundled with the app: one JSON file per locale, one
 * `translation` namespace each. Nothing is ever fetched at runtime.
 */
export const LOCALE_MESSAGES: Readonly<Record<LocaleCode, Record<string, unknown>>> = {
  en,
  fr,
  es,
  de,
  it,
  'pt-BR': ptBR,
  nl,
  pl,
  ru,
  tr,
  id,
  hi,
  ja,
  ko,
  'zh-CN': zhCN,
  'zh-TW': zhTW,
};

export const RESOURCES: Resource = Object.fromEntries(
  Object.entries(LOCALE_MESSAGES).map(([code, translation]) => [code, { translation }]),
) as Resource;
