import { useState } from 'react';
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
  const [text, setText] = useState(String(current ?? 0));
  const value = Number(text);
  const valid =
    text.trim() !== '' && Number.isInteger(value) && value >= NICE_MIN && value <= NICE_MAX;

  return (
    <ConfirmDialog
      title={`Custom nice value — ${name}`}
      body={[
        `PID ${pid}. Nice values run from ${NICE_MIN} (most favoured) to ${NICE_MAX} (least).`,
        'Lowering the value usually requires privileges PULSE does not have.',
      ]}
      confirmLabel="Apply"
      tone="neutral"
      confirmDisabled={!valid}
      onConfirm={() => onApply(value)}
      onCancel={onCancel}
    >
      <label className="nice-input">
        Nice value
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
