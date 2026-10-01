import type { Dashboard, DashboardsSection, WidgetInstance } from '@/dashboard/model';
import { MAX_DASHBOARDS } from '@/dashboard/model';
import { defaultWidgets } from '@/dashboard/dashboards';
import { repairLayout } from '@/dashboard/layout';
import { newId } from '@/dashboard/ids';
import type { StyleId } from '@/design/styles';
import type { ModeId } from '@/modes/modes';
import { K, widget } from '@/presets/widgets';

/**
 * PULSE's built-in dashboard templates. Each is a composed layout on the
 * 12-column grid, with a style that suits it; a dashboard made from one
 * remembers its origin, so *Reset* returns to the template.
 */

export const TEMPLATES_VERSION = 1;

export interface DashboardTemplate {
  readonly id: string;
  readonly name: string;
  readonly description: string;
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
  title?: string,
  thermal = false,
) => widget({ metrics, rect, title, thermal, style: { renderer: 'area' } });
const line = (
  metrics: Parameters<typeof widget>[0]['metrics'],
  rect: [number, number, number, number],
  title?: string,
  thermal = false,
) => widget({ metrics, rect, title, thermal, style: { renderer: 'line' } });
const gauge = (
  metric: Parameters<typeof widget>[0]['metrics'][number],
  rect: [number, number, number, number],
  title?: string,
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
  title?: string,
) => widget({ metrics: [metric], rect, title, style: { renderer: 'bar' } });
const summary = (
  metrics: Parameters<typeof widget>[0]['metrics'],
  rect: [number, number, number, number],
) =>
  widget({
    metrics,
    kind: 'summary',
    title: 'System',
    rect,
    titled: false,
    group: { orientation: 'inline', sparklines: true },
  });
const group = (
  title: string,
  metrics: Parameters<typeof widget>[0]['metrics'],
  rect: [number, number, number, number],
) =>
  widget({ metrics, kind: 'group', title, rect, group: { orientation: 'rows', sparklines: true } });

export const DASHBOARD_TEMPLATES: readonly DashboardTemplate[] = [
  {
    id: 'balanced',
    name: 'Default balanced',
    description:
      'The everyday overview: a summary strip, CPU and memory, heat, GPU, network, disk.',
    styleId: null,
    modes: ['personal'],
    widgets: () => defaultWidgets(),
  },
  {
    id: 'gaming',
    name: 'Gaming dashboard',
    description: 'Load and heat first: CPU and GPU rings, temperatures, VRAM, memory and traffic.',
    styleId: 'gaming',
    modes: ['gaming'],
    widgets: () => [
      summary(
        [K.cpu, K.gpu, K.ram, { ...K.cpuTemp, label: 'CPU °C' }, { ...K.gpuTemp, label: 'GPU °C' }],
        [0, 0, 12, 2],
      ),
      gauge(K.cpu, [0, 2, 3, 4], 'CPU'),
      gauge(K.gpu, [3, 2, 3, 4], 'GPU'),
      line([K.cpuTemp, K.gpuTemp], [6, 2, 6, 4], 'Temperatures', true),
      area([K.ram], [0, 6, 4, 4], 'Memory'),
      group(
        'Video memory',
        [K.vram, { ...K.gpuTemp, label: 'GPU temp' }, { ...K.hotspot, label: 'Hotspot' }],
        [4, 6, 4, 4],
      ),
      area(
        [
          { ...K.down, label: 'Download' },
          { ...K.up, label: 'Upload' },
        ],
        [8, 6, 4, 4],
        'Network',
      ),
    ],
  },
  {
    id: 'development',
    name: 'Development dashboard',
    description:
      'Build pressure at engineering resolution: CPU, memory, disk I/O, network, processes.',
    styleId: 'technical',
    modes: ['development'],
    widgets: () => [
      summary(
        [
          K.cpu,
          K.ram,
          { ...K.read, label: 'Read' },
          { ...K.write, label: 'Write' },
          { ...K.down, label: 'Net ↓' },
        ],
        [0, 0, 12, 2],
      ),
      area([K.cpu], [0, 2, 8, 4], 'CPU total'),
      group('Processes', [K.procs, K.running, K.threads], [8, 2, 4, 4]),
      area([K.ram], [0, 6, 4, 4], 'Memory'),
      area([K.read, K.write], [4, 6, 4, 4], 'Disk I/O'),
      area(
        [
          { ...K.down, label: 'Download' },
          { ...K.up, label: 'Upload' },
        ],
        [8, 6, 4, 4],
        'Network',
      ),
      line([K.cpuTemp], [0, 10, 6, 3], 'CPU temperature', true),
      bar(K.disk, [6, 10, 6, 3], 'Filesystem'),
    ],
  },
  {
    id: 'personal',
    name: 'Personal starter',
    description: 'A pleasant, balanced start to make yours: rings, a chart, heat and network.',
    styleId: 'glass',
    modes: ['personal'],
    widgets: () => [
      summary([K.cpu, K.gpu, K.ram, { ...K.down, label: 'Net ↓' }], [0, 0, 12, 2]),
      area([K.cpu], [0, 2, 6, 4], 'Processor'),
      gauge(K.ram, [6, 2, 3, 4], 'Memory'),
      gauge({ ...K.cpuTemp, label: 'Temp' }, [9, 2, 3, 4], 'Temperature', true),
      area(
        [
          { ...K.down, label: 'Download' },
          { ...K.up, label: 'Upload' },
        ],
        [0, 6, 8, 4],
        'Network',
      ),
      group('Storage', [K.disk, { ...K.ssdTemp, label: 'SSD temp' }], [8, 6, 4, 4]),
    ],
  },
  {
    id: 'thermal',
    name: 'Thermal focus',
    description: 'Every temperature PULSE reads, colour-banded, beside the load that drives it.',
    styleId: 'neon',
    modes: ['gaming'],
    widgets: () => [
      line([K.cpuTemp, K.gpuTemp, K.hotspot], [0, 0, 8, 5], 'Temperatures', true),
      gauge({ ...K.cpuTemp, label: 'CPU' }, [8, 0, 4, 5], 'CPU package', true),
      tile({ ...K.cpuTemp, label: 'CPU' }, [0, 5, 3, 2], true),
      tile({ ...K.gpuTemp, label: 'GPU' }, [3, 5, 3, 2], true),
      tile({ ...K.hotspot, label: 'Hotspot' }, [6, 5, 3, 2], true),
      tile({ ...K.ssdTemp, label: 'SSD' }, [9, 5, 3, 2], true),
      area([K.cpu, K.gpu], [0, 7, 12, 4], 'Load'),
    ],
  },
  {
    id: 'network-io',
    name: 'Network & I/O',
    description: 'Traffic and disk throughput up close, with filesystem usage and Wi-Fi.',
    styleId: 'technical',
    modes: ['development'],
    widgets: () => [
      area(
        [
          { ...K.down, label: 'Download' },
          { ...K.up, label: 'Upload' },
        ],
        [0, 0, 6, 5],
        'Network',
      ),
      area([K.read, K.write], [6, 0, 6, 5], 'Disk I/O'),
      tile({ ...K.down, label: '↓' }, [0, 5, 3, 2]),
      tile({ ...K.up, label: '↑' }, [3, 5, 3, 2]),
      tile(K.wifi, [6, 5, 3, 2]),
      tile(K.ssdTemp, [9, 5, 3, 2], true),
      bar(K.disk, [0, 7, 12, 2], 'Filesystem'),
    ],
  },
  {
    id: 'minimal',
    name: 'Minimal clean',
    description: 'Four big numbers and one quiet chart. Nothing else.',
    styleId: 'stealth',
    modes: ['mini'],
    widgets: () => [
      tile(K.cpu, [0, 0, 3, 3]),
      tile(K.ram, [3, 0, 3, 3]),
      tile({ ...K.cpuTemp, label: 'Temp' }, [6, 0, 3, 3]),
      tile({ ...K.down, label: '↓' }, [9, 0, 3, 3]),
      area([K.cpu, K.ram], [0, 3, 12, 5], 'Load'),
    ],
  },
  {
    id: 'showcase',
    name: 'Fancy showcase',
    description:
      'PULSE at its most striking: frosted glass, rings, gradient trends and a live strip.',
    styleId: 'glass',
    modes: ['personal'],
    featured: true,
    widgets: () => [
      summary(
        [K.cpu, K.gpu, K.ram, { ...K.cpuTemp, label: 'Temp' }, { ...K.down, label: 'Net ↓' }],
        [0, 0, 12, 2],
      ),
      area([K.cpu], [0, 2, 7, 5], 'Processor'),
      gauge(K.ram, [7, 2, 5, 5], 'Memory'),
      gauge(K.gpu, [0, 7, 3, 4], 'Graphics'),
      gauge({ ...K.cpuTemp, label: 'Temp' }, [3, 7, 3, 4], 'Heat', true),
      area(
        [
          { ...K.down, label: 'Download' },
          { ...K.up, label: 'Upload' },
        ],
        [6, 7, 6, 4],
        'Network',
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

/** Adds a dashboard made from `template` and shows it. */
export function createDashboardFromTemplate(
  section: DashboardsSection,
  template: DashboardTemplate,
  name = template.name,
): { section: DashboardsSection; id: string | null } {
  if (section.items.length >= MAX_DASHBOARDS) return { section, id: null };
  const dashboard: Dashboard = {
    id: newId('d'),
    name: name.trim().slice(0, 40) || template.name,
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
