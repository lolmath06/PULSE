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
  followLabel = 'App style',
}: {
  readonly value: StyleId | null;
  readonly onChange: (value: StyleId | null) => void;
  readonly label: string;
  readonly followLabel?: string;
}) {
  return (
    <label className="style-picker">
      Style
      <select
        className="history-panel__select"
        aria-label={label}
        value={value ?? ''}
        onChange={(event) => onChange(isStyleId(event.target.value) ? event.target.value : null)}
      >
        <option value="">{followLabel}</option>
        {STYLES.map((style) => (
          <option key={style.id} value={style.id}>
            {style.name}
          </option>
        ))}
      </select>
    </label>
  );
}
