import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { LOCALES, isLanguagePreference, localeInfo, resolvePreference } from '@/i18n/locales';
import { chooseLanguage, useLanguagePreference } from '@/i18n/language';

/**
 * The interface language: *System* or one of PULSE's languages, each named in
 * itself so anyone can find their own. A change applies at once, in every
 * PULSE window, and is saved with the rest of the shared configuration.
 */
export function LanguageSelect({ compact = false }: { readonly compact?: boolean }) {
  const { t } = useTranslation();
  const preference = useLanguagePreference();
  const id = useId();
  const system = localeInfo(resolvePreference('system'));

  return (
    <div className={`language-select${compact ? ' language-select--compact' : ''}`}>
      <label className="language-select__label" htmlFor={id}>
        {t('language.label')}
      </label>
      <select
        id={id}
        className="history-panel__select language-select__control"
        value={preference}
        onChange={(event) => {
          if (isLanguagePreference(event.target.value)) chooseLanguage(event.target.value);
        }}
      >
        <option value="system">{t('language.system', { language: system.nativeName })}</option>
        {LOCALES.map((locale) => (
          <option key={locale.code} value={locale.code} lang={locale.code}>
            {locale.nativeName}
          </option>
        ))}
      </select>
      {!compact && <p className="card__note language-select__hint">{t('language.hint')}</p>}
    </div>
  );
}
