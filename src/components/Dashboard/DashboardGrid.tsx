import { useMemo, useRef, useState } from 'react';
import type { KeyboardEvent, PointerEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { dashboardName } from '@/dashboard/dashboards';
import type { Dashboard, GridRect, WidgetInstance } from '@/dashboard/model';
import { GRID_COLUMNS, ROW_HEIGHT } from '@/dashboard/model';
import { useLook } from '@/design/hooks';
import { bottom, moveWidget, reflow, resizeWidget, toPixels } from '@/dashboard/layout';
import { useElementSize } from '@/visualization/useElementSize';
import { useInViewport } from '@/hooks/useInViewport';
import { WidgetCard } from '@/components/Dashboard/WidgetCard';
import { columnsFor } from '@/dashboard/geometry';

interface Drag {
  readonly id: string;
  readonly mode: 'move' | 'resize';
  readonly startX: number;
  readonly startY: number;
  readonly origin: GridRect;
  readonly target: GridRect;
}

export interface GridActions {
  readonly move: (id: string, x: number, y: number) => void;
  readonly resize: (id: string, w: number, h: number) => void;
  readonly remove: (id: string) => void;
  readonly duplicate: (id: string) => void;
  readonly customize: (id: string) => void;
  readonly sendToOverlay?: (id: string) => void;
}

/**
 * The dashboard grid: 12 columns of cells, rows of fixed height.
 *
 * - **Locked**: nothing moves; charts are fully interactive.
 * - **Edit**: each widget gets a drag handle and a resize grip, and is
 *   keyboard-operable — focus it, then arrows move it, Shift + arrows resize
 *   it, Delete removes it, Enter customizes it.
 *
 * While dragging, the other widgets make room live (the same pure layout
 * functions that commit the change), and nothing is saved until release.
 * On narrow windows the layout reflows to 6 or 1 columns for display only;
 * editing then waits for a wider window so the stored 12-column layout is
 * never scrambled.
 */
export function DashboardGrid({
  dashboard,
  editing,
  actions,
  fallbackWidth = 960,
}: {
  readonly dashboard: Dashboard;
  readonly editing: boolean;
  readonly actions: GridActions;
  readonly fallbackWidth?: number;
}) {
  const { t } = useTranslation();
  const [measureRef, measured] = useElementSize<HTMLDivElement>();
  const width = measured.width || fallbackWidth;
  const columns = columnsFor(width);
  // The gap belongs to the look (density, or the user's own); grid units do not change.
  const gap = useLook().gridGap;
  const canEdit = editing && columns === GRID_COLUMNS;
  const columnWidth = (width - gap * (columns - 1)) / columns;
  const [drag, setDrag] = useState<Drag | null>(null);
  const dragRef = useRef<Drag | null>(null);

  const shown: WidgetInstance[] = useMemo(() => {
    let widgets = reflow(dashboard.widgets, columns);
    if (drag) {
      widgets =
        drag.mode === 'move'
          ? moveWidget(widgets, drag.id, drag.target.x, drag.target.y)
          : resizeWidget(widgets, drag.id, drag.target.w, drag.target.h);
    }
    return widgets;
  }, [dashboard.widgets, columns, drag]);

  const height = Math.max(1, bottom(shown)) * (ROW_HEIGHT + gap);

  const begin =
    (widget: WidgetInstance, mode: Drag['mode']) => (event: PointerEvent<HTMLElement>) => {
      if (!canEdit || event.button !== 0) return;
      event.preventDefault();
      event.currentTarget.setPointerCapture?.(event.pointerId);
      const next: Drag = {
        id: widget.id,
        mode,
        startX: event.clientX,
        startY: event.clientY,
        origin: widget.layout,
        target: widget.layout,
      };
      dragRef.current = next;
      setDrag(next);
      const cellX = columnWidth + gap;
      const cellY = ROW_HEIGHT + gap;

      const onMove = (move: globalThis.PointerEvent) => {
        const current = dragRef.current;
        if (!current) return;
        const dx = Math.round((move.clientX - current.startX) / cellX);
        const dy = Math.round((move.clientY - current.startY) / cellY);
        const target =
          current.mode === 'move'
            ? { ...current.origin, x: current.origin.x + dx, y: Math.max(0, current.origin.y + dy) }
            : { ...current.origin, w: current.origin.w + dx, h: current.origin.h + dy };
        if (
          target.x === current.target.x &&
          target.y === current.target.y &&
          target.w === current.target.w &&
          target.h === current.target.h
        )
          return;
        dragRef.current = { ...current, target };
        setDrag(dragRef.current);
      };
      const onUp = () => {
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
        window.removeEventListener('pointercancel', onUp);
        const done = dragRef.current;
        dragRef.current = null;
        setDrag(null);
        if (!done) return;
        if (done.mode === 'move') actions.move(done.id, done.target.x, done.target.y);
        else actions.resize(done.id, done.target.w, done.target.h);
      };
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
      window.addEventListener('pointercancel', onUp);
    };

  const onKey = (widget: WidgetInstance) => (event: KeyboardEvent<HTMLElement>) => {
    if (event.target !== event.currentTarget) return;
    const { x, y, w, h } = widget.layout;
    const step: Record<string, [number, number]> = {
      ArrowLeft: [-1, 0],
      ArrowRight: [1, 0],
      ArrowUp: [0, -1],
      ArrowDown: [0, 1],
    };
    const delta = step[event.key];
    if (delta) {
      event.preventDefault();
      if (event.shiftKey) actions.resize(widget.id, w + delta[0], h + delta[1]);
      else actions.move(widget.id, x + delta[0], Math.max(0, y + delta[1]));
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault();
      actions.remove(widget.id);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      actions.customize(widget.id);
    }
  };

  return (
    <div ref={measureRef} className="dashboard-grid-host">
      {editing && !canEdit && <p className="card__note">{t('dashboard.widenToEdit')}</p>}
      <div
        className={`dashboard-grid${canEdit ? ' dashboard-grid--editing' : ''}`}
        style={{ height, ['--grid-columns' as string]: columns }}
        role="list"
        aria-label={t('dashboard.widgets', { name: dashboardName(dashboard) })}
      >
        {shown.map((widget) => {
          const box = toPixels(widget.layout, columnWidth, ROW_HEIGHT, gap);
          const dragging = drag?.id === widget.id;
          return (
            <GridItem
              key={widget.id}
              widget={widget}
              box={box}
              dragging={dragging}
              editing={canEdit}
              actions={{
                onMoveStart: begin(widget, 'move'),
                onResizeStart: begin(widget, 'resize'),
                onKeyDown: onKey(widget),
                onCustomize: () => actions.customize(widget.id),
                onDuplicate: () => actions.duplicate(widget.id),
                onRemove: () => actions.remove(widget.id),
                onSendToOverlay: actions.sendToOverlay
                  ? () => actions.sendToOverlay!(widget.id)
                  : undefined,
              }}
            />
          );
        })}
      </div>
    </div>
  );
}

function GridItem({
  widget,
  box,
  dragging,
  editing,
  actions,
}: {
  readonly widget: WidgetInstance;
  readonly box: { left: number; top: number; width: number; height: number };
  readonly dragging: boolean;
  readonly editing: boolean;
  readonly actions: Parameters<typeof WidgetCard>[0]['actions'];
}) {
  const [ref, visible] = useInViewport<HTMLDivElement>();
  return (
    <div
      ref={ref}
      role="listitem"
      className={`dashboard-grid__item${dragging ? ' dashboard-grid__item--dragging' : ''}`}
      style={{
        transform: `translate(${box.left}px, ${box.top}px)`,
        width: box.width,
        height: box.height,
      }}
    >
      <WidgetCard
        widget={widget}
        width={box.width}
        height={box.height}
        editing={editing}
        enabled={visible}
        actions={actions}
      />
    </div>
  );
}
