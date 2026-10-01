import { useTranslation } from 'react-i18next';
import type { StyleId } from '@/design/styles';
import { STYLES, isStyleId } from '@/design/styles';

/**
 * Chooses the style one part of PULSE wears — a dashboard, an overlay, Mini —
 * or none of its own (it then follows the app's style).
 */
export function StylePicker({
  value,
  onChange,
  label,
  followLabel,
}: {
  readonly value: StyleId | null;
  readonly onChange: (value: StyleId | null) => void;
  readonly label: string;
  readonly followLabel?: string;
}) {
  const { t } = useTranslation();
  return (
    <label className="style-picker">
      {t('nav.style')}
      <select
        className="history-panel__select"
        aria-label={label}
        value={value ?? ''}
        onChange={(event) => onChange(isStyleId(event.target.value) ? event.target.value : null)}
      >
        <option value="">{followLabel ?? t('presets.appStyle')}</option>
        {STYLES.map((style) => (
          <option key={style.id} value={style.id}>
            {t(`styles.${style.id}.name`)}
          </option>
        ))}
      </select>
    </label>
  );
}
