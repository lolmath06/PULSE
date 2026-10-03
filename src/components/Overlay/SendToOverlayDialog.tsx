import { useTranslation } from 'react-i18next';
import type { WidgetInstance } from '@/dashboard/model';
import { widgetTitle } from '@/dashboard/geometry';
import { MAX_OVERLAYS, addOverlayWidget, createOverlay, overlayName } from '@/overlay/model';
import { updateOverlays, useOverlays } from '@/overlay/store';
import { useDialogWindow } from '@/hooks/useDialogWindow';

/**
 * Copies a dashboard widget into an overlay — a new one, or an existing one.
 * The copy keeps the metric, renderer and style, and gets its own id and a
 * pixel size; the dashboard widget is untouched.
 */
export function SendToOverlayDialog({
  widget,
  onClose,
  onSent,
}: {
  readonly widget: WidgetInstance;
  readonly onClose: () => void;
  readonly onSent?: (overlayName: string) => void;
}) {
  useDialogWindow();
  const { t } = useTranslation();
  const overlays = useOverlays();
  const title = widgetTitle(widget);

  return (
    <div className="dialog-backdrop" onMouseDown={onClose}>
      <div
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-label={t('overlays.send.aria', { name: title })}
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === 'Escape') onClose();
        }}
      >
        <h3 className="dialog__title">{t('overlays.send.title', { name: title })}</h3>
        <div className="send-overlay">
          <button
            type="button"
            className="button"
            disabled={overlays.items.length >= MAX_OVERLAYS}
            onClick={() => {
              // A built-in title keeps its key, so the overlay's name follows
              // the language; anything else is named as the widget reads now.
              updateOverlays(
                (section) =>
                  createOverlay(
                    section,
                    widget.titleKey && widget.title ? widget.title : title,
                    [widget],
                    widget.titleKey ? { nameKey: widget.titleKey } : {},
                  ).section,
              );
              onSent?.(title);
              onClose();
            }}
          >
            {t('overlays.page.newOverlay')}
          </button>
          {overlays.items.map((overlay) => (
            <button
              key={overlay.id}
              type="button"
              className="button button--quiet"
              onClick={() => {
                updateOverlays((section) => addOverlayWidget(section, overlay.id, widget));
                onSent?.(overlayName(overlay));
                onClose();
              }}
            >
              {t('overlays.send.addTo', { name: overlayName(overlay) })}
            </button>
          ))}
        </div>
        <div className="dialog__actions">
          <button type="button" className="button" onClick={onClose}>
            {t('common.cancel')}
          </button>
        </div>
      </div>
    </div>
  );
}
