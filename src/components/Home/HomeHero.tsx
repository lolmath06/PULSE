import { useMemo } from 'react';
import type { ReactNode } from 'react';
import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { MODES } from '@/modes/modes';
import { modeStyle } from '@/design/appearance';
import { useAppearance } from '@/design/store';
import { resolveLook } from '@/design/look';
import { StyleScope } from '@/design/LookContext';
import { K, strip, tx } from '@/presets/widgets';
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
  const { t } = useTranslation();
  const appearance = useAppearance();
  const live = useMemo(
    () => ({
      ...strip(
        [
          K.cpu,
          K.gpu,
          K.ram,
          { ...K.cpuTemp, label: tx('temp') },
          { ...K.down, label: tx('netDown') },
        ],
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

      <div ref={ref} className="home-hero__live" aria-label={t('home.liveSystem')}>
        <WidgetCard widget={live} width={width} height={72} editing={false} />
      </div>

      <div className="home-hero__modes" role="list" aria-label={t('nav.groups.modes')}>
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
                  {t(`modes.${mode.id}.name`)}
                  {active && <span className="pill pill--tone-good">{t('common.active')}</span>}
                </span>
                <span className="home-mode__tagline">{t(`modes.${mode.id}.tagline`)}</span>
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
          <Icon name="dashboard" /> {t('home.links.dashboards')}{' '}
          <small>{t('home.links.dashboardsHint')}</small>
        </Link>
        <Link to="/overlays" className="home-link">
          <Icon name="overlays" /> {t('home.links.overlays')}{' '}
          <small>{t('home.links.overlaysHint', { count: 12 })}</small>
        </Link>
        <Link to="/appearance" className="home-link">
          <Icon name="appearance" /> {t('home.links.appearance')}{' '}
          <small>{t('home.links.appearanceHint', { count: 8 })}</small>
        </Link>
      </div>
    </div>
  );
}
