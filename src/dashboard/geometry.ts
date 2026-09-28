import type { WidgetInstance } from '@/dashboard/model';
import { GRID_COLUMNS } from '@/dashboard/model';

/** Height of a widget's title line, in px. */
export const TITLE_HEIGHT = 20;

/** Container widths at which the dashboard grid reflows. */
export function columnsFor(width: number): number {
  if (width >= 880) return GRID_COLUMNS;
  if (width >= 540) return 6;
  return 1;
}

/** The inner box a widget's content gets, after its frame. */
export function contentBox(widget: WidgetInstance, width: number, height: number) {
  const inset = widget.frame.padding * 2 + (widget.frame.border === 'thin' ? 2 : 0);
  const title = widget.frame.showTitle ? TITLE_HEIGHT : 0;
  return { width: Math.max(8, width - inset), height: Math.max(8, height - inset - title) };
}

export function widgetTitle(widget: WidgetInstance): string {
  return widget.title ?? widget.bindings.map((binding) => binding.label ?? binding.key).join(' · ');
}
