import { useEffect } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isTauriRuntime } from '@/services/tauri';
import type { Overlay } from '@/overlay/model';
import { overlayChromeStyle } from '@/overlay/style';
import { updateOverlay } from '@/overlay/model';
import { updateOverlays, useOverlays } from '@/overlay/store';
import { openMainWindow } from '@/overlay/desktop';
import { WidgetCard } from '@/components/Dashboard/WidgetCard';

/**
 * The content of one desktop overlay window.
 *
 * A separate Tauri window (`overlay-<id>`), frameless and transparent, created
 * and placed by the backend. It renders its widgets with the same engine as
 * the dashboard — there is no second drawing path — and reads the shared
 * configuration, so an edit in the main window appears here at once.
 *
 * - **Edit**: a thin bar to drag the window, *Lock* and *Open PULSE*, and a
 *   resize grip.
 * - **Locked**: no bar, no grip — only the widgets. The backend makes the
 *   window click-through and unable to take focus where the platform allows.
 */
export function OverlayApp({ id }: { readonly id: string }) {
  const overlay = useOverlays().items.find((item) => item.id === id);

  useEffect(() => {
    document.documentElement.classList.add('pulse-overlay');
    return () => document.documentElement.classList.remove('pulse-overlay');
  }, []);

  if (!overlay) return null;
  return <OverlaySurface overlay={overlay} />;
}

export function OverlaySurface({
  overlay,
  preview = false,
}: {
  readonly overlay: Overlay;
  readonly preview?: boolean;
}) {
  const editing = !overlay.locked && !preview;
  const layout =
    overlay.layout === 'grid'
      ? { display: 'grid', gridTemplateColumns: `repeat(${overlay.columns}, max-content)` }
      : {
          display: 'flex',
          flexDirection: overlay.layout === 'vertical' ? ('column' as const) : ('row' as const),
        };

  return (
    <div
      className={`overlay${editing ? ' overlay--editing' : ''}`}
      style={overlayChromeStyle(overlay)}
      aria-label={`${overlay.name} overlay`}
    >
      {overlay.chrome.background && (
        <span
          className="overlay__backdrop"
          aria-hidden="true"
          style={{
            background: overlay.chrome.background,
            opacity: overlay.chrome.opacity,
            borderRadius: overlay.chrome.radius,
          }}
        />
      )}
      {editing && (
        <div className="overlay__bar" data-tauri-drag-region>
          <span className="overlay__name" data-tauri-drag-region>
            {overlay.name} · drag to move
          </span>
          <button
            type="button"
            className="overlay__button"
            onClick={() =>
              updateOverlays((section) =>
                updateOverlay(section, overlay.id, (o) => ({ ...o, locked: true })),
              )
            }
          >
            Lock
          </button>
          <button
            type="button"
            className="overlay__button"
            onClick={() => void openMainWindow().catch(() => undefined)}
          >
            Open PULSE
          </button>
        </div>
      )}
      <div className="overlay__widgets" style={{ ...layout, gap: overlay.gap }}>
        {overlay.widgets.map((widget) => (
          <WidgetCard
            key={widget.id}
            widget={widget}
            width={widget.size.width}
            height={widget.size.height}
            editing={false}
            className="widget--overlay"
          />
        ))}
        {overlay.widgets.length === 0 && (
          <p className="overlay__empty">Add widgets from PULSE → Overlays.</p>
        )}
      </div>
      {editing && (
        <span
          className="overlay__resize"
          title="Drag to resize"
          onPointerDown={(event) => {
            event.preventDefault();
            if (isTauriRuntime())
              void getCurrentWindow()
                .startResizeDragging('SouthEast')
                .catch(() => undefined);
          }}
        />
      )}
    </div>
  );
}
