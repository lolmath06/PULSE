import { createBrowserRouter } from 'react-router-dom';
import { AppLayout } from '@/layouts/AppLayout';
import { OverviewPage } from '@/pages/OverviewPage';
import { DashboardRoute } from '@/pages/DashboardRoute';
import { GamingPage } from '@/pages/GamingPage';
import { DevelopmentPage } from '@/pages/DevelopmentPage';
import { PersonalPage } from '@/pages/PersonalPage';
import { MiniPage } from '@/pages/MiniPage';
import { NotFoundPage } from '@/pages/NotFoundPage';

export const router = createBrowserRouter([
  {
    path: '/',
    element: <AppLayout />,
    children: [
      { index: true, element: <OverviewPage /> },
      { path: 'dashboard', element: <DashboardRoute /> },
      { path: 'gaming', element: <GamingPage /> },
      { path: 'development', element: <DevelopmentPage /> },
      { path: 'personal', element: <PersonalPage /> },
      { path: 'mini', element: <MiniPage /> },
      { path: '*', element: <NotFoundPage /> },
    ],
  },
]);
