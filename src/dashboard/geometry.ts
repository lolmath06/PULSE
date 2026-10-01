import type { WidgetBinding, WidgetInstance } from '@/dashboard/model';
import { GRID_COLUMNS } from '@/dashboard/model';
import { storedText } from '@/i18n/text';

/** Height of a widget's title line, in px. */
export const TITLE_HEIGHT = 20;

/** Container widths at which the dashboard grid reflows. */
export function columnsFor(width: number): number {
  if (width >= 880) return GRID_COLUMNS;
  if (width >= 540) return 6;
  return 1;
}

/**
 * How a widget's frame uses its box. Part of the same density policy as the
 * visualizations (`src/visualization/presentation.ts`): at small sizes the
 * title goes first, padding shrinks, and the edit tools collapse into a
 * handle and a "…" menu, so the metric itself keeps the space.
 */
export interface FrameLayout {
  readonly padding: number;
  readonly showTitle: boolean;
  readonly compactTools: boolean;
  readonly content: { readonly width: number; readonly height: number };
}

export function frameLayout(widget: WidgetInstance, width: number, height: number): FrameLayout {
  const small = width < 140 || height < 70;
  const padding = small ? Math.min(widget.frame.padding, 3) : widget.frame.padding;
  const showTitle = widget.frame.showTitle && height >= 72 && width >= 100;
  const inset = padding * 2 + (widget.frame.border === 'thin' ? 2 : 0);
  return {
    padding,
    showTitle,
    compactTools: width < 300 || height < 90,
    content: {
      width: Math.max(8, width - inset),
      height: Math.max(8, height - inset - (showTitle ? TITLE_HEIGHT : 0)),
    },
  };
}

/** The inner box a widget's content gets, after its frame. */
export function contentBox(widget: WidgetInstance, width: number, height: number) {
  return frameLayout(widget, width, height).content;
}

/** A widget's own title as shown — a built-in one translated — or `null`. */
export function widgetTitleText(widget: Pick<WidgetInstance, 'title' | 'titleKey'>): string | null {
  return storedText(widget.titleKey, widget.title);
}

/** A binding's own label as shown — a built-in one translated — or `null`. */
export function bindingLabelText(
  binding: Pick<WidgetBinding, 'label' | 'labelKey'>,
): string | null {
  return storedText(binding.labelKey, binding.label);
}

export function widgetTitle(widget: WidgetInstance): string {
  return (
    widgetTitleText(widget) ??
    widget.bindings.map((binding) => bindingLabelText(binding) ?? binding.key).join(' · ')
  );
}
