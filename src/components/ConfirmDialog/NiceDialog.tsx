import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ConfirmDialog } from '@/components/ConfirmDialog/ConfirmDialog';
import { NICE_MAX, NICE_MIN } from '@/utils/processes';

/**
 * Sets an exact Linux nice value.
 *
 * The real system value is what is edited — presets elsewhere are shortcuts to
 * it. Lowering a nice value below where it is usually needs privileges PULSE
 * does not have; the kernel's refusal is reported as-is.
 */
export function NiceDialog({
  name,
  pid,
  current,
  onApply,
  onCancel,
}: {
  readonly name: string;
  readonly pid: number;
  readonly current: number | null;
  readonly onApply: (value: number) => void;
  readonly onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [text, setText] = useState(String(current ?? 0));
  const value = Number(text);
  const valid =
    text.trim() !== '' && Number.isInteger(value) && value >= NICE_MIN && value <= NICE_MAX;

  return (
    <ConfirmDialog
      title={t('processes.nice.title', { name })}
      body={[
        t('processes.nice.range', { pid, min: NICE_MIN, max: NICE_MAX }),
        t('processes.nice.privileges'),
      ]}
      confirmLabel={t('common.apply')}
      tone="neutral"
      confirmDisabled={!valid}
      onConfirm={() => onApply(value)}
      onCancel={onCancel}
    >
      <label className="nice-input">
        {t('processes.nice.value')}
        <input
          type="number"
          min={NICE_MIN}
          max={NICE_MAX}
          step={1}
          value={text}
          onChange={(event) => setText(event.target.value)}
        />
      </label>
    </ConfirmDialog>
  );
}
