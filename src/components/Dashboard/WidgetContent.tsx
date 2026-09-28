import { MetricVisualization } from '@/visualization/MetricVisualization';
import type { WidgetInstance } from '@/dashboard/model';
import { useWidgetData } from '@/dashboard/widgetData';
import { GroupView } from '@/components/Dashboard/GroupView';
import { useResolvedBindings } from '@/components/Dashboard/useWidget';

/**
 * A widget's content in a box of exactly `width × height` pixels.
 *
 * The same component draws a widget on the dashboard grid, in the Mini window
 * and in a desktop overlay — the Phase 10 engine does the drawing, whatever
 * the size, from a 60×20 value to a full-width chart.
 */
export function WidgetContent({
  widget,
  width,
  height,
  enabled = true,
}: {
  readonly widget: WidgetInstance;
  readonly width: number;
  readonly height: number;
  /** False while off screen: no subscription, no query. */
  readonly enabled?: boolean;
}) {
  const resolved = useResolvedBindings(widget);
  const { data, meta, live } = useWidgetData(widget, resolved, enabled);
  const w = Math.max(8, Math.floor(width));
  const h = Math.max(8, Math.floor(height));

  if (widget.kind === 'group' || widget.kind === 'summary') {
    return (
      <GroupView
        resolved={resolved}
        live={live}
        config={widget.visual.config}
        orientation={widget.kind === 'summary' ? 'inline' : widget.group.orientation}
        sparklines={widget.group.sparklines}
        width={w}
        height={h}
      />
    );
  }

  return (
    <MetricVisualization
      data={data}
      meta={meta}
      config={widget.visual.config}
      width={w}
      height={h}
    />
  );
}
