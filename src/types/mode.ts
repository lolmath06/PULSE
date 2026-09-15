/**
 * Behavioural modes of PULSE.
 *
 * A mode changes *how* PULSE presents itself (window chrome, update cadence,
 * interaction model). It is not the same thing as a dashboard, which describes
 * *what* is displayed. See docs/architecture/overview.md.
 */
export type AppMode = 'standard' | 'mini' | 'gaming' | 'development';

export const APP_MODES: readonly AppMode[] = ['standard', 'mini', 'gaming', 'development'] as const;
