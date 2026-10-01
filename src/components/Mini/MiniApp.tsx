import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { dashboardName } from '@/dashboard/dashboards';
import { openMainWindow } from '@/overlay/desktop';
import { MINI_LAYOUT_IDS, modeStyle, setMini } from '@/design/appearance';
import type { MiniLayoutId } from '@/design/appearance';
import { resolveLook } from '@/design/look';
import { RootLook } from '@/design/LookContext';
import { updateAppearance, useAppearance } from '@/design/store';
import { useDashboards } from '@/dashboard/store';
import { findMiniLayout } from '@/modes/miniLayouts';
import { MiniView } from '@/components/Modes/MiniView';
import { Icon } from '@/components/Icon';

/**
 * The Mini window: a small, **ordinary** PULSE window — decorated, focusable,
 * interactive, in the taskbar — showing one of Mini's own layouts or one of
 * your dashboards. Unlike an overlay it is never always-on-top and never
 * click-through. It wears the Mini mode's style unless given its own.
 */
export function MiniApp() {
  const appearance = useAppearance();
  const look = useMemo(
    () => resolveLook(appearance.mini.styleId ?? modeStyle(appearance, 'mini'), appearance.custom),
    [appearance],
  );
  return (
    <RootLook look={look}>
      <MiniContent />
    </RootLook>
  );
}

function MiniContent() {
  const { t } = useTranslation();
  const appearance = useAppearance();
  const dashboards = useDashboards();
  const source = appearance.mini.source;
  const value = source.kind === 'layout' ? `layout:${source.id}` : `dashboard:${source.id}`;

  return (
    <div className="mini">
      <header className="mini__header">
        <span className="mini__brand" aria-hidden="true" />
        <select
          className="history-panel__select mini__select"
          aria-label={t('mini.shownIn')}
          value={value}
          onChange={(event) => {
            const [kind, id] = event.target.value.split(':') as [string, string];
            updateAppearance((section) =>
              setMini(section, {
                source:
                  kind === 'layout'
                    ? { kind: 'layout', id: id as MiniLayoutId }
                    : { kind: 'dashboard', id },
              }),
            );
          }}
        >
          <optgroup label={t('modes.miniPage.layouts')}>
            {MINI_LAYOUT_IDS.map((id) => (
              <option key={id} value={`layout:${id}`}>
                {t(`modes.miniLayouts.${findMiniLayout(id).id}.name`)}
              </option>
            ))}
          </optgroup>
          <optgroup label={t('home.links.dashboards')}>
            {dashboards.items.map((item) => (
              <option key={item.id} value={`dashboard:${item.id}`}>
                {dashboardName(item)}
              </option>
            ))}
          </optgroup>
        </select>
        <button
          type="button"
          className="button button--quiet mini__open"
          title={t('overlays.surface.openPulse')}
          aria-label={t('overlays.surface.openPulse')}
          onClick={() => void openMainWindow().catch(() => undefined)}
        >
          <Icon name="arrowRight" />
        </button>
      </header>
      <MiniView settings={appearance.mini} />
    </div>
  );
}
