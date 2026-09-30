import { createBrowserRouter } from 'react-router-dom';
import { AppLayout } from '@/layouts/AppLayout';
import { OverviewPage } from '@/pages/OverviewPage';
import { DashboardRoute } from '@/pages/DashboardRoute';
import { OverlaysRoute } from '@/pages/OverlaysRoute';
import { GamingPage } from '@/pages/GamingPage';
import { DevelopmentPage } from '@/pages/DevelopmentPage';
import { PersonalPage } from '@/pages/PersonalPage';
import { MiniPage } from '@/pages/MiniPage';
import { NotFoundPage } from '@/pages/NotFoundPage';
import { AppearancePage } from '@/components/Appearance/AppearancePage';

export const router = createBrowserRouter([
  {
    path: '/',
    element: <AppLayout />,
    children: [
      { index: true, element: <OverviewPage /> },
      { path: 'dashboard', element: <DashboardRoute /> },
      { path: 'overlays', element: <OverlaysRoute /> },
      { path: 'gaming', element: <GamingPage /> },
      { path: 'development', element: <DevelopmentPage /> },
      { path: 'personal', element: <PersonalPage /> },
      { path: 'mini', element: <MiniPage /> },
      { path: 'appearance', element: <AppearancePage /> },
      { path: '*', element: <NotFoundPage /> },
    ],
  },
]);
