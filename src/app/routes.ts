import type { AppMode } from '@/types/mode';
import type { IconName } from '@/components/Icon';

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
  readonly icon: IconName;
  /** The sidebar section it sits in. */
  readonly group: NavGroup;
}

export type NavGroup = 'monitor' | 'modes' | 'studio';

export const NAV_GROUPS: readonly { readonly id: NavGroup; readonly label: string }[] = [
  { id: 'monitor', label: 'Monitor' },
  { id: 'modes', label: 'Modes' },
  { id: 'studio', label: 'Studio' },
];

export const NAV_ROUTES: readonly NavRoute[] = [
  {
    path: '/',
    label: 'Overview',
    mode: 'standard',
    description: 'The default PULSE dashboard.',
    icon: 'overview',
    group: 'monitor',
  },
  {
    path: '/dashboard',
    label: 'Dashboard',
    mode: 'standard',
    description: 'Your own dashboards: widgets you add, arrange and style.',
    icon: 'dashboard',
    group: 'monitor',
  },
  {
    path: '/overlays',
    label: 'Overlays',
    mode: 'mini',
    description: 'Widgets in their own windows on the desktop.',
    icon: 'overlays',
    group: 'monitor',
  },
  {
    path: '/gaming',
    label: 'Gaming',
    mode: 'gaming',
    description: 'A low-overhead view focused on in-game relevant metrics.',
    icon: 'gaming',
    group: 'modes',
  },
  {
    path: '/development',
    label: 'Development',
    mode: 'development',
    description: 'A view focused on build, compile and workload pressure.',
    icon: 'development',
    group: 'modes',
  },
  {
    path: '/personal',
    label: 'Personal',
    mode: 'personal',
    description: 'A freely composable dashboard with many widgets.',
    icon: 'personal',
    group: 'modes',
  },
  {
    path: '/mini',
    label: 'Mini',
    mode: 'mini',
    description: 'A small, ordinary PULSE window showing one dashboard.',
    icon: 'mini',
    group: 'modes',
  },
  {
    path: '/appearance',
    label: 'Appearance',
    mode: 'standard',
    description: 'Styles, colours, type, density and motion — for every surface.',
    icon: 'appearance',
    group: 'studio',
  },
] as const;
