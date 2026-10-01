import type { Dashboard, DashboardsSection, WidgetInstance } from '@/dashboard/model';
import { MAX_DASHBOARDS } from '@/dashboard/model';
import { defaultWidgets } from '@/dashboard/dashboards';
import { repairLayout } from '@/dashboard/layout';
import { newId } from '@/dashboard/ids';
import type { StyleId } from '@/design/styles';
import type { ModeId } from '@/modes/modes';
import type { Text } from '@/i18n/text';
import { K, tx, widget } from '@/presets/widgets';
import { englishText } from '@/i18n/i18n';

/**
 * PULSE's built-in dashboard templates. Each is a composed layout on the
 * 12-column grid, with a style that suits it; a dashboard made from one
 * remembers its origin, so *Reset* returns to the template.
 */

export const TEMPLATES_VERSION = 1;

/**
 * A built-in template. Its name and description are translations keyed by id
 * (`presets.templates.<id>.name` / `.description`).
 */
export interface DashboardTemplate {
  readonly id: string;
  /** `null`: follows the app's style. */
  readonly styleId: StyleId | null;
  readonly modes: readonly ModeId[];
  /** Marks the showcase. */
  readonly featured?: boolean;
  readonly widgets: () => WidgetInstance[];
}

const area = (
  metrics: Parameters<typeof widget>[0]['metrics'],
  rect: [number, number, number, number],
  title?: Text,
  thermal = false,
) => widget({ metrics, rect, title, thermal, style: { renderer: 'area' } });
const line = (
  metrics: Parameters<typeof widget>[0]['metrics'],
  rect: [number, number, number, number],
  title?: Text,
  thermal = false,
) => widget({ metrics, rect, title, thermal, style: { renderer: 'line' } });
const gauge = (
  metric: Parameters<typeof widget>[0]['metrics'][number],
  rect: [number, number, number, number],
  title?: Text,
  thermal = false,
) =>
  widget({
    metrics: [metric],
    rect,
    title,
    thermal,
    style: {
      renderer: 'gauge',
      gauge: { thickness: 0.12, arc: 'three-quarter' },
      // A ring needs bounds; a temperature has none of its own. 20–100 °C is
      // a visual range, not a verdict about the hardware.
      ...(thermal ? { scale: { mode: 'fixed' as const, min: 20, max: 100 } } : {}),
    },
  });
const tile = (
  metric: Parameters<typeof widget>[0]['metrics'][number],
  rect: [number, number, number, number],
  thermal = false,
) => widget({ metrics: [metric], kind: 'value', rect, thermal, titled: false });
const bar = (
  metric: Parameters<typeof widget>[0]['metrics'][number],
  rect: [number, number, number, number],
  title?: Text,
) => widget({ metrics: [metric], rect, title, style: { renderer: 'bar' } });
const summary = (
  metrics: Parameters<typeof widget>[0]['metrics'],
  rect: [number, number, number, number],
) =>
  widget({
    metrics,
    kind: 'summary',
    title: tx('system'),
    rect,
    titled: false,
    group: { orientation: 'inline', sparklines: true },
  });
const group = (
  title: Text,
  metrics: Parameters<typeof widget>[0]['metrics'],
  rect: [number, number, number, number],
) =>
  widget({ metrics, kind: 'group', title, rect, group: { orientation: 'rows', sparklines: true } });

export const DASHBOARD_TEMPLATES: readonly DashboardTemplate[] = [
  {
    id: 'balanced',
    styleId: null,
    modes: ['personal'],
    widgets: () => defaultWidgets(),
  },
  {
    id: 'gaming',
    styleId: 'gaming',
    modes: ['gaming'],
    widgets: () => [
      summary(
        [K.cpu, K.gpu, K.ram, { ...K.cpuTemp, label: 'CPU °C' }, { ...K.gpuTemp, label: 'GPU °C' }],
        [0, 0, 12, 2],
      ),
      gauge(K.cpu, [0, 2, 3, 4], 'CPU'),
      gauge(K.gpu, [3, 2, 3, 4], 'GPU'),
      line([K.cpuTemp, K.gpuTemp], [6, 2, 6, 4], tx('temperatures'), true),
      area([K.ram], [0, 6, 4, 4], tx('memory')),
      group(
        tx('videoMemory'),
        [K.vram, { ...K.gpuTemp, label: tx('gpuTemp') }, { ...K.hotspot, label: tx('hotspot') }],
        [4, 6, 4, 4],
      ),
      area(
        [
          { ...K.down, label: tx('download') },
          { ...K.up, label: tx('upload') },
        ],
        [8, 6, 4, 4],
        tx('network'),
      ),
    ],
  },
  {
    id: 'development',
    styleId: 'technical',
    modes: ['development'],
    widgets: () => [
      summary(
        [
          K.cpu,
          K.ram,
          { ...K.read, label: tx('read') },
          { ...K.write, label: tx('write') },
          { ...K.down, label: tx('netDown') },
        ],
        [0, 0, 12, 2],
      ),
      area([K.cpu], [0, 2, 8, 4], tx('cpuTotal')),
      group(tx('processes'), [K.procs, K.running, K.threads], [8, 2, 4, 4]),
      area([K.ram], [0, 6, 4, 4], tx('memory')),
      area([K.read, K.write], [4, 6, 4, 4], tx('diskIo')),
      area(
        [
          { ...K.down, label: tx('download') },
          { ...K.up, label: tx('upload') },
        ],
        [8, 6, 4, 4],
        tx('network'),
      ),
      line([K.cpuTemp], [0, 10, 6, 3], tx('cpuTemperature'), true),
      bar(K.disk, [6, 10, 6, 3], tx('filesystem')),
    ],
  },
  {
    id: 'personal',
    styleId: 'glass',
    modes: ['personal'],
    widgets: () => [
      summary([K.cpu, K.gpu, K.ram, { ...K.down, label: tx('netDown') }], [0, 0, 12, 2]),
      area([K.cpu], [0, 2, 6, 4], tx('processor')),
      gauge(K.ram, [6, 2, 3, 4], tx('memory')),
      gauge({ ...K.cpuTemp, label: tx('temp') }, [9, 2, 3, 4], tx('temperature'), true),
      area(
        [
          { ...K.down, label: tx('download') },
          { ...K.up, label: tx('upload') },
        ],
        [0, 6, 8, 4],
        tx('network'),
      ),
      group(tx('storage'), [K.disk, { ...K.ssdTemp, label: tx('ssdTemp') }], [8, 6, 4, 4]),
    ],
  },
  {
    id: 'thermal',
    styleId: 'neon',
    modes: ['gaming'],
    widgets: () => [
      line([K.cpuTemp, K.gpuTemp, K.hotspot], [0, 0, 8, 5], tx('temperatures'), true),
      gauge({ ...K.cpuTemp, label: 'CPU' }, [8, 0, 4, 5], tx('cpuPackage'), true),
      tile({ ...K.cpuTemp, label: 'CPU' }, [0, 5, 3, 2], true),
      tile({ ...K.gpuTemp, label: 'GPU' }, [3, 5, 3, 2], true),
      tile({ ...K.hotspot, label: tx('hotspot') }, [6, 5, 3, 2], true),
      tile({ ...K.ssdTemp, label: 'SSD' }, [9, 5, 3, 2], true),
      area([K.cpu, K.gpu], [0, 7, 12, 4], tx('load')),
    ],
  },
  {
    id: 'network-io',
    styleId: 'technical',
    modes: ['development'],
    widgets: () => [
      area(
        [
          { ...K.down, label: tx('download') },
          { ...K.up, label: tx('upload') },
        ],
        [0, 0, 6, 5],
        tx('network'),
      ),
      area([K.read, K.write], [6, 0, 6, 5], tx('diskIo')),
      tile({ ...K.down, label: '↓' }, [0, 5, 3, 2]),
      tile({ ...K.up, label: '↑' }, [3, 5, 3, 2]),
      tile(K.wifi, [6, 5, 3, 2]),
      tile(K.ssdTemp, [9, 5, 3, 2], true),
      bar(K.disk, [0, 7, 12, 2], tx('filesystem')),
    ],
  },
  {
    id: 'minimal',
    styleId: 'stealth',
    modes: ['mini'],
    widgets: () => [
      tile(K.cpu, [0, 0, 3, 3]),
      tile(K.ram, [3, 0, 3, 3]),
      tile({ ...K.cpuTemp, label: tx('temp') }, [6, 0, 3, 3]),
      tile({ ...K.down, label: '↓' }, [9, 0, 3, 3]),
      area([K.cpu, K.ram], [0, 3, 12, 5], tx('load')),
    ],
  },
  {
    id: 'showcase',
    styleId: 'glass',
    modes: ['personal'],
    featured: true,
    widgets: () => [
      summary(
        [
          K.cpu,
          K.gpu,
          K.ram,
          { ...K.cpuTemp, label: tx('temp') },
          { ...K.down, label: tx('netDown') },
        ],
        [0, 0, 12, 2],
      ),
      area([K.cpu], [0, 2, 7, 5], tx('processor')),
      gauge(K.ram, [7, 2, 5, 5], tx('memory')),
      gauge(K.gpu, [0, 7, 3, 4], tx('graphics')),
      gauge({ ...K.cpuTemp, label: tx('temp') }, [3, 7, 3, 4], tx('heat'), true),
      area(
        [
          { ...K.down, label: tx('download') },
          { ...K.up, label: tx('upload') },
        ],
        [6, 7, 6, 4],
        tx('network'),
      ),
      tile(K.vram, [0, 11, 4, 2]),
      tile(K.procs, [4, 11, 4, 2]),
      tile(K.ssdTemp, [8, 11, 4, 2], true),
    ],
  },
];

export function findTemplate(id: unknown): DashboardTemplate | undefined {
  return DASHBOARD_TEMPLATES.find((template) => template.id === id);
}

/** A template's widgets, fresh ids, repaired so nothing overlaps. */
export function templateWidgets(template: DashboardTemplate): WidgetInstance[] {
  return repairLayout(template.widgets().map((w) => ({ ...w, id: newId('w') })));
}

export function templateNameKey(id: string): string {
  return `presets.templates.${id}.name`;
}

/**
 * Adds a dashboard made from `template` and shows it. Without a name of the
 * user's, it is named after the template — by key, so the name follows the
 * interface language until the user renames it.
 */
export function createDashboardFromTemplate(
  section: DashboardsSection,
  template: DashboardTemplate,
  name?: Text,
): { section: DashboardsSection; id: string | null } {
  if (section.items.length >= MAX_DASHBOARDS) return { section, id: null };
  const own = typeof name === 'string' ? name.trim().slice(0, 40) : '';
  const builtIn = typeof name === 'object' ? name.key : templateNameKey(template.id);
  const dashboard: Dashboard = {
    id: newId('d'),
    ...(own ? { name: own } : { name: englishText(builtIn), nameKey: builtIn }),
    locked: true,
    styleId: template.styleId,
    origin: { template: template.id, version: TEMPLATES_VERSION },
    widgets: templateWidgets(template),
  };
  return {
    section: { ...section, items: [...section.items, dashboard], activeId: dashboard.id },
    id: dashboard.id,
  };
}

/**
 * Resets a dashboard: to its template's widgets and style when it came from
 * one, otherwise to the default widgets. Templates saved by the user and
 * overlays are untouched.
 */
export function resetDashboardToOrigin(section: DashboardsSection, id: string): DashboardsSection {
  return {
    ...section,
    items: section.items.map((dashboard) => {
      if (dashboard.id !== id) return dashboard;
      const template = findTemplate(dashboard.origin?.template);
      return template
        ? {
            ...dashboard,
            styleId: template.styleId,
            origin: { template: template.id, version: TEMPLATES_VERSION },
            widgets: templateWidgets(template),
          }
        : { ...dashboard, widgets: repairLayout(defaultWidgets()) };
    }),
  };
}
