import type { VisualizationConfig } from '@/visualization/config';
import type {
  Dashboard,
  DashboardsSection,
  GridRect,
  WidgetBinding,
  WidgetInstance,
  WidgetKind,
} from '@/dashboard/model';
import {
  DASHBOARDS_VERSION,
  MAX_DASHBOARDS,
  MAX_WIDGETS,
  normalizeWidget,
  uniqueWidgetIds,
} from '@/dashboard/model';
import { firstFit, moveWidget, repairLayout, resizeWidget } from '@/dashboard/layout';
import { configFor } from '@/dashboard/metricInfo';
import { createWidget, findBlueprint } from '@/dashboard/library';
import { isValidId, newId } from '@/dashboard/ids';
import type { StyleId } from '@/design/styles';
import { isStyleId } from '@/design/styles';

/**
 * The `dashboards` section: several named dashboards, one active.
 *
 * Everything here is a pure function from one section to the next — the UI
 * dispatches, tests assert — and every result is valid by construction.
 */

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** The visual defaults a stored widget falls back to: its first metric's. */
export function visualDefaultsFor(
  bindings: readonly WidgetBinding[],
  _kind: WidgetKind,
): VisualizationConfig {
  return configFor(bindings[0]?.key ?? 'cpu.usage.total');
}

function place(widget: WidgetInstance, rect: GridRect): WidgetInstance {
  return { ...widget, layout: rect };
}

/**
 * The default dashboard: a clean first screen, not everything at once.
 * CPU Total, memory, temperatures, GPU, network and storage, under a summary.
 */
export function defaultWidgets(): WidgetInstance[] {
  // Stable ids: the default is rebuilt whenever no section is stored yet, and
  // must name the same widgets every time.
  const make = (id: string, rect: GridRect) =>
    place({ ...createWidget(findBlueprint(id)!), id: `w-default-${id}` }, rect);
  return [
    make('summary', { x: 0, y: 0, w: 12, h: 2 }),
    make('cpu-total', { x: 0, y: 2, w: 6, h: 4 }),
    make('memory', { x: 6, y: 2, w: 6, h: 4 }),
    make('thermal', { x: 0, y: 6, w: 4, h: 4 }),
    make('gpu', { x: 4, y: 6, w: 4, h: 4 }),
    make('network', { x: 8, y: 6, w: 4, h: 4 }),
    make('storage', { x: 0, y: 10, w: 6, h: 4 }),
  ];
}

export function defaultDashboard(id = 'default', name = 'Default'): Dashboard {
  return { id, name, locked: true, styleId: null, origin: null, widgets: defaultWidgets() };
}

export function defaultSection(): DashboardsSection {
  return { version: DASHBOARDS_VERSION, activeId: 'default', items: [defaultDashboard()] };
}

export function normalizeDashboard(raw: unknown): Dashboard | null {
  if (!isRecord(raw)) return null;
  const widgets = (Array.isArray(raw.widgets) ? raw.widgets : [])
    .slice(0, MAX_WIDGETS)
    .map((widget) => normalizeWidget(widget, visualDefaultsFor))
    .filter((widget): widget is WidgetInstance => widget !== null);
  const name =
    typeof raw.name === 'string' && raw.name.trim() ? raw.name.trim().slice(0, 40) : 'Dashboard';
  return {
    id: isValidId(raw.id) ? raw.id : newId('d'),
    name,
    locked: raw.locked !== false,
    styleId: isStyleId(raw.styleId) ? raw.styleId : null,
    origin:
      isRecord(raw.origin) && isValidId(raw.origin.template)
        ? {
            template: raw.origin.template,
            version:
              typeof raw.origin.version === 'number' && Number.isInteger(raw.origin.version)
                ? Math.max(1, Math.min(1000, raw.origin.version))
                : 1,
          }
        : null,
    // Repaired on every load: a hand-edited or older file can never leave two
    // widgets on top of each other.
    widgets: repairLayout(uniqueWidgetIds(widgets)),
  };
}

/**
 * Reads the section. Anything unusable — missing, another version, no valid
 * dashboard — becomes the default dashboard, never an error.
 */
export function normalizeDashboards(raw: unknown): DashboardsSection {
  if (!isRecord(raw) || raw.version !== DASHBOARDS_VERSION || !Array.isArray(raw.items)) {
    return defaultSection();
  }
  const seen = new Set<string>();
  const items: Dashboard[] = [];
  for (const entry of raw.items.slice(0, MAX_DASHBOARDS)) {
    const dashboard = normalizeDashboard(entry);
    if (!dashboard) continue;
    const id = seen.has(dashboard.id) ? newId('d') : dashboard.id;
    seen.add(id);
    items.push({ ...dashboard, id });
  }
  if (items.length === 0) return defaultSection();
  const activeId = items.some((item) => item.id === raw.activeId)
    ? (raw.activeId as string)
    : items[0]!.id;
  return { version: DASHBOARDS_VERSION, activeId, items };
}

// --- actions ---------------------------------------------------------------

function mapDashboard(
  section: DashboardsSection,
  id: string,
  change: (dashboard: Dashboard) => Dashboard,
): DashboardsSection {
  return { ...section, items: section.items.map((item) => (item.id === id ? change(item) : item)) };
}

export function activeDashboard(section: DashboardsSection): Dashboard {
  return section.items.find((item) => item.id === section.activeId) ?? section.items[0]!;
}

export function setActive(section: DashboardsSection, id: string): DashboardsSection {
  return section.items.some((item) => item.id === id) ? { ...section, activeId: id } : section;
}

export function createDashboard(section: DashboardsSection, name: string): DashboardsSection {
  if (section.items.length >= MAX_DASHBOARDS) return section;
  const dashboard: Dashboard = {
    id: newId('d'),
    name: name.trim().slice(0, 40) || 'Dashboard',
    locked: false,
    styleId: null,
    origin: null,
    widgets: [],
  };
  return { ...section, items: [...section.items, dashboard], activeId: dashboard.id };
}

export function renameDashboard(
  section: DashboardsSection,
  id: string,
  name: string,
): DashboardsSection {
  const trimmed = name.trim().slice(0, 40);
  if (!trimmed) return section;
  return mapDashboard(section, id, (dashboard) => ({ ...dashboard, name: trimmed }));
}

/** A copy with new dashboard and widget ids — nothing shared with the original. */
export function duplicateDashboard(section: DashboardsSection, id: string): DashboardsSection {
  const source = section.items.find((item) => item.id === id);
  if (!source || section.items.length >= MAX_DASHBOARDS) return section;
  const copy: Dashboard = {
    ...source,
    id: newId('d'),
    name: `${source.name} copy`.slice(0, 40),
    widgets: source.widgets.map((widget) => ({ ...widget, id: newId('w') })),
  };
  return { ...section, items: [...section.items, copy], activeId: copy.id };
}

/** Deletes a dashboard. The last one cannot be deleted. */
export function deleteDashboard(section: DashboardsSection, id: string): DashboardsSection {
  if (section.items.length <= 1) return section;
  const items = section.items.filter((item) => item.id !== id);
  return { ...section, items, activeId: section.activeId === id ? items[0]!.id : section.activeId };
}

/** Restores the default widgets. Templates are untouched (another section). */
export function resetDashboard(section: DashboardsSection, id: string): DashboardsSection {
  return mapDashboard(section, id, (dashboard) => ({ ...dashboard, widgets: defaultWidgets() }));
}

/** The style a dashboard wears; `null` follows the app. */
export function setDashboardStyle(
  section: DashboardsSection,
  id: string,
  styleId: StyleId | null,
): DashboardsSection {
  return mapDashboard(section, id, (dashboard) => ({ ...dashboard, styleId }));
}

export function setLocked(
  section: DashboardsSection,
  id: string,
  locked: boolean,
): DashboardsSection {
  return mapDashboard(section, id, (dashboard) => ({ ...dashboard, locked }));
}

/** Adds a widget at the first free spot of its size. */
export function addWidget(
  section: DashboardsSection,
  id: string,
  widget: WidgetInstance,
): DashboardsSection {
  return mapDashboard(section, id, (dashboard) => {
    if (dashboard.widgets.length >= MAX_WIDGETS) return dashboard;
    const spot = firstFit(dashboard.widgets, widget.layout.w, widget.layout.h);
    return {
      ...dashboard,
      widgets: [...dashboard.widgets, place(widget, { ...widget.layout, ...spot })],
    };
  });
}

export function removeWidget(
  section: DashboardsSection,
  id: string,
  widgetId: string,
): DashboardsSection {
  return mapDashboard(section, id, (dashboard) => ({
    ...dashboard,
    widgets: repairLayout(dashboard.widgets.filter((widget) => widget.id !== widgetId)),
  }));
}

/** Copies metric, renderer, style and size; the copy gets a new id. */
export function duplicateWidget(
  section: DashboardsSection,
  id: string,
  widgetId: string,
): DashboardsSection {
  const dashboard = section.items.find((item) => item.id === id);
  const original = dashboard?.widgets.find((widget) => widget.id === widgetId);
  if (!original) return section;
  return addWidget(section, id, { ...original, id: newId('w') });
}

export function updateWidget(
  section: DashboardsSection,
  id: string,
  widgetId: string,
  change: (widget: WidgetInstance) => WidgetInstance,
): DashboardsSection {
  return mapDashboard(section, id, (dashboard) => ({
    ...dashboard,
    widgets: dashboard.widgets.map((widget) => (widget.id === widgetId ? change(widget) : widget)),
  }));
}

export function moveWidgetTo(
  section: DashboardsSection,
  id: string,
  widgetId: string,
  x: number,
  y: number,
): DashboardsSection {
  return mapDashboard(section, id, (dashboard) => ({
    ...dashboard,
    widgets: moveWidget(dashboard.widgets, widgetId, x, y),
  }));
}

export function resizeWidgetTo(
  section: DashboardsSection,
  id: string,
  widgetId: string,
  w: number,
  h: number,
): DashboardsSection {
  return mapDashboard(section, id, (dashboard) => ({
    ...dashboard,
    widgets: resizeWidget(dashboard.widgets, widgetId, w, h),
  }));
}

// --- import / export --------------------------------------------------------

export const EXPORT_FORMAT = 'pulse.dashboard';

/**
 * A dashboard as a portable JSON object. Bindings already hold persistable
 * references, so no MAC address, serial or user data is in it; a device
 * source from another machine simply resolves to *Source unavailable* there.
 */
export function exportDashboard(dashboard: Dashboard): Record<string, unknown> {
  return { format: EXPORT_FORMAT, version: DASHBOARDS_VERSION, dashboard };
}

/** Adds an exported dashboard as a new one, with fresh ids. */
export function importDashboard(
  section: DashboardsSection,
  raw: unknown,
): { section: DashboardsSection; error?: string } {
  if (!isRecord(raw) || raw.format !== EXPORT_FORMAT) {
    return { section, error: 'This is not a PULSE dashboard export.' };
  }
  if (raw.version !== DASHBOARDS_VERSION) {
    return { section, error: `Unsupported dashboard export version: ${String(raw.version)}.` };
  }
  const dashboard = normalizeDashboard(raw.dashboard);
  if (!dashboard) return { section, error: 'The export contains no valid dashboard.' };
  if (section.items.length >= MAX_DASHBOARDS) {
    return { section, error: `At most ${MAX_DASHBOARDS} dashboards.` };
  }
  const imported: Dashboard = {
    ...dashboard,
    id: newId('d'),
    widgets: dashboard.widgets.map((widget) => ({ ...widget, id: newId('w') })),
  };
  return { section: { ...section, items: [...section.items, imported], activeId: imported.id } };
}
