import type { IconName } from '@/components/Icon';
import type { Density, StyleId } from '@/design/styles';
import type { MetricSpec } from '@/presets/widgets';
import { K } from '@/presets/widgets';

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

export interface ModeDefinition {
  readonly id: ModeId;
  readonly name: string;
  readonly icon: IconName;
  readonly tagline: string;
  readonly description: string;
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
  readonly principles: readonly string[];
}

export const MODES: readonly ModeDefinition[] = [
  {
    id: 'gaming',
    name: 'Gaming',
    icon: 'gaming',
    tagline: 'Overlay-first. Read it in a glance.',
    description:
      'Bold, compact overlays for performance and thermals next to your game — never inside it. Locked overlays let every click through and never take focus.',
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
    principles: [
      'No FPS counter: PULSE has no real FPS source, so it shows none.',
      'Safe desktop overlays only — no injection, no DirectX/Vulkan/OpenGL hooks.',
      'Borderless or windowed games; exclusive fullscreen hides desktop windows.',
    ],
  },
  {
    id: 'development',
    name: 'Development',
    icon: 'development',
    tagline: 'Build pressure, at engineering resolution.',
    description:
      'CPU, memory, disk I/O, network and process counts in a dense, exact layout — for compiles, containers, test runs and debugging sessions.',
    style: 'technical',
    density: 'compact',
    template: 'development',
    packs: ['dev-rail', 'right-rail', 'network-strip', 'bottom-bar'],
    emphasis: [
      K.cpu,
      K.ram,
      { ...K.read, label: 'Disk ↓' },
      { ...K.write, label: 'Disk ↑' },
      { ...K.down, label: 'Net ↓' },
      K.procs,
    ],
    defaults: { lockOverlays: false, keepRunning: false },
    principles: [
      'Every figure is a real sample; history comes from PULSE’s own recorder.',
      'The Processes view is one click away for the process behind a spike.',
      'Technical style: straight segments and sample markers — nothing smoothed.',
    ],
  },
  {
    id: 'personal',
    name: 'Personal',
    icon: 'personal',
    tagline: 'Your everyday view, made yours.',
    description:
      'A pleasant, balanced starting point to curate: add what you care about, pick any style, keep it on screen all day.',
    style: 'glass',
    density: 'comfortable',
    template: 'personal',
    packs: ['summary-card', 'minimal-hud', 'tiny-stats', 'left-rail'],
    emphasis: [K.cpu, K.ram, { ...K.cpuTemp, label: 'Temp' }, { ...K.down, label: 'Net ↓' }],
    defaults: { lockOverlays: false, keepRunning: false },
    principles: [
      'Everything here is editable: widgets, style, layout, overlays.',
      'Save the look you build as your own style in Appearance.',
      'Share a dashboard with Export — it carries no hardware identifier.',
    ],
  },
  {
    id: 'mini',
    name: 'Mini',
    icon: 'mini',
    tagline: 'A small, elegant monitor window.',
    description:
      'A compact, ordinary window with a handful of carefully sized readings. Interactive, never always-on-top — for that, use an overlay.',
    style: 'compact',
    density: 'compact',
    template: 'minimal',
    packs: ['tiny-stats', 'tiny-thermals', 'minimal-hud'],
    emphasis: [K.cpu, K.ram, { ...K.cpuTemp, label: 'Temp' }],
    defaults: { lockOverlays: false, keepRunning: false },
    principles: [
      'Mini is a normal window: movable, focusable, in the taskbar.',
      'Pick a Mini layout below, or show one of your dashboards in it.',
    ],
  },
];

export function findMode(id: unknown): ModeDefinition | undefined {
  return MODES.find((mode) => mode.id === id);
}

export function isModeId(value: unknown): value is ModeId {
  return typeof value === 'string' && (MODE_IDS as readonly string[]).includes(value);
}
