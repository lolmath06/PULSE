import { useEffect } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isTauriRuntime } from '@/services/tauri';
import type { Overlay } from '@/overlay/model';
import { overlayChromeStyle } from '@/overlay/style';
import { overlayLayout, updateOverlay } from '@/overlay/model';
import { useElementSize } from '@/visualization/useElementSize';
import { updateOverlays, useOverlays } from '@/overlay/store';
import { openMainWindow } from '@/overlay/desktop';
import { WidgetCard } from '@/components/Dashboard/WidgetCard';
import { RootLook, StyleScope } from '@/design/LookContext';
import { useScopedLook } from '@/design/hooks';

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
  return <OverlayWindow overlay={overlay} />;
}

/** The overlay window wears the overlay's style (or the app's) on `:root`. */
function OverlayWindow({ overlay }: { readonly overlay: Overlay }) {
  const look = useScopedLook(overlay.styleId);
  return (
    <RootLook look={look}>
      <OverlaySurface overlay={overlay} />
    </RootLook>
  );
}

export function OverlaySurface({
  overlay,
  preview = false,
}: {
  readonly overlay: Overlay;
  readonly preview?: boolean;
}) {
  const editing = !overlay.locked && !preview;
  const layout = overlayLayout(overlay);
  const byId = new Map(overlay.widgets.map((widget) => [widget.id, widget]));

  return (
    <div
      className={`overlay${editing ? ' overlay--editing' : ''}${preview ? ' overlay--preview' : ''}`}
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
      <div
        className="overlay__content"
        style={{ width: layout.width, height: layout.height }}
        data-layout={overlay.layout}
      >
        {layout.boxes.map((box) => {
          const widget = byId.get(box.id)!;
          return (
            <div
              key={box.id}
              className="overlay__slot"
              style={{ left: box.x, top: box.y, width: box.width, height: box.height }}
              data-slot={box.id}
            >
              <WidgetCard
                widget={widget}
                width={box.width}
                height={box.height}
                editing={false}
                className="widget--overlay"
              />
            </div>
          );
        })}
        {overlay.widgets.length === 0 && (
          <p className="overlay__empty">Add widgets from PULSE → Overlays.</p>
        )}
      </div>
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

/**
 * The overlay exactly as its window draws it, scaled **uniformly** to fit the
 * editor. Children keep their boxes; only the whole composition shrinks.
 */
export function OverlayPreview({ overlay }: { readonly overlay: Overlay }) {
  const look = useScopedLook(overlay.styleId);
  const [ref, size] = useElementSize<HTMLDivElement>();
  const layout = overlayLayout(overlay);
  const available = size.width || layout.width;
  const scale = Math.min(1, available / Math.max(1, layout.width));
  return (
    <div ref={ref} className="overlay-preview" data-scale={scale.toFixed(3)}>
      <div
        className="overlay-preview__frame"
        style={{ width: layout.width * scale, height: layout.height * scale }}
      >
        <div
          className="overlay-preview__inner"
          style={{ width: layout.width, height: layout.height, transform: `scale(${scale})` }}
        >
          <StyleScope look={look} className="style-scope--bare">
            <OverlaySurface overlay={overlay} preview />
          </StyleScope>
        </div>
      </div>
    </div>
  );
}
