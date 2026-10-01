import type { WidgetInstance } from '@/dashboard/model';
import type { StyleId } from '@/design/styles';
import type { Customization } from '@/design/appearance';
import { DEFAULT_CUSTOMIZATION } from '@/design/appearance';
import { overlayStyleChrome, resolveLook } from '@/design/look';
import type { ModeId } from '@/modes/modes';
import type {
  Overlay,
  OverlayChrome,
  OverlayGeometry,
  OverlayLayout,
  OverlaySpan,
  OverlaysSection,
} from '@/overlay/model';
import { contentSize, createOverlay, updateOverlay } from '@/overlay/model';
import { K, chart, meter, ring, rows, spark, strip, value } from '@/presets/widgets';

/**
 * PULSE's built-in overlay packs: composed, styled, ready-to-use overlays.
 *
 * Each pack is a deliberate composition for a footprint — a micro readout, a
 * floating card, a full-width bar, a full-height rail, a corner HUD — not a
 * resized copy of another. Every part stays editable after creation.
 */

export const PACKS_VERSION = 1;

export const FOOTPRINTS = [
  'micro',
  'card',
  'top-strip',
  'bottom-strip',
  'left-rail',
  'right-rail',
  'corner',
  'block',
] as const;
export type Footprint = (typeof FOOTPRINTS)[number];

export const FOOTPRINT_LABELS: Readonly<Record<Footprint, string>> = {
  micro: 'Micro',
  card: 'Floating card',
  'top-strip': 'Top bar',
  'bottom-strip': 'Bottom bar',
  'left-rail': 'Left rail',
  'right-rail': 'Right rail',
  corner: 'Corner HUD',
  block: 'Summary block',
};

/** What the pack needs from the session to look as designed (never enforced). */
export type PackNeed = 'placement' | 'above' | null;

export interface OverlayPack {
  readonly id: string;
  readonly name: string;
  readonly description: string;
  readonly footprint: Footprint;
  readonly styleId: StyleId;
  readonly modes: readonly ModeId[];
  readonly needs: PackNeed;
  readonly layout: OverlayLayout;
  readonly columns?: number;
  readonly span?: OverlaySpan;
  /** Adjustments over the style's chrome for this composition. */
  readonly chrome?: Partial<OverlayChrome>;
  readonly gap?: number;
  /** What it reads like, item by item, for the pack card. */
  readonly sample: readonly string[];
  readonly widgets: () => WidgetInstance[];
}

export const OVERLAY_PACKS: readonly OverlayPack[] = [
  {
    id: 'tiny-thermals',
    name: 'Tiny Thermals',
    description: 'CPU and GPU temperature with memory, in a whisper-small block.',
    footprint: 'micro',
    styleId: 'compact',
    modes: ['gaming', 'mini'],
    needs: 'above',
    layout: 'vertical',
    sample: ['CPU 64 °C', 'GPU 58 °C', 'RAM 42 %'],
    widgets: () => [rows('Thermals', [K.cpuTemp, K.gpuTemp, K.ram], [150, 60])],
  },
  {
    id: 'tiny-stats',
    name: 'Tiny Stats',
    description: 'CPU, GPU, RAM and temperature in one slim line.',
    footprint: 'micro',
    styleId: 'compact',
    modes: ['gaming', 'personal', 'mini'],
    needs: 'above',
    layout: 'horizontal',
    sample: ['CPU 24 %', 'GPU 84 %', 'RAM 41 %', '67 °C'],
    widgets: () => [strip([K.cpu, K.gpu, K.ram, { ...K.cpuTemp, label: '°C' }], [360, 30])],
  },
  {
    id: 'top-bar',
    name: 'Top Bar',
    description: 'A full-width summary strip across the top of the screen, with live trends.',
    footprint: 'top-strip',
    styleId: 'clean',
    modes: ['gaming', 'personal'],
    needs: 'placement',
    layout: 'horizontal',
    span: 'fill',
    chrome: { radius: 0, padding: 3, shadow: false },
    gap: 12,
    sample: [
      'CPU 23 % ∿',
      'GPU 61 % ∿',
      'RAM 44 % ∿',
      'CPU 67 °C ∿',
      '↓ 2.1 MB/s ∿',
      '↑ 180 kB/s ∿',
    ],
    widgets: () => [
      spark([K.cpu], [150, 26]),
      spark([K.gpu], [150, 26]),
      spark([K.ram], [150, 26]),
      spark([{ ...K.cpuTemp, label: 'CPU' }], [150, 26], true),
      spark([K.down], [150, 26]),
      spark([K.up], [150, 26]),
    ],
  },
  {
    id: 'bottom-bar',
    name: 'Bottom Bar',
    description: 'Meters along the bottom edge: load bars for CPU, GPU and memory, and traffic.',
    footprint: 'bottom-strip',
    styleId: 'stealth',
    modes: ['development'],
    needs: 'placement',
    layout: 'horizontal',
    span: 'fill',
    chrome: { radius: 0, padding: 3 },
    gap: 16,
    sample: ['CPU ▰▰▰▱▱', 'GPU ▰▰▱▱▱', 'RAM ▰▰▰▰▱', '↓ 2.1 MB/s', '↑ 180 kB/s'],
    widgets: () => [
      meter(K.cpu, [200, 24]),
      meter(K.gpu, [200, 24]),
      meter(K.ram, [200, 24]),
      value(K.down, [130, 24]),
      value(K.up, [130, 24]),
    ],
  },
  {
    id: 'left-rail',
    name: 'Left Rail',
    description: 'A full-height column of rings and trends down the left edge.',
    footprint: 'left-rail',
    styleId: 'neon',
    modes: ['personal'],
    needs: 'placement',
    layout: 'vertical',
    span: 'fill',
    chrome: { radius: 0 },
    gap: 10,
    sample: ['◔ CPU 23 %', '◑ GPU 61 %', '◕ RAM 44 %', 'CPU 67 °C', '↓ 2.1 MB/s'],
    widgets: () => [
      ring(K.cpu, [150, 118]),
      ring(K.gpu, [150, 118]),
      ring(K.ram, [150, 118]),
      rows('Temperatures', [K.cpuTemp, K.gpuTemp], [150, 48]),
      spark([K.down], [150, 32]),
    ],
  },
  {
    id: 'right-rail',
    name: 'Right Rail',
    description: 'Full-height monitoring charts on the right: CPU, memory, heat, network, disk.',
    footprint: 'right-rail',
    styleId: 'clean',
    modes: ['development'],
    needs: 'placement',
    layout: 'vertical',
    span: 'fill',
    chrome: { radius: 0 },
    gap: 8,
    sample: ['CPU ⌇⌇⌇', 'RAM ⌇⌇⌇', '°C ⌇⌇⌇', '↓↑ ⌇⌇⌇', 'R/W ⌇⌇⌇'],
    widgets: () => [
      chart([K.cpu], [230, 104]),
      chart([K.ram], [230, 104]),
      chart([K.cpuTemp, K.gpuTemp], [230, 104], 'line', true),
      chart(
        [
          { ...K.down, label: 'Download' },
          { ...K.up, label: 'Upload' },
        ],
        [230, 104],
      ),
      chart([K.read, K.write], [230, 104]),
    ],
  },
  {
    id: 'gaming-corner',
    name: 'Gaming Corner',
    description: 'A bold six-figure HUD for a top corner: load, heat, VRAM and RAM.',
    footprint: 'corner',
    styleId: 'gaming',
    modes: ['gaming'],
    needs: 'above',
    layout: 'grid',
    columns: 2,
    gap: 6,
    sample: ['CPU 31 %', 'GPU 97 %', 'CPU 71 °C', 'GPU 74 °C', 'VRAM 6.1 GB', 'RAM 44 %'],
    widgets: () => [
      value(K.cpu, [112, 40]),
      value(K.gpu, [112, 40]),
      value({ ...K.cpuTemp, label: 'CPU' }, [112, 40], true),
      value({ ...K.gpuTemp, label: 'GPU' }, [112, 40], true),
      value(K.vram, [112, 40]),
      value(K.ram, [112, 40]),
    ],
  },
  {
    id: 'thermal-strip',
    name: 'Thermal Strip',
    description: 'Every temperature PULSE can read, colour-banded, with trends.',
    footprint: 'card',
    styleId: 'neon',
    modes: ['gaming'],
    needs: 'above',
    layout: 'horizontal',
    gap: 10,
    sample: ['CPU 71 °C ∿', 'GPU 64 °C ∿', 'SSD 41 °C'],
    widgets: () => [
      spark([{ ...K.cpuTemp, label: 'CPU' }], [176, 34], true),
      spark([{ ...K.gpuTemp, label: 'GPU' }], [176, 34], true),
      value(K.ssdTemp, [104, 34], true),
    ],
  },
  {
    id: 'summary-card',
    name: 'System Summary Card',
    description: 'An elegant floating card: CPU and memory rings, GPU and network grouped.',
    footprint: 'block',
    styleId: 'glass',
    modes: ['personal'],
    needs: 'above',
    layout: 'grid',
    columns: 2,
    gap: 10,
    sample: ['◔ CPU 23 %', '◕ RAM 44 %', 'GPU 61 %', '↓ 2.1 MB/s'],
    widgets: () => [
      ring(K.cpu, [150, 124]),
      ring(K.ram, [150, 124]),
      rows(
        'GPU',
        [{ ...K.gpu, label: 'Usage' }, K.vram, { ...K.gpuTemp, label: 'Temp' }],
        [150, 64],
      ),
      rows(
        'Network',
        [
          { ...K.down, label: 'Down' },
          { ...K.up, label: 'Up' },
        ],
        [150, 64],
      ),
    ],
  },
  {
    id: 'network-strip',
    name: 'Network Strip',
    description: 'Download and upload with their trends, and Wi-Fi signal where there is one.',
    footprint: 'card',
    styleId: 'technical',
    modes: ['development'],
    needs: 'above',
    layout: 'horizontal',
    gap: 10,
    sample: ['↓ 12 MB/s ∿', '↑ 1.2 MB/s ∿', 'Wi-Fi −52 dBm'],
    widgets: () => [
      spark([{ ...K.down, label: '↓' }], [180, 34]),
      spark([{ ...K.up, label: '↑' }], [180, 34]),
      value(K.wifi, [124, 34]),
    ],
  },
  {
    id: 'minimal-hud',
    name: 'Minimal Transparent HUD',
    description: 'Three figures and nothing else — no panel, a soft halo for legibility.',
    footprint: 'micro',
    styleId: 'hud',
    modes: ['gaming', 'personal', 'mini'],
    needs: 'above',
    layout: 'vertical',
    gap: 2,
    sample: ['CPU 23 %', 'GPU 61 %', 'RAM 44 %'],
    widgets: () => [value(K.cpu, [120, 30]), value(K.gpu, [120, 30]), value(K.ram, [120, 30])],
  },
  {
    id: 'dev-rail',
    name: 'Dev Monitor Rail',
    description:
      'For coding sessions: CPU and memory charts, disk I/O, network and process counts.',
    footprint: 'right-rail',
    styleId: 'technical',
    modes: ['development'],
    needs: 'placement',
    layout: 'vertical',
    gap: 6,
    sample: ['CPU ⌇⌇⌇', 'RAM ⌇⌇⌇', 'Read ∿', 'Write ∿', 'Net ↓ ∿', '412 processes'],
    widgets: () => [
      chart([K.cpu], [240, 92]),
      chart([K.ram], [240, 92]),
      spark([{ ...K.read, label: 'Read' }], [240, 30]),
      spark([{ ...K.write, label: 'Write' }], [240, 30]),
      spark([{ ...K.down, label: 'Net ↓' }], [240, 30]),
      rows('Processes', [K.procs, K.running, K.threads], [240, 62]),
    ],
  },
];

export function findPack(id: unknown): OverlayPack | undefined {
  return OVERLAY_PACKS.find((pack) => pack.id === id);
}

/** A monitor's usable size in logical pixels. */
export interface ScreenSize {
  readonly width: number;
  readonly height: number;
}

export const FALLBACK_SCREEN: ScreenSize = { width: 1920, height: 1080 };

/** Where a pack's footprint sits on `screen` (monitor-relative, logical px). */
export function footprintGeometry(
  footprint: Footprint,
  size: { width: number; height: number },
  screen: ScreenSize,
  index: number,
): Pick<OverlayGeometry, 'x' | 'y' | 'width' | 'height'> {
  const margin = 24;
  const cascade = 40 + index * 24;
  switch (footprint) {
    case 'top-strip':
      return { x: 0, y: 0, width: screen.width, height: size.height };
    case 'bottom-strip':
      return {
        x: 0,
        y: Math.max(0, screen.height - size.height),
        width: screen.width,
        height: size.height,
      };
    case 'left-rail':
      return { x: 0, y: 0, width: size.width, height: screen.height };
    case 'right-rail':
      return {
        x: Math.max(0, screen.width - size.width),
        y: 0,
        width: size.width,
        height: screen.height,
      };
    case 'corner':
      return {
        x: Math.max(0, screen.width - size.width - margin),
        y: margin,
        width: size.width,
        height: size.height,
      };
    default:
      return { x: cascade, y: cascade, width: size.width, height: size.height };
  }
}

/**
 * Creates an overlay from a pack: its widgets, its style (tokens and chrome,
 * with the user's overlay padding/gap tuning), its footprint's geometry on
 * `screen`, and its origin — so it can be recognised and reset later.
 */
export function createOverlayFromPack(
  section: OverlaysSection,
  pack: OverlayPack,
  screen: ScreenSize = FALLBACK_SCREEN,
  custom: Customization = DEFAULT_CUSTOMIZATION,
): { section: OverlaysSection; id: string | null } {
  const look = overlayStyleChrome(resolveLook(pack.styleId, custom));
  const chrome: OverlayChrome = { ...look.chrome, ...pack.chrome };
  const created = createOverlay(section, pack.name, pack.widgets(), {
    styleId: pack.styleId,
    layout: pack.layout,
    columns: pack.columns ?? 2,
    gap: custom.overlayGap ?? pack.gap ?? look.gap,
    chrome,
    span: pack.span ?? 'content',
    origin: { pack: pack.id, version: PACKS_VERSION },
  });
  if (!created.id) return created;
  const index = section.items.length;
  const next = updateOverlay(created.section, created.id, (overlay: Overlay) => {
    const natural = contentSize({ ...overlay, span: 'content' });
    const geometry = footprintGeometry(pack.footprint, natural, screen, index);
    return { ...overlay, geometry: { ...overlay.geometry, ...geometry } };
  });
  return { section: next, id: created.id };
}

/** Rebuilds a pack overlay's widgets and chrome, keeping its place and name. */
export function resetOverlayToPack(
  section: OverlaysSection,
  id: string,
  custom: Customization = DEFAULT_CUSTOMIZATION,
): OverlaysSection {
  const overlay = section.items.find((item) => item.id === id);
  const pack = findPack(overlay?.origin?.pack);
  if (!overlay || !pack) return section;
  const fresh = createOverlayFromPack({ ...section, items: [] }, pack, FALLBACK_SCREEN, custom);
  const rebuilt = fresh.section.items[0];
  if (!rebuilt) return section;
  return updateOverlay(section, id, () => ({
    ...rebuilt,
    id: overlay.id,
    name: overlay.name,
    visible: overlay.visible,
    locked: overlay.locked,
    geometry:
      rebuilt.span === 'fill'
        ? {
            ...rebuilt.geometry,
            x: overlay.geometry.x,
            y: overlay.geometry.y,
            monitor: overlay.geometry.monitor,
          }
        : { ...overlay.geometry, width: rebuilt.geometry.width, height: rebuilt.geometry.height },
  }));
}
