import { Link, NavLink, Outlet, useLocation } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { NAV_GROUPS, NAV_ROUTES } from '@/app/routes';
import { StatusBar } from '@/components/StatusBar/StatusBar';
import { APP_VERSION } from '@/app/constants';
import { Icon } from '@/components/Icon';
import { RouteMemory } from '@/app/RouteMemory';
import { useLook } from '@/design/hooks';
import { useAppearance } from '@/design/store';

export function AppLayout() {
  const { t } = useTranslation();
  const location = useLocation();
  const look = useLook();
  const appearance = useAppearance();
  const userStyle = appearance.userStyles.find((style) => style.id === appearance.userStyleId);

  return (
    <div className="app-shell">
      <aside className="app-shell__sidebar">
        <div className="brand">
          <span className="brand__mark" aria-hidden="true">
            <svg viewBox="0 0 24 24" width="18" height="18">
              <path d="M2 12h4l2.5-6 4 12 3-9 2 3H22" />
            </svg>
          </span>
          <div>
            <p className="brand__name">PULSE</p>
            <p className="brand__version">{APP_VERSION}</p>
          </div>
        </div>

        <nav className="nav" aria-label={t('nav.main')}>
          {NAV_GROUPS.map((group) => (
            <div key={group.id} className="nav__group">
              <p className="nav__group-label">{t(`nav.groups.${group.id}`)}</p>
              {NAV_ROUTES.filter((route) => route.group === group.id).map((route) => (
                <NavLink
                  key={route.path}
                  to={route.path}
                  end={route.path === '/'}
                  title={t(`nav.routes.${route.id}.description`)}
                  className={({ isActive }) => `nav__item${isActive ? ' nav__item--active' : ''}`}
                >
                  <Icon name={route.icon} className="nav__icon" />
                  <span>{t(`nav.routes.${route.id}.label`)}</span>
                </NavLink>
              ))}
            </div>
          ))}
        </nav>

        <Link to="/appearance" className="sidebar__style" title={t('nav.changeStyle')}>
          <span className="sidebar__swatches" aria-hidden="true">
            {look.viz.slice(0, 3).map((color) => (
              <span key={color} style={{ background: color }} />
            ))}
          </span>
          <span className="sidebar__style-text">
            <span className="sidebar__style-label">{t('nav.style')}</span>
            <span className="sidebar__style-name">
              {userStyle?.name ?? t(`styles.${look.style.id}.name`)}
            </span>
          </span>
        </Link>
      </aside>

      <div className="app-shell__body">
        <main className="app-shell__content">
          <div className="page-enter" key={location.pathname}>
            <Outlet />
          </div>
        </main>
        <StatusBar />
      </div>
      <RouteMemory />
    </div>
  );
}
