import { useMemo } from 'react';
import type { MiniSettings } from '@/design/appearance';
import { activeDashboard } from '@/dashboard/dashboards';
import { useDashboards } from '@/dashboard/store';
import { ROW_HEIGHT } from '@/dashboard/model';
import { findMiniLayout } from '@/modes/miniLayouts';
import { useLook } from '@/design/hooks';
import { useElementSize } from '@/visualization/useElementSize';
import { WidgetCard } from '@/components/Dashboard/WidgetCard';

/**
 * What Mini shows, at the width it is given: one of Mini's own layouts, or a
 * dashboard stacked in one column. Used by the Mini window and its preview.
 */
export function MiniView({ settings }: { readonly settings: MiniSettings }) {
  const dashboards = useDashboards();
  const look = useLook();
  const [ref, size] = useElementSize<HTMLDivElement>();
  const width = Math.max(140, Math.floor(size.width || 320));
  const source = settings.source;
  const layoutRows = useMemo(
    () => (source.kind === 'layout' ? findMiniLayout(source.id).rows() : []),
    [source],
  );
  const dashboard =
    source.kind === 'dashboard'
      ? (dashboards.items.find((item) => item.id === source.id) ?? activeDashboard(dashboards))
      : null;
  const rows = dashboard
    ? [...dashboard.widgets]
        .sort((a, b) => a.layout.y - b.layout.y || a.layout.x - b.layout.x)
        .map((widget) => ({
          widget,
          height: Math.min(200, Math.max(44, widget.layout.h * ROW_HEIGHT)),
        }))
    : layoutRows;

  return (
    <div
      ref={ref}
      className="mini-view"
      style={{ gap: Math.max(4, Math.round(look.gridGap * 0.7)) }}
    >
      {rows.map(({ widget, height }) => (
        <WidgetCard key={widget.id} widget={widget} width={width} height={height} editing={false} />
      ))}
    </div>
  );
}
