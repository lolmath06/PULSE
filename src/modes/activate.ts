import type { ModeDefinition } from '@/modes/modes';
import { enterMode } from '@/design/appearance';
import { withTransition } from '@/design/hooks';
import { readAppearance, updateAppearance } from '@/design/store';
import { setActive } from '@/dashboard/dashboards';
import { updateDashboards } from '@/dashboard/store';
import { overlayAction } from '@/overlay/desktop';
import { setCloseBehavior } from '@/overlay/settings';

/** Enters `mode`: its style, its dashboard, and what it asks for on entry. */
export function activateMode(mode: ModeDefinition) {
  // Read now: the style change itself may run later, inside a View Transition.
  const settings = readAppearance().modes[mode.id];
  withTransition(() => updateAppearance((section) => enterMode(section, mode.id)));
  if (settings.dashboardId) {
    const id = settings.dashboardId;
    updateDashboards((section) => setActive(section, id));
  }
  if (settings.lockOverlays) void overlayAction('lockAll').catch(() => undefined);
  if (settings.keepRunning) setCloseBehavior('keep-running');
}
