import type { GridRect, WidgetInstance } from '@/dashboard/model';
import { GRID_COLUMNS, KIND_LIMITS, clampRect } from '@/dashboard/model';

/**
 * The dashboard grid, as pure functions.
 *
 * Positions are **grid cells** (12 columns, rows of fixed height), never
 * pixels: the same layout is right on a 1080p and a 4K screen, and pixels are
 * computed only when drawing. After every change the layout is kept valid:
 * inside the grid, within each kind's limits, and without overlaps — a moved
 * widget pushes the ones it lands on downwards, then everything floats up.
 */

export function overlaps(a: GridRect, b: GridRect): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

function withRect(widget: WidgetInstance, layout: GridRect): WidgetInstance {
  return widget.layout === layout ? widget : { ...widget, layout };
}

/** Floats every widget up as far as it can go, keeping relative order. */
export function compact(widgets: readonly WidgetInstance[]): WidgetInstance[] {
  const order = [...widgets].sort((a, b) => a.layout.y - b.layout.y || a.layout.x - b.layout.x);
  const placed: WidgetInstance[] = [];
  for (const widget of order) {
    let y = widget.layout.y;
    while (
      y > 0 &&
      !placed.some((other) => overlaps({ ...widget.layout, y: y - 1 }, other.layout))
    ) {
      y -= 1;
    }
    placed.push(withRect(widget, { ...widget.layout, y }));
  }
  const byId = new Map(placed.map((widget) => [widget.id, widget]));
  return widgets.map((widget) => byId.get(widget.id) ?? widget);
}

/**
 * Removes every overlap, keeping `anchorId` (the widget the user just moved)
 * where it is and pushing the others down.
 */
export function resolveCollisions(
  widgets: readonly WidgetInstance[],
  anchorId?: string,
): WidgetInstance[] {
  const order = [...widgets].sort((a, b) => {
    if (a.id === anchorId) return -1;
    if (b.id === anchorId) return 1;
    return a.layout.y - b.layout.y || a.layout.x - b.layout.x;
  });
  const placed: WidgetInstance[] = [];
  for (const widget of order) {
    let rect = widget.layout;
    let guard = 0;
    while (placed.some((other) => overlaps(rect, other.layout)) && guard < 2_000) {
      const blocker = placed.find((other) => overlaps(rect, other.layout))!;
      rect = { ...rect, y: blocker.layout.y + blocker.layout.h };
      guard += 1;
    }
    placed.push(withRect(widget, rect));
  }
  const byId = new Map(placed.map((widget) => [widget.id, widget]));
  return compact(widgets.map((widget) => byId.get(widget.id) ?? widget));
}

/** Moves one widget to a cell, then repairs the layout. */
export function moveWidget(
  widgets: readonly WidgetInstance[],
  id: string,
  x: number,
  y: number,
): WidgetInstance[] {
  return resolveCollisions(
    widgets.map((widget) =>
      widget.id === id
        ? withRect(widget, clampRect({ ...widget.layout, x, y }, widget.kind))
        : widget,
    ),
    id,
  );
}

/** Resizes one widget within its kind's limits, then repairs the layout. */
export function resizeWidget(
  widgets: readonly WidgetInstance[],
  id: string,
  w: number,
  h: number,
): WidgetInstance[] {
  return resolveCollisions(
    widgets.map((widget) =>
      widget.id === id
        ? withRect(widget, clampRect({ ...widget.layout, w, h }, widget.kind))
        : widget,
    ),
    id,
  );
}

/** The first free rectangle of `w × h`, scanning row by row. */
export function firstFit(
  widgets: readonly WidgetInstance[],
  w: number,
  h: number,
  columns = GRID_COLUMNS,
): { x: number; y: number } {
  const width = Math.min(w, columns);
  for (let y = 0; y < 1_000; y += 1) {
    for (let x = 0; x + width <= columns; x += 1) {
      const candidate = { x, y, w: width, h };
      if (!widgets.some((widget) => overlaps(candidate, widget.layout))) return { x, y };
    }
  }
  return { x: 0, y: bottom(widgets) };
}

/** The first empty row below every widget. */
export function bottom(widgets: readonly WidgetInstance[]): number {
  return widgets.reduce((most, widget) => Math.max(most, widget.layout.y + widget.layout.h), 0);
}

/** Makes a stored layout valid: clamped and overlap-free. Idempotent. */
export function repairLayout(widgets: readonly WidgetInstance[]): WidgetInstance[] {
  return resolveCollisions(
    widgets.map((widget) => withRect(widget, clampRect(widget.layout, widget.kind))),
  );
}

/**
 * The layout drawn on a narrower screen. The stored 12-column layout is never
 * changed by this: resizing the window back restores it exactly.
 *
 * - 12 columns: as stored.
 * - 6 columns: every x and width halved, then repaired.
 * - 1 column: one widget per row, in reading order, full width.
 */
export function reflow(widgets: readonly WidgetInstance[], columns: number): WidgetInstance[] {
  if (columns >= GRID_COLUMNS) return [...widgets];
  if (columns <= 1) {
    let y = 0;
    const order = [...widgets].sort((a, b) => a.layout.y - b.layout.y || a.layout.x - b.layout.x);
    const stacked = order.map((widget) => {
      const h = Math.max(KIND_LIMITS[widget.kind].minH, widget.layout.h);
      const next = withRect(widget, { x: 0, y, w: 1, h });
      y += h;
      return next;
    });
    const byId = new Map(stacked.map((widget) => [widget.id, widget]));
    return widgets.map((widget) => byId.get(widget.id) ?? widget);
  }
  const ratio = columns / GRID_COLUMNS;
  const scaled = widgets.map((widget) => {
    const w = Math.max(1, Math.min(columns, Math.round(widget.layout.w * ratio)));
    const x = Math.min(columns - w, Math.round(widget.layout.x * ratio));
    return withRect(widget, { ...widget.layout, x, w });
  });
  return resolveCollisions(scaled);
}

/** Pixel geometry of a rectangle, for drawing. */
export function toPixels(
  rect: GridRect,
  columnWidth: number,
  rowHeight: number,
  gap: number,
): { left: number; top: number; width: number; height: number } {
  return {
    left: rect.x * (columnWidth + gap),
    top: rect.y * (rowHeight + gap),
    width: rect.w * columnWidth + (rect.w - 1) * gap,
    height: rect.h * rowHeight + (rect.h - 1) * gap,
  };
}
