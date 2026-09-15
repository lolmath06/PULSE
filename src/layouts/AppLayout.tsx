import { NavLink, Outlet } from 'react-router-dom';
import { NAV_ROUTES } from '@/app/routes';
import { StatusBar } from '@/components/StatusBar/StatusBar';
import { APP_VERSION } from '@/app/constants';

export function AppLayout() {
  return (
    <div className="app-shell">
      <aside className="app-shell__sidebar">
        <div className="brand">
          <span className="brand__mark" aria-hidden="true" />
          <div>
            <p className="brand__name">PULSE</p>
            <p className="brand__version">{APP_VERSION}</p>
          </div>
        </div>

        <nav className="nav" aria-label="Main">
          {NAV_ROUTES.map((route) => (
            <NavLink
              key={route.path}
              to={route.path}
              end={route.path === '/'}
              className={({ isActive }) => `nav__item${isActive ? ' nav__item--active' : ''}`}
            >
              {route.label}
            </NavLink>
          ))}
        </nav>
      </aside>

      <div className="app-shell__body">
        <main className="app-shell__content">
          <Outlet />
        </main>
        <StatusBar />
      </div>
    </div>
  );
}
