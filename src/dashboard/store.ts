import { readSectionNormalized, useUiSection, writeSection } from '@/config/uiConfig';
import type { DashboardsSection } from '@/dashboard/model';
import { normalizeDashboards } from '@/dashboard/dashboards';
import type { TemplatesSection } from '@/dashboard/templates';
import { normalizeTemplates } from '@/dashboard/templates';

/** The dashboards, as every window sees them. */
export function useDashboards(): DashboardsSection {
  return useUiSection('dashboards', normalizeDashboards);
}

/** Applies a pure action to the current section and saves the result. */
export function updateDashboards(action: (section: DashboardsSection) => DashboardsSection) {
  const current = readSectionNormalized('dashboards', normalizeDashboards);
  const next = action(current);
  if (next !== current) writeSection('dashboards', next);
}

export function useTemplates(): TemplatesSection {
  return useUiSection('templates', normalizeTemplates);
}

export function updateTemplates(action: (section: TemplatesSection) => TemplatesSection) {
  const current = readSectionNormalized('templates', normalizeTemplates);
  const next = action(current);
  if (next !== current) writeSection('templates', next);
}
