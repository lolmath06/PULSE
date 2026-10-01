import { useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import type { WidgetCardActions } from '@/components/Dashboard/WidgetCard';

/**
 * The compact edit tools of a small widget: one "…" button whose menu —
 * Customize, Duplicate, To overlay, Remove — opens in a portal, so the
 * widget's own clipping never cuts it and it never covers the metric for
 * longer than it is open.
 */
export function WidgetMenu({
  title,
  actions,
  extra,
}: {
  readonly title: string;
  readonly actions: WidgetCardActions;
  readonly extra?: ReactNode;
}) {
  const { t } = useTranslation();
  const [anchor, setAnchor] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!anchor) return;
    const close = (event: Event) => {
      if (event instanceof KeyboardEvent && event.key !== 'Escape') return;
      if (event instanceof MouseEvent && menuRef.current?.contains(event.target as Node)) return;
      setAnchor(null);
    };
    window.addEventListener('mousedown', close);
    window.addEventListener('keydown', close);
    return () => {
      window.removeEventListener('mousedown', close);
      window.removeEventListener('keydown', close);
    };
  }, [anchor]);

  const run = (action?: () => void) => () => {
    setAnchor(null);
    action?.();
  };

  return (
    <>
      <button
        type="button"
        className="widget__tool"
        aria-label={t('widget.actionsMenu', { name: title })}
        aria-haspopup="menu"
        aria-expanded={anchor !== null}
        onClick={(event) => {
          const box = event.currentTarget.getBoundingClientRect();
          setAnchor(anchor ? null : { x: box.right, y: box.bottom + 4 });
        }}
      >
        …
      </button>
      {anchor &&
        createPortal(
          <div
            ref={menuRef}
            className="widget-menu"
            role="menu"
            aria-label={t('widget.actions', { name: title })}
            style={{ left: Math.max(4, anchor.x - 160), top: anchor.y }}
          >
            {actions.onCustomize && (
              <button type="button" role="menuitem" onClick={run(actions.onCustomize)}>
                {t('common.customize')}
              </button>
            )}
            {actions.onDuplicate && (
              <button type="button" role="menuitem" onClick={run(actions.onDuplicate)}>
                {t('common.duplicate')}
              </button>
            )}
            {actions.onSendToOverlay && (
              <button type="button" role="menuitem" onClick={run(actions.onSendToOverlay)}>
                {t('widget.toOverlay')}
              </button>
            )}
            {extra}
            {actions.onRemove && (
              <button
                type="button"
                role="menuitem"
                className="widget-menu__danger"
                onClick={run(actions.onRemove)}
              >
                {t('common.remove')}
              </button>
            )}
          </div>,
          document.body,
        )}
    </>
  );
}
