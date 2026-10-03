import { useEffect, useRef } from 'react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { useDialogWindow } from '@/hooks/useDialogWindow';

/**
 * A modal confirmation.
 *
 * Focus starts on **Cancel**, so an Enter pressed out of habit never confirms
 * a destructive action. Escape cancels. Nothing is sent to the backend until
 * the confirm button is pressed.
 */
export function ConfirmDialog({
  title,
  body,
  confirmLabel,
  tone,
  onConfirm,
  onCancel,
  children,
  confirmDisabled = false,
}: {
  readonly title: string;
  readonly body: readonly string[];
  readonly confirmLabel: string;
  readonly tone: 'danger' | 'warning' | 'neutral';
  readonly onConfirm: () => void;
  readonly onCancel: () => void;
  readonly children?: ReactNode;
  readonly confirmDisabled?: boolean;
}) {
  useDialogWindow();
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    cancelRef.current?.focus();
  }, []);

  return (
    <div className="dialog-backdrop" onMouseDown={onCancel}>
      <div
        className={`dialog dialog--${tone}`}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="pulse-dialog-title"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.stopPropagation();
            onCancel();
          }
        }}
      >
        <h3 id="pulse-dialog-title" className="dialog__title">
          {title}
        </h3>
        {body.map((line) => (
          <p key={line} className="dialog__text">
            {line}
          </p>
        ))}
        {children}
        <div className="dialog__actions">
          <button ref={cancelRef} type="button" className="button button--quiet" onClick={onCancel}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className={tone === 'neutral' ? 'button' : `button button--${tone}`}
            onClick={onConfirm}
            disabled={confirmDisabled}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
