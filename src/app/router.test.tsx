import { describe, expect, it } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { AppLayout } from '@/layouts/AppLayout';
import { OverviewPage } from '@/pages/OverviewPage';
import { GamingPage } from '@/pages/GamingPage';
import { MiniPage } from '@/pages/MiniPage';
import { NAV_ROUTES } from '@/app/routes';

/**
 * Renders the shell at `path` and waits for the backend probe to settle.
 *
 * Tests run outside the Tauri runtime, so `get_platform_info` always rejects;
 * awaiting that rejection keeps the assertions free of act() races.
 */
async function renderAt(path: string) {
  const router = createMemoryRouter(
    [
      {
        path: '/',
        element: <AppLayout />,
        children: [
          { index: true, element: <OverviewPage /> },
          { path: 'gaming', element: <GamingPage /> },
          { path: 'mini', element: <MiniPage /> },
        ],
      },
    ],
    { initialEntries: [path] },
  );

  const result = render(<RouterProvider router={router} />);

  await waitFor(() =>
    expect(screen.getByLabelText('Backend status')).toHaveTextContent(/Backend unavailable/),
  );

  return result;
}

describe('PULSE shell', () => {
  it('exposes every mode as a navigation entry', async () => {
    await renderAt('/');

    const nav = screen.getByRole('navigation', { name: 'Main' });
    for (const route of NAV_ROUTES) {
      expect(nav).toHaveTextContent(route.label);
    }
  });

  it('renders the Overview hero on the index route', async () => {
    await renderAt('/');

    expect(screen.getByRole('heading', { level: 1, name: 'PULSE' })).toBeInTheDocument();
    expect(screen.getByText('Your system, at a glance.')).toBeInTheDocument();
  });

  it('renders the matching page for a nested route', async () => {
    await renderAt('/gaming');

    expect(screen.getByRole('heading', { level: 1, name: 'Gaming' })).toBeInTheDocument();
    expect(screen.getByText('Coming in a future PULSE phase.')).toBeInTheDocument();
  });

  it('degrades gracefully when the Tauri backend is unreachable', async () => {
    await renderAt('/mini');

    // Outside the Tauri runtime there is no backend; the shell must still
    // render its content instead of throwing.
    expect(screen.getByRole('heading', { level: 1, name: 'Mini' })).toBeInTheDocument();
  });
});
