import { useEffect, useRef } from 'react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Icon } from '@/components/Icon';
import { useDialogWindow } from '@/hooks/useDialogWindow';

/**
 * A wide modal for choosing among rich options (templates, the welcome).
 * Escape or a click outside closes it; focus starts on the close button.
 */
export function Sheet({
  title,
  subtitle,
  onClose,
  children,
  footer,
  label,
}: {
  readonly title: string;
  readonly subtitle?: string;
  readonly onClose: () => void;
  readonly children: ReactNode;
  readonly footer?: ReactNode;
  readonly label?: string;
}) {
  useDialogWindow();
  const { t } = useTranslation();
  const closeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => closeRef.current?.focus(), []);
  return (
    <div className="dialog-backdrop" onMouseDown={onClose}>
      <div
        className="sheet"
        role="dialog"
        aria-modal="true"
        aria-label={label ?? title}
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.stopPropagation();
            onClose();
          }
        }}
      >
        <header className="sheet__header">
          <div>
            <h2 className="sheet__title">{title}</h2>
            {subtitle && <p className="sheet__subtitle">{subtitle}</p>}
          </div>
          <button
            ref={closeRef}
            type="button"
            className="button button--quiet sheet__close"
            aria-label={t('common.close')}
            onClick={onClose}
          >
            <Icon name="close" />
          </button>
        </header>
        <div className="sheet__body">{children}</div>
        {footer && <footer className="sheet__footer">{footer}</footer>}
      </div>
    </div>
  );
}
