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
import { K, chart, meter, ring, rows, spark, strip, tx, value } from '@/presets/widgets';
import { englishText } from '@/i18n/i18n';

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

/** A footprint's name: `presets.footprints.<footprint>`. */
export function footprintLabelKey(footprint: Footprint): string {
  return `presets.footprints.${footprint}`;
}

/** What the pack needs from the session to look as designed (never enforced). */
export type PackNeed = 'placement' | 'above' | null;

/**
 * A built-in pack. Its name, description and card sample are translations
 * keyed by id (`presets.packs.<id>.name` / `.description` / `.sample`).
 */
export interface OverlayPack {
  readonly id: string;
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
  readonly widgets: () => WidgetInstance[];
}

export const OVERLAY_PACKS: readonly OverlayPack[] = [
  {
    id: 'tiny-thermals',
    footprint: 'micro',
    styleId: 'compact',
    modes: ['gaming', 'mini'],
    needs: 'above',
    layout: 'vertical',
    widgets: () => [rows(tx('thermals'), [K.cpuTemp, K.gpuTemp, K.ram], [150, 60])],
  },
  {
    id: 'tiny-stats',
    footprint: 'micro',
    styleId: 'compact',
    modes: ['gaming', 'personal', 'mini'],
    needs: 'above',
    layout: 'horizontal',
    widgets: () => [strip([K.cpu, K.gpu, K.ram, { ...K.cpuTemp, label: '°C' }], [360, 30])],
  },
  {
    id: 'top-bar',
    footprint: 'top-strip',
    styleId: 'clean',
    modes: ['gaming', 'personal'],
    needs: 'placement',
    layout: 'horizontal',
    span: 'fill',
    chrome: { radius: 0, padding: 3, shadow: false },
    gap: 12,
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
    footprint: 'bottom-strip',
    styleId: 'stealth',
    modes: ['development'],
    needs: 'placement',
    layout: 'horizontal',
    span: 'fill',
    chrome: { radius: 0, padding: 3 },
    gap: 16,
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
    footprint: 'left-rail',
    styleId: 'neon',
    modes: ['personal'],
    needs: 'placement',
    layout: 'vertical',
    span: 'fill',
    chrome: { radius: 0 },
    gap: 10,
    widgets: () => [
      ring(K.cpu, [150, 118]),
      ring(K.gpu, [150, 118]),
      ring(K.ram, [150, 118]),
      rows(tx('temperatures'), [K.cpuTemp, K.gpuTemp], [150, 48]),
      spark([K.down], [150, 32]),
    ],
  },
  {
    id: 'right-rail',
    footprint: 'right-rail',
    styleId: 'clean',
    modes: ['development'],
    needs: 'placement',
    layout: 'vertical',
    span: 'fill',
    chrome: { radius: 0 },
    gap: 8,
    widgets: () => [
      chart([K.cpu], [230, 104]),
      chart([K.ram], [230, 104]),
      chart([K.cpuTemp, K.gpuTemp], [230, 104], 'line', true),
      chart(
        [
          { ...K.down, label: tx('download') },
          { ...K.up, label: tx('upload') },
        ],
        [230, 104],
      ),
      chart([K.read, K.write], [230, 104]),
    ],
  },
  {
    id: 'gaming-corner',
    footprint: 'corner',
    styleId: 'gaming',
    modes: ['gaming'],
    needs: 'above',
    layout: 'grid',
    columns: 2,
    gap: 6,
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
    footprint: 'card',
    styleId: 'neon',
    modes: ['gaming'],
    needs: 'above',
    layout: 'horizontal',
    gap: 10,
    widgets: () => [
      spark([{ ...K.cpuTemp, label: 'CPU' }], [176, 34], true),
      spark([{ ...K.gpuTemp, label: 'GPU' }], [176, 34], true),
      value(K.ssdTemp, [104, 34], true),
    ],
  },
  {
    id: 'summary-card',
    footprint: 'block',
    styleId: 'glass',
    modes: ['personal'],
    needs: 'above',
    layout: 'grid',
    columns: 2,
    gap: 10,
    widgets: () => [
      ring(K.cpu, [150, 124]),
      ring(K.ram, [150, 124]),
      rows(
        'GPU',
        [{ ...K.gpu, label: tx('usage') }, K.vram, { ...K.gpuTemp, label: tx('temp') }],
        [150, 64],
      ),
      rows(
        tx('network'),
        [
          { ...K.down, label: tx('down') },
          { ...K.up, label: tx('up') },
        ],
        [150, 64],
      ),
    ],
  },
  {
    id: 'network-strip',
    footprint: 'card',
    styleId: 'technical',
    modes: ['development'],
    needs: 'above',
    layout: 'horizontal',
    gap: 10,
    widgets: () => [
      spark([{ ...K.down, label: '↓' }], [180, 34]),
      spark([{ ...K.up, label: '↑' }], [180, 34]),
      value(K.wifi, [124, 34]),
    ],
  },
  {
    id: 'minimal-hud',
    footprint: 'micro',
    styleId: 'hud',
    modes: ['gaming', 'personal', 'mini'],
    needs: 'above',
    layout: 'vertical',
    gap: 2,
    widgets: () => [value(K.cpu, [120, 30]), value(K.gpu, [120, 30]), value(K.ram, [120, 30])],
  },
  {
    id: 'dev-rail',
    footprint: 'right-rail',
    styleId: 'technical',
    modes: ['development'],
    needs: 'placement',
    layout: 'vertical',
    gap: 6,
    widgets: () => [
      chart([K.cpu], [240, 92]),
      chart([K.ram], [240, 92]),
      spark([{ ...K.read, label: tx('read') }], [240, 30]),
      spark([{ ...K.write, label: tx('write') }], [240, 30]),
      spark([{ ...K.down, label: tx('netDown') }], [240, 30]),
      rows(tx('processes'), [K.procs, K.running, K.threads], [240, 62]),
    ],
  },
];

export function packNameKey(id: string): string {
  return `presets.packs.${id}.name`;
}

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
  const nameKey = packNameKey(pack.id);
  const created = createOverlay(section, englishText(nameKey), pack.widgets(), {
    nameKey,
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
  // The name stays the overlay's: the pack's while never renamed, else the user's.
  const { nameKey: _packName, ...base } = rebuilt;
  return updateOverlay(section, id, () => ({
    ...base,
    id: overlay.id,
    name: overlay.name,
    ...(overlay.nameKey ? { nameKey: overlay.nameKey } : {}),
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
