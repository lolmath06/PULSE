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
  /** Stable id; its label and description are `nav.routes.<id>.*` translations. */
  readonly id: NavRouteId;
  readonly path: string;
  readonly mode: AppMode;
  readonly icon: IconName;
  /** The sidebar section it sits in. */
  readonly group: NavGroup;
}

export type NavGroup = 'monitor' | 'modes' | 'studio';

export type NavRouteId =
  | 'overview'
  | 'dashboard'
  | 'overlays'
  | 'gaming'
  | 'development'
  | 'personal'
  | 'mini'
  | 'appearance';

/** Sidebar sections, in order. Their names are `nav.groups.<id>` translations. */
export const NAV_GROUPS: readonly { readonly id: NavGroup }[] = [
  { id: 'monitor' },
  { id: 'modes' },
  { id: 'studio' },
];

export const NAV_ROUTES: readonly NavRoute[] = [
  {
    id: 'overview',
    path: '/',
    mode: 'standard',
    icon: 'overview',
    group: 'monitor',
  },
  {
    id: 'dashboard',
    path: '/dashboard',
    mode: 'standard',
    icon: 'dashboard',
    group: 'monitor',
  },
  {
    id: 'overlays',
    path: '/overlays',
    mode: 'mini',
    icon: 'overlays',
    group: 'monitor',
  },
  {
    id: 'gaming',
    path: '/gaming',
    mode: 'gaming',
    icon: 'gaming',
    group: 'modes',
  },
  {
    id: 'development',
    path: '/development',
    mode: 'development',
    icon: 'development',
    group: 'modes',
  },
  {
    id: 'personal',
    path: '/personal',
    mode: 'personal',
    icon: 'personal',
    group: 'modes',
  },
  {
    id: 'mini',
    path: '/mini',
    mode: 'mini',
    icon: 'mini',
    group: 'modes',
  },
  {
    id: 'appearance',
    path: '/appearance',
    mode: 'standard',
    icon: 'appearance',
    group: 'studio',
  },
] as const;
