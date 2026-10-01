import type { IconName } from '@/components/Icon';
import type { Density, StyleId } from '@/design/styles';
import type { MetricSpec } from '@/presets/widgets';
import { K, tx } from '@/presets/widgets';

/**
 * PULSE's modes: Gaming, Development, Personal and Mini.
 *
 * A mode is a starting point for a way of working — a style and density, a
 * dashboard, a set of overlay packs, the metrics it puts first and a few
 * interaction defaults. Entering a mode makes PULSE wear its style; the user
 * can change the style, the dashboard and the packs of any mode.
 */

export const MODE_IDS = ['gaming', 'development', 'personal', 'mini'] as const;
export type ModeId = (typeof MODE_IDS)[number];

/**
 * A mode. Its name, tagline, description and principles are translations
 * keyed by id (`modes.<id>.name` / `.tagline` / `.description` /
 * `.principles.<principle>`).
 */
export interface ModeDefinition {
  readonly id: ModeId;
  readonly icon: IconName;
  readonly style: StyleId;
  readonly density: Density;
  /** The dashboard template the mode starts from. */
  readonly template: string;
  /** Overlay packs suggested for the mode, best first. */
  readonly packs: readonly string[];
  /** The metrics the mode's header shows live, in order. */
  readonly emphasis: readonly MetricSpec[];
  /** What the mode suggests, shown as switches on its page. */
  readonly defaults: {
    /** Lock every overlay when entering the mode (click-through, no focus). */
    readonly lockOverlays: boolean;
    /** Keep PULSE running with its overlays when the main window closes. */
    readonly keepRunning: boolean;
  };
  /** Principle ids, in order. */
  readonly principles: readonly string[];
}

export const MODES: readonly ModeDefinition[] = [
  {
    id: 'gaming',
    icon: 'gaming',
    style: 'gaming',
    density: 'compact',
    template: 'gaming',
    packs: [
      'gaming-corner',
      'tiny-stats',
      'thermal-strip',
      'top-bar',
      'minimal-hud',
      'tiny-thermals',
    ],
    emphasis: [
      { ...K.cpu, label: 'CPU' },
      { ...K.gpu, label: 'GPU' },
      { ...K.cpuTemp, label: 'CPU °C' },
      { ...K.gpuTemp, label: 'GPU °C' },
      K.vram,
      K.ram,
    ],
    defaults: { lockOverlays: true, keepRunning: true },
    principles: ['noFps', 'safeOverlays', 'borderless'],
  },
  {
    id: 'development',
    icon: 'development',
    style: 'technical',
    density: 'compact',
    template: 'development',
    packs: ['dev-rail', 'right-rail', 'network-strip', 'bottom-bar'],
    emphasis: [
      K.cpu,
      K.ram,
      { ...K.read, label: tx('diskDown') },
      { ...K.write, label: tx('diskUp') },
      { ...K.down, label: tx('netDown') },
      K.procs,
    ],
    defaults: { lockOverlays: false, keepRunning: false },
    principles: ['realSamples', 'processesNearby', 'technicalStyle'],
  },
  {
    id: 'personal',
    icon: 'personal',
    style: 'glass',
    density: 'comfortable',
    template: 'personal',
    packs: ['summary-card', 'minimal-hud', 'tiny-stats', 'left-rail'],
    emphasis: [
      K.cpu,
      K.ram,
      { ...K.cpuTemp, label: tx('temp') },
      { ...K.down, label: tx('netDown') },
    ],
    defaults: { lockOverlays: false, keepRunning: false },
    principles: ['editable', 'saveLook', 'shareExport'],
  },
  {
    id: 'mini',
    icon: 'mini',
    style: 'compact',
    density: 'compact',
    template: 'minimal',
    packs: ['tiny-stats', 'tiny-thermals', 'minimal-hud'],
    emphasis: [K.cpu, K.ram, { ...K.cpuTemp, label: tx('temp') }],
    defaults: { lockOverlays: false, keepRunning: false },
    principles: ['normalWindow', 'pickLayout'],
  },
];

export function findMode(id: unknown): ModeDefinition | undefined {
  return MODES.find((mode) => mode.id === id);
}

export function isModeId(value: unknown): value is ModeId {
  return typeof value === 'string' && (MODE_IDS as readonly string[]).includes(value);
}
