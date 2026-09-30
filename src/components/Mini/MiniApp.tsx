import { useState } from 'react';
import { activeDashboard } from '@/dashboard/dashboards';
import { useDashboards } from '@/dashboard/store';
import { ROW_HEIGHT } from '@/dashboard/model';
import { useElementSize } from '@/visualization/useElementSize';
import { openMainWindow } from '@/overlay/desktop';
import { WidgetCard } from '@/components/Dashboard/WidgetCard';
import { RootLook } from '@/design/LookContext';
import { useAppLook } from '@/design/hooks';

/**
 * The Mini window: a small, **ordinary** PULSE window — decorated, focusable,
 * interactive, in the taskbar — showing one dashboard stacked in a single
 * column. Unlike an overlay it is never always-on-top and never click-through.
 */
export function MiniApp() {
  const look = useAppLook();
  return (
    <RootLook look={look}>
      <MiniContent />
    </RootLook>
  );
}

function MiniContent() {
  const section = useDashboards();
  const [chosen, setChosen] = useState<string | null>(null);
  const dashboard = section.items.find((item) => item.id === chosen) ?? activeDashboard(section);
  const [ref, size] = useElementSize<HTMLDivElement>();
  const width = Math.max(120, (size.width || 360) - 4);
  const widgets = [...dashboard.widgets].sort(
    (a, b) => a.layout.y - b.layout.y || a.layout.x - b.layout.x,
  );

  return (
    <div className="mini">
      <header className="mini__header">
        <select
          className="history-panel__select"
          aria-label="Dashboard shown in Mini"
          value={dashboard.id}
          onChange={(event) => setChosen(event.target.value)}
        >
          {section.items.map((item) => (
            <option key={item.id} value={item.id}>
              {item.name}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="button button--quiet"
          onClick={() => void openMainWindow().catch(() => undefined)}
        >
          Open PULSE
        </button>
      </header>
      <div ref={ref} className="mini__widgets">
        {widgets.map((widget) => (
          <WidgetCard
            key={widget.id}
            widget={widget}
            width={width}
            height={Math.min(200, Math.max(48, widget.layout.h * ROW_HEIGHT))}
            editing={false}
          />
        ))}
      </div>
    </div>
  );
}
