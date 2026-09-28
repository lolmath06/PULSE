import type { WidgetInstance } from '@/dashboard/model';
import { widgetTitle } from '@/dashboard/geometry';
import { MAX_OVERLAYS, addOverlayWidget, createOverlay } from '@/overlay/model';
import { updateOverlays, useOverlays } from '@/overlay/store';

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
  const overlays = useOverlays();
  const title = widgetTitle(widget);

  return (
    <div className="dialog-backdrop" onMouseDown={onClose}>
      <div
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-label={`Send ${title} to an overlay`}
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === 'Escape') onClose();
        }}
      >
        <h3 className="dialog__title">{`Send “${title}” to an overlay`}</h3>
        <div className="send-overlay">
          <button
            type="button"
            className="button"
            disabled={overlays.items.length >= MAX_OVERLAYS}
            onClick={() => {
              updateOverlays((section) => createOverlay(section, title, [widget]).section);
              onSent?.(title);
              onClose();
            }}
          >
            New overlay
          </button>
          {overlays.items.map((overlay) => (
            <button
              key={overlay.id}
              type="button"
              className="button button--quiet"
              onClick={() => {
                updateOverlays((section) => addOverlayWidget(section, overlay.id, widget));
                onSent?.(overlay.name);
                onClose();
              }}
            >
              {`Add to “${overlay.name}”`}
            </button>
          ))}
        </div>
        <div className="dialog__actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
