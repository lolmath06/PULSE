import { useEffect, useRef } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { NAV_ROUTES } from '@/app/routes';
import { setLastRoute } from '@/design/appearance';
import { readAppearance, updateAppearance } from '@/design/store';

/**
 * The main window reopens on the page it was left on: the route is saved as
 * it changes, and restored once when the window starts on the Overview.
 */
export function RouteMemory() {
  const location = useLocation();
  const navigate = useNavigate();
  const restored = useRef(false);

  useEffect(() => {
    if (restored.current) return;
    restored.current = true;
    const last = readAppearance().lastRoute;
    const known = NAV_ROUTES.some((route) => route.path === last);
    if (location.pathname === '/' && last && last !== '/' && known) {
      void navigate(last, { replace: true });
    }
  }, [location.pathname, navigate]);

  useEffect(() => {
    if (!restored.current) return;
    updateAppearance((section) => setLastRoute(section, location.pathname));
  }, [location.pathname]);

  return null;
}
