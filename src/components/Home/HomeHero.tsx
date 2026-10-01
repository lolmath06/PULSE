import { useMemo } from 'react';
import type { ReactNode } from 'react';
import { Link } from 'react-router-dom';
import { MODES } from '@/modes/modes';
import { modeStyle } from '@/design/appearance';
import { useAppearance } from '@/design/store';
import { resolveLook } from '@/design/look';
import { StyleScope } from '@/design/LookContext';
import { K, strip } from '@/presets/widgets';
import { useElementSize } from '@/visualization/useElementSize';
import { WidgetCard } from '@/components/Dashboard/WidgetCard';
import { Icon } from '@/components/Icon';

const ROUTES = {
  gaming: '/gaming',
  development: '/development',
  personal: '/personal',
  mini: '/mini',
} as const;

/**
 * The home: the brand, a live strip of the machine, the modes (each shown in
 * its own style), and the three places to go next.
 */
export function HomeHero({ children }: { readonly children: ReactNode }) {
  const appearance = useAppearance();
  const live = useMemo(
    () => ({
      ...strip(
        [K.cpu, K.gpu, K.ram, { ...K.cpuTemp, label: 'Temp' }, { ...K.down, label: 'Net ↓' }],
        [900, 64],
        true,
      ),
      id: 'w-home-live',
    }),
    [],
  );
  const [ref, size] = useElementSize<HTMLDivElement>();
  const width = Math.max(320, Math.floor(size.width || 820));

  return (
    <div className="home-hero">
      <div className="home-hero__brand">{children}</div>

      <div ref={ref} className="home-hero__live" aria-label="Live system">
        <WidgetCard widget={live} width={width} height={72} editing={false} />
      </div>

      <div className="home-hero__modes" role="list" aria-label="Modes">
        {MODES.map((mode) => {
          const active = appearance.activeMode === mode.id;
          const look = resolveLook(modeStyle(appearance, mode.id), appearance.custom);
          return (
            <StyleScope key={mode.id} look={look} className="home-mode">
              <Link to={ROUTES[mode.id]} className="home-mode__link" role="listitem">
                <span className="home-mode__icon" aria-hidden="true">
                  <Icon name={mode.icon} size={20} />
                </span>
                <span className="home-mode__name">
                  {mode.name}
                  {active && <span className="pill pill--tone-good">Active</span>}
                </span>
                <span className="home-mode__tagline">{mode.tagline}</span>
                <span className="home-mode__go" aria-hidden="true">
                  <Icon name="arrowRight" />
                </span>
              </Link>
            </StyleScope>
          );
        })}
      </div>

      <div className="home-hero__links">
        <Link to="/dashboard" className="home-link">
          <Icon name="dashboard" /> Dashboards <small>templates, widgets, layouts</small>
        </Link>
        <Link to="/overlays" className="home-link">
          <Icon name="overlays" /> Overlays <small>12 packs, backend status</small>
        </Link>
        <Link to="/appearance" className="home-link">
          <Icon name="appearance" /> Appearance <small>8 styles, deep tuning</small>
        </Link>
      </div>
    </div>
  );
}
