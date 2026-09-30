import { useMemo } from 'react';
import type { CSSProperties, KeyboardEvent, PointerEvent, ReactNode } from 'react';
import type { WidgetInstance } from '@/dashboard/model';
import { WidgetContent } from '@/components/Dashboard/WidgetContent';
import { TITLE_HEIGHT, frameLayout, widgetTitle } from '@/dashboard/geometry';
import { WidgetMenu } from '@/components/Dashboard/WidgetMenu';
import { useLook } from '@/design/hooks';
import { styleWidget } from '@/design/look';

export interface WidgetCardActions {
  readonly onCustomize?: () => void;
  readonly onDuplicate?: () => void;
  readonly onRemove?: () => void;
  readonly onSendToOverlay?: () => void;
  readonly onMoveStart?: (event: PointerEvent<HTMLButtonElement>) => void;
  readonly onResizeStart?: (event: PointerEvent<HTMLSpanElement>) => void;
  readonly onKeyDown?: (event: KeyboardEvent<HTMLElement>) => void;
}

/**
 * One widget's frame: its own background, opacity, border, radius, padding
 * and title, around {@link WidgetContent}.
 *
 * In edit mode it gains an explicit drag handle, a resize grip and its
 * actions. The chart itself is never the drag surface, so its tooltip keeps
 * working and a click on it never moves anything.
 */
export function WidgetCard({
  widget: stored,
  width,
  height,
  editing,
  enabled = true,
  actions = {},
  style,
  className,
  extraTools,
}: {
  readonly widget: WidgetInstance;
  readonly width: number;
  readonly height: number;
  readonly editing: boolean;
  readonly enabled?: boolean;
  readonly actions?: WidgetCardActions;
  readonly style?: CSSProperties;
  readonly className?: string;
  readonly extraTools?: ReactNode;
}) {
  const look = useLook();
  // Drawn under the surface's style; the stored widget is never changed.
  const styled = useMemo(() => styleWidget(stored, look), [stored, look]);
  const layout = frameLayout(styled, width, height);
  // Title hierarchy: when the card's title already names a single-metric
  // chart, the chart does not repeat it in its own header.
  const widget = useMemo(
    () =>
      layout.showTitle &&
      styled.kind === 'visualization' &&
      styled.bindings.length === 1 &&
      styled.visual.config.text.showLabel
        ? {
            ...styled,
            visual: {
              ...styled.visual,
              config: {
                ...styled.visual.config,
                text: { ...styled.visual.config.text, showLabel: false },
              },
            },
          }
        : styled,
    [styled, layout.showTitle],
  );
  const box = layout.content;
  const title = widgetTitle(widget);
  const frame = widget.frame;

  return (
    <article
      className={`widget${editing ? ' widget--editing' : ''}${
        look.widget.surface === 'bare' ? ' widget--bare' : ''
      }${className ? ` ${className}` : ''}`}
      aria-label={title}
      tabIndex={editing ? 0 : undefined}
      onKeyDown={editing ? actions.onKeyDown : undefined}
      style={{
        ...style,
        width,
        height,
        padding: layout.padding,
        borderRadius: frame.radius,
        borderWidth: 1,
        borderStyle: 'solid',
        borderColor: frame.border === 'thin' ? 'var(--pulse-border)' : 'transparent',
      }}
      data-widget-id={widget.id}
    >
      {frame.background && (
        <span
          className="widget__backdrop"
          aria-hidden="true"
          style={{
            background: frame.background,
            opacity: frame.opacity,
            borderRadius: frame.radius,
          }}
        />
      )}
      {layout.showTitle && (
        <header className="widget__title" style={{ height: TITLE_HEIGHT }}>
          <span className="widget__title-text">{title}</span>
        </header>
      )}
      <div className="widget__body" style={{ width: box.width, height: box.height }}>
        <WidgetContent widget={widget} width={box.width} height={box.height} enabled={enabled} />
      </div>

      {editing && (
        <>
          <div
            className={`widget__tools${layout.compactTools ? ' widget__tools--compact' : ''}`}
            role="toolbar"
            aria-label={`${title} actions`}
          >
            {actions.onMoveStart && (
              <button
                type="button"
                className="widget__tool widget__handle"
                aria-label={`Drag to move ${title}`}
                title="Drag to move (or use the arrow keys)"
                onPointerDown={actions.onMoveStart}
              >
                ⠿
              </button>
            )}
            {layout.compactTools ? (
              <WidgetMenu title={title} actions={actions} extra={extraTools} />
            ) : (
              <>
                {actions.onCustomize && (
                  <button type="button" className="widget__tool" onClick={actions.onCustomize}>
                    Customize
                  </button>
                )}
                {actions.onDuplicate && (
                  <button type="button" className="widget__tool" onClick={actions.onDuplicate}>
                    Duplicate
                  </button>
                )}
                {actions.onSendToOverlay && (
                  <button type="button" className="widget__tool" onClick={actions.onSendToOverlay}>
                    To overlay
                  </button>
                )}
                {extraTools}
                {actions.onRemove && (
                  <button
                    type="button"
                    className="widget__tool widget__tool--danger"
                    aria-label={`Remove ${title}`}
                    onClick={actions.onRemove}
                  >
                    ×
                  </button>
                )}
              </>
            )}
          </div>
          {actions.onResizeStart && (
            <span
              className="widget__resize"
              role="presentation"
              title="Drag to resize (or Shift + arrow keys)"
              onPointerDown={actions.onResizeStart}
            />
          )}
        </>
      )}
    </article>
  );
}
