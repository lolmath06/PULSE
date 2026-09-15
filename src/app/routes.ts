import type { AppMode } from '@/types/mode';

/**
 * Navigation entries of the PULSE shell.
 *
 * `mode` links a route to a behavioural mode (see docs/architecture/overview.md).
 * Routes and modes are intentionally separate concepts: a mode describes *how*
 * PULSE behaves (window shape, refresh cadence, visibility), a dashboard
 * describes *what* is displayed. Phase 0 only wires the navigation.
 */
export interface NavRoute {
  readonly path: string;
  readonly label: string;
  readonly mode: AppMode;
  readonly description: string;
}

export const NAV_ROUTES: readonly NavRoute[] = [
  {
    path: '/',
    label: 'Overview',
    mode: 'standard',
    description: 'The default PULSE dashboard.',
  },
  {
    path: '/gaming',
    label: 'Gaming',
    mode: 'gaming',
    description: 'A low-overhead view focused on in-game relevant metrics.',
  },
  {
    path: '/development',
    label: 'Development',
    mode: 'development',
    description: 'A view focused on build, compile and workload pressure.',
  },
  {
    path: '/personal',
    label: 'Personal',
    mode: 'standard',
    description: 'A freely composable dashboard with many widgets.',
  },
  {
    path: '/mini',
    label: 'Mini',
    mode: 'mini',
    description: 'Configuration surface for the permanent desktop overlay.',
  },
] as const;
