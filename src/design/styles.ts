import type { DeepPartial, VisualizationConfig } from '@/visualization/config';

/**
 * PULSE's built-in visual styles.
 *
 * A style is a complete look, not a colour swap: surfaces and their
 * translucency, depth (shadows, glow, blur), shape (radius, chamfer), type
 * (families, weights, casing), density, the chart treatment and the overlay
 * chrome. Everything is data; `look.ts` turns it into CSS custom properties
 * that any container can carry, so a dashboard, an overlay and the Mini
 * window can each wear a different style at once.
 *
 * Colours are `#rrggbb` with a separate alpha where translucency matters, so
 * the user's *background opacity* and *border strength* can scale them.
 */

export const STYLE_IDS = [
  'clean',
  'glass',
  'technical',
  'neon',
  'gaming',
  'stealth',
  'compact',
  'hud',
] as const;
export type StyleId = (typeof STYLE_IDS)[number];

export const DENSITIES = ['compact', 'comfortable', 'spacious'] as const;
export type Density = (typeof DENSITIES)[number];

/** How much the spacing scale grows or shrinks per density. */
export const DENSITY_SCALE: Readonly<Record<Density, number>> = {
  compact: 0.78,
  comfortable: 1,
  spacious: 1.22,
};

export const FONT_STACKS = {
  sans: "'Inter', 'Cantarell', 'Segoe UI Variable', 'Segoe UI', system-ui, sans-serif",
  display:
    "'Inter Display', 'Inter', 'Cantarell', 'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif",
  rounded:
    "'Nunito', 'Varela Round', 'SF Pro Rounded', 'Quicksand', 'Cantarell', system-ui, sans-serif",
  condensed:
    "'Barlow Condensed', 'Oswald', 'Roboto Condensed', 'Bahnschrift', 'Arial Narrow', 'Cantarell', sans-serif",
  mono: "'JetBrains Mono', 'Cascadia Mono', 'Cascadia Code', 'DejaVu Sans Mono', ui-monospace, monospace",
} as const;
export type FontKey = keyof typeof FONT_STACKS;

export interface Surface {
  readonly color: string;
  readonly alpha: number;
}

export interface StyleTokens {
  /** The app canvas: a base colour and optional image layers (gradients). */
  readonly bg: string;
  readonly backdrop: string;
  readonly surface: Surface;
  readonly raised: Surface;
  readonly sunken: Surface;
  readonly border: Surface;
  readonly borderStrong: Surface;
  readonly text: string;
  readonly textMuted: string;
  readonly textFaint: string;
  readonly accent: string;
  readonly accent2: string;
  /** Series colours, in the order a chart assigns them. */
  readonly viz: readonly [string, string, string, string, string, string];
  readonly grid: Surface;
  readonly font: FontKey;
  readonly fontDisplay: FontKey;
  readonly fontNumeric: FontKey;
  readonly weightTitle: number;
  readonly weightValue: number;
  readonly titleCase: 'uppercase' | 'none';
  /** Letter spacing of small titles, in em. */
  readonly titleTracking: number;
  /** The medium corner radius, in px; small and large derive from it. */
  readonly radius: number;
  /** Backdrop blur behind translucent surfaces, in px (0: none). */
  readonly blur: number;
  /** Card shadow depth, 0–1.5. */
  readonly shadow: number;
  /** Coloured glow around lines, values and focus, 0–1. */
  readonly glow: number;
  /** A 1px inner highlight on the top edge of cards (glass). */
  readonly highlight: number;
  /** Cut corners on cards, in px (0: none). */
  readonly chamfer: number;
  /** A dark halo under numbers so they read over anything (HUD). */
  readonly valueHalo: boolean;
  readonly density: Density;
  /** Multiplies every font size. */
  readonly fontScale: number;
}

export interface WidgetLook {
  readonly radius: number;
  readonly padding: number;
  readonly border: 'none' | 'thin';
  /** `card`: the style's surface; `bare`: no surface at all. */
  readonly surface: 'card' | 'bare';
}

export interface OverlayLook {
  readonly background: string | null;
  readonly opacity: number;
  readonly border: 'none' | 'thin';
  readonly shadow: boolean;
  readonly radius: number;
  readonly padding: number;
  readonly gap: number;
}

/**
 * A built-in style. Its name, tagline and description are translations keyed
 * by id (`styles.<id>.name` / `.tagline` / `.description`).
 */
export interface VisualStyle {
  readonly id: StyleId;
  /** Where it shines — shown on its card, never enforced. */
  readonly bestFor: readonly ('dashboard' | 'overlay' | 'mini')[];
  readonly tokens: StyleTokens;
  /**
   * The chart treatment for widgets that follow their preset. Never a
   * renderer, size, scale, precision, thresholds or series names.
   */
  readonly chart: DeepPartial<VisualizationConfig>;
  readonly widget: WidgetLook;
  readonly overlay: OverlayLook;
}

const s = (color: string, alpha = 1): Surface => ({ color, alpha });

/**
 * The card is the surface: charts inside a styled card draw none of their
 * own. The colour *mode* stays the widget's — a threshold-coloured
 * temperature keeps its bands in every style.
 */
const CARD_CHART: DeepPartial<VisualizationConfig> = {
  background: { mode: 'none', opacity: 0 },
  frame: { border: 'none', radius: 0, shadow: 'none' },
};

function chart(patch: DeepPartial<VisualizationConfig>): DeepPartial<VisualizationConfig> {
  return {
    ...CARD_CHART,
    ...patch,
    frame: { ...CARD_CHART.frame, ...patch.frame },
    background: { ...CARD_CHART.background, ...patch.background },
  };
}

export const STYLES: readonly VisualStyle[] = [
  {
    id: 'clean',
    bestFor: ['dashboard', 'mini'],
    tokens: {
      bg: '#0b0e12',
      backdrop:
        'radial-gradient(1100px 520px at 88% -8%, rgba(56, 214, 196, 0.07), transparent 62%), radial-gradient(900px 600px at -10% 110%, rgba(143, 156, 255, 0.05), transparent 60%)',
      surface: s('#141920'),
      raised: s('#1a2029'),
      sunken: s('#090c10'),
      border: s('#ffffff', 0.07),
      borderStrong: s('#ffffff', 0.13),
      text: '#e9eef4',
      textMuted: '#95a2b2',
      textFaint: '#606c7b',
      accent: '#38d6c4',
      accent2: '#8f9cff',
      viz: ['#38d6c4', '#8f9cff', '#f6c177', '#f28fad', '#6cc3ff', '#a3e635'],
      grid: s('#ffffff', 0.06),
      font: 'sans',
      fontDisplay: 'display',
      fontNumeric: 'sans',
      weightTitle: 600,
      weightValue: 600,
      titleCase: 'uppercase',
      titleTracking: 0.09,
      radius: 12,
      blur: 0,
      shadow: 0.8,
      glow: 0,
      highlight: 0.04,
      chamfer: 0,
      valueHalo: false,
      density: 'comfortable',
      fontScale: 1,
    },
    chart: chart({
      line: { width: 2, curve: 'smooth', points: 'none' },
      fill: { mode: 'gradient', opacity: 0.24 },
      axes: { x: true, y: true, grid: true },
    }),
    widget: { radius: 12, padding: 10, border: 'thin', surface: 'card' },
    overlay: {
      background: '#0d1116',
      opacity: 0.8,
      border: 'none',
      shadow: true,
      radius: 10,
      padding: 8,
      gap: 8,
    },
  },
  {
    id: 'glass',
    bestFor: ['dashboard'],
    tokens: {
      bg: '#090a18',
      backdrop:
        'radial-gradient(900px 620px at 8% -4%, rgba(99, 102, 241, 0.42), transparent 62%), radial-gradient(760px 520px at 96% 14%, rgba(34, 211, 238, 0.26), transparent 60%), radial-gradient(900px 700px at 58% 112%, rgba(236, 72, 153, 0.26), transparent 62%)',
      surface: s('#ffffff', 0.06),
      raised: s('#ffffff', 0.1),
      sunken: s('#000000', 0.28),
      border: s('#ffffff', 0.13),
      borderStrong: s('#ffffff', 0.22),
      text: '#f5f7ff',
      textMuted: '#bac1dc',
      textFaint: '#8088a8',
      accent: '#7dd3fc',
      accent2: '#c4b5fd',
      viz: ['#7dd3fc', '#c4b5fd', '#f9a8d4', '#fde68a', '#86efac', '#fca5a5'],
      grid: s('#ffffff', 0.08),
      font: 'sans',
      fontDisplay: 'display',
      fontNumeric: 'display',
      weightTitle: 600,
      weightValue: 600,
      titleCase: 'none',
      titleTracking: 0.01,
      radius: 18,
      blur: 22,
      shadow: 1.2,
      glow: 0.3,
      highlight: 0.16,
      chamfer: 0,
      valueHalo: false,
      density: 'comfortable',
      fontScale: 1,
    },
    chart: chart({
      line: { width: 2.25, curve: 'smooth', points: 'none' },
      fill: { mode: 'gradient', opacity: 0.4 },
      axes: { x: false, y: true, grid: true },
    }),
    widget: { radius: 18, padding: 12, border: 'thin', surface: 'card' },
    overlay: {
      background: '#161a33',
      opacity: 0.62,
      border: 'thin',
      shadow: true,
      radius: 16,
      padding: 10,
      gap: 10,
    },
  },
  {
    id: 'technical',
    bestFor: ['dashboard', 'overlay'],
    tokens: {
      bg: '#080a0c',
      backdrop:
        'linear-gradient(rgba(245, 185, 66, 0.035) 1px, transparent 1px) 0 0 / 24px 24px, linear-gradient(90deg, rgba(245, 185, 66, 0.035) 1px, transparent 1px) 0 0 / 24px 24px',
      surface: s('#0d1115'),
      raised: s('#131920'),
      sunken: s('#060809'),
      border: s('#2b3641'),
      borderStrong: s('#3d4a58'),
      text: '#d8e1ea',
      textMuted: '#8897a9',
      textFaint: '#57636f',
      accent: '#f5b942',
      accent2: '#4fd1c5',
      viz: ['#f5b942', '#4fd1c5', '#9ae66e', '#ff7a59', '#7aa2ff', '#e879f9'],
      grid: s('#f5b942', 0.1),
      font: 'sans',
      fontDisplay: 'mono',
      fontNumeric: 'mono',
      weightTitle: 600,
      weightValue: 500,
      titleCase: 'uppercase',
      titleTracking: 0.14,
      radius: 3,
      blur: 0,
      shadow: 0,
      glow: 0,
      highlight: 0,
      chamfer: 0,
      valueHalo: false,
      density: 'compact',
      fontScale: 0.96,
    },
    chart: chart({
      line: { width: 1.5, curve: 'straight', points: 'small' },
      fill: { mode: 'none', opacity: 0 },
      axes: { x: true, y: true, grid: true },
    }),
    widget: { radius: 3, padding: 8, border: 'thin', surface: 'card' },
    overlay: {
      background: '#0a0d10',
      opacity: 0.9,
      border: 'thin',
      shadow: false,
      radius: 2,
      padding: 6,
      gap: 6,
    },
  },
  {
    id: 'neon',
    bestFor: ['dashboard', 'overlay'],
    tokens: {
      bg: '#05050b',
      backdrop:
        'radial-gradient(820px 520px at 0% 0%, rgba(255, 43, 214, 0.17), transparent 62%), radial-gradient(900px 620px at 100% 100%, rgba(0, 240, 255, 0.15), transparent 62%)',
      surface: s('#0b0b18', 0.86),
      raised: s('#14142c'),
      sunken: s('#040409'),
      border: s('#ff2bd6', 0.26),
      borderStrong: s('#00f0ff', 0.4),
      text: '#f4ecff',
      textMuted: '#bcaad8',
      textFaint: '#7c6c9b',
      accent: '#ff2bd6',
      accent2: '#00f0ff',
      viz: ['#00f0ff', '#ff2bd6', '#faff00', '#8b5cf6', '#39ff88', '#ff8a00'],
      grid: s('#00f0ff', 0.1),
      font: 'sans',
      fontDisplay: 'display',
      fontNumeric: 'display',
      weightTitle: 700,
      weightValue: 650,
      titleCase: 'uppercase',
      titleTracking: 0.2,
      radius: 10,
      blur: 8,
      shadow: 0.6,
      glow: 1,
      highlight: 0,
      chamfer: 0,
      valueHalo: false,
      density: 'comfortable',
      fontScale: 1,
    },
    chart: chart({
      line: { width: 2.5, curve: 'smooth', points: 'none' },
      fill: { mode: 'gradient', opacity: 0.45 },
      axes: { x: false, y: true, grid: true },
    }),
    widget: { radius: 10, padding: 10, border: 'thin', surface: 'card' },
    overlay: {
      background: '#07070f',
      opacity: 0.82,
      border: 'thin',
      shadow: true,
      radius: 10,
      padding: 8,
      gap: 8,
    },
  },
  {
    id: 'gaming',
    bestFor: ['overlay', 'dashboard'],
    tokens: {
      bg: '#0a070b',
      backdrop:
        'linear-gradient(135deg, rgba(255, 59, 92, 0.12), transparent 42%), radial-gradient(720px 420px at 100% 0%, rgba(255, 176, 32, 0.12), transparent 62%)',
      surface: s('#140d14', 0.96),
      raised: s('#1d131d'),
      sunken: s('#070508'),
      border: s('#ff3b5c', 0.24),
      borderStrong: s('#ff3b5c', 0.5),
      text: '#fff4f6',
      textMuted: '#d6adb7',
      textFaint: '#916c76',
      accent: '#ff3b5c',
      accent2: '#ffb020',
      viz: ['#ff3b5c', '#ffb020', '#22d3ee', '#a78bfa', '#4ade80', '#f472b6'],
      grid: s('#ff3b5c', 0.1),
      font: 'sans',
      fontDisplay: 'condensed',
      fontNumeric: 'condensed',
      weightTitle: 700,
      weightValue: 750,
      titleCase: 'uppercase',
      titleTracking: 0.16,
      radius: 6,
      blur: 0,
      shadow: 1,
      glow: 0.55,
      highlight: 0.04,
      chamfer: 12,
      valueHalo: false,
      density: 'comfortable',
      fontScale: 1.04,
    },
    chart: chart({
      line: { width: 3, curve: 'smooth', points: 'none' },
      fill: { mode: 'gradient', opacity: 0.5 },
      axes: { x: false, y: true, grid: true },
    }),
    widget: { radius: 6, padding: 10, border: 'thin', surface: 'card' },
    overlay: {
      background: '#120b12',
      opacity: 0.84,
      border: 'thin',
      shadow: true,
      radius: 6,
      padding: 8,
      gap: 8,
    },
  },
  {
    id: 'stealth',
    bestFor: ['overlay', 'mini'],
    tokens: {
      bg: '#060708',
      backdrop: 'none',
      surface: s('#0b0c0e'),
      raised: s('#101114'),
      sunken: s('#050506'),
      border: s('#ffffff', 0.045),
      borderStrong: s('#ffffff', 0.085),
      text: '#c5cad1',
      textMuted: '#7f868f',
      textFaint: '#4c5159',
      accent: '#8b98a9',
      accent2: '#6b7686',
      viz: ['#9aa7b8', '#6f7c8e', '#b8a07a', '#8a9f8e', '#a08aa8', '#7d9aa8'],
      grid: s('#ffffff', 0.03),
      font: 'sans',
      fontDisplay: 'sans',
      fontNumeric: 'sans',
      weightTitle: 500,
      weightValue: 500,
      titleCase: 'none',
      titleTracking: 0.01,
      radius: 8,
      blur: 0,
      shadow: 0,
      glow: 0,
      highlight: 0,
      chamfer: 0,
      valueHalo: false,
      density: 'comfortable',
      fontScale: 0.98,
    },
    chart: chart({
      line: { width: 1.25, curve: 'smooth', points: 'none' },
      fill: { mode: 'none', opacity: 0 },
      axes: { x: false, y: false, grid: false },
      display: { legend: false, min: false, max: false, average: false },
    }),
    widget: { radius: 8, padding: 10, border: 'none', surface: 'card' },
    overlay: {
      background: '#060708',
      opacity: 0.62,
      border: 'none',
      shadow: false,
      radius: 6,
      padding: 6,
      gap: 6,
    },
  },
  {
    id: 'compact',
    bestFor: ['mini', 'overlay'],
    tokens: {
      bg: '#0c0f13',
      backdrop: 'none',
      surface: s('#131920'),
      raised: s('#19202a'),
      sunken: s('#090c0f'),
      border: s('#ffffff', 0.07),
      borderStrong: s('#ffffff', 0.12),
      text: '#e6ebf1',
      textMuted: '#8e9bab',
      textFaint: '#5b6776',
      accent: '#4ade80',
      accent2: '#60a5fa',
      viz: ['#4ade80', '#60a5fa', '#fbbf24', '#f472b6', '#22d3ee', '#c084fc'],
      grid: s('#ffffff', 0.05),
      font: 'sans',
      fontDisplay: 'sans',
      fontNumeric: 'sans',
      weightTitle: 600,
      weightValue: 600,
      titleCase: 'uppercase',
      titleTracking: 0.08,
      radius: 6,
      blur: 0,
      shadow: 0.3,
      glow: 0,
      highlight: 0,
      chamfer: 0,
      valueHalo: false,
      density: 'compact',
      fontScale: 0.92,
    },
    chart: chart({
      line: { width: 1.5, curve: 'smooth', points: 'none' },
      fill: { mode: 'solid', opacity: 0.14 },
      axes: { x: false, y: true, grid: true },
      display: { legend: false, min: false, max: false, average: false },
    }),
    widget: { radius: 6, padding: 6, border: 'thin', surface: 'card' },
    overlay: {
      background: '#0d1014',
      opacity: 0.82,
      border: 'none',
      shadow: false,
      radius: 6,
      padding: 4,
      gap: 4,
    },
  },
  {
    id: 'hud',
    bestFor: ['overlay'],
    tokens: {
      bg: '#07090c',
      backdrop:
        'radial-gradient(1000px 600px at 50% -20%, rgba(255, 255, 255, 0.05), transparent 60%)',
      surface: s('#0a0d10', 0.34),
      raised: s('#11151a', 0.6),
      sunken: s('#000000', 0.35),
      border: s('#ffffff', 0.06),
      borderStrong: s('#ffffff', 0.14),
      text: '#ffffff',
      textMuted: '#c9d2dc',
      textFaint: '#7e8894',
      accent: '#5eead4',
      accent2: '#93c5fd',
      viz: ['#5eead4', '#93c5fd', '#fcd34d', '#f9a8d4', '#bef264', '#fda4af'],
      grid: s('#ffffff', 0.05),
      font: 'sans',
      fontDisplay: 'display',
      fontNumeric: 'display',
      weightTitle: 600,
      weightValue: 700,
      titleCase: 'uppercase',
      titleTracking: 0.12,
      radius: 10,
      blur: 6,
      shadow: 0,
      glow: 0,
      highlight: 0,
      chamfer: 0,
      valueHalo: true,
      density: 'comfortable',
      fontScale: 1,
    },
    chart: chart({
      line: { width: 2, curve: 'smooth', points: 'none' },
      fill: { mode: 'solid', opacity: 0.12 },
      axes: { x: false, y: false, grid: false },
      display: { legend: false, min: false, max: false, average: false },
    }),
    widget: { radius: 10, padding: 6, border: 'none', surface: 'bare' },
    overlay: {
      background: null,
      opacity: 0,
      border: 'none',
      shadow: false,
      radius: 0,
      padding: 4,
      gap: 10,
    },
  },
];

export function findStyle(id: unknown): VisualStyle | undefined {
  return STYLES.find((style) => style.id === id);
}

export function styleById(id: StyleId): VisualStyle {
  return findStyle(id) ?? STYLES[0]!;
}

export function isStyleId(value: unknown): value is StyleId {
  return typeof value === 'string' && (STYLE_IDS as readonly string[]).includes(value);
}

/** Accent presets for the colour picker. Names: `styles.accents.<id>`. */
export const ACCENTS: readonly { readonly id: string; readonly color: string }[] = [
  { id: 'teal', color: '#38d6c4' },
  { id: 'sky', color: '#7dd3fc' },
  { id: 'blue', color: '#60a5fa' },
  { id: 'violet', color: '#a78bfa' },
  { id: 'magenta', color: '#ff2bd6' },
  { id: 'crimson', color: '#ff3b5c' },
  { id: 'amber', color: '#f5b942' },
  { id: 'lime', color: '#a3e635' },
  { id: 'mint', color: '#4ade80' },
  { id: 'silver', color: '#aab4c0' },
];

/** Series palettes; `style` keeps the style's own. Names: `styles.palettes.<id>`. */
export const PALETTES: Readonly<Record<string, { readonly colors: readonly string[] }>> = {
  aurora: {
    colors: ['#38d6c4', '#8f9cff', '#f6c177', '#f28fad', '#6cc3ff', '#a3e635'],
  },
  ocean: {
    colors: ['#38bdf8', '#2dd4bf', '#818cf8', '#67e8f9', '#a5b4fc', '#5eead4'],
  },
  ember: {
    colors: ['#fb7185', '#fbbf24', '#f97316', '#fda4af', '#facc15', '#ef4444'],
  },
  forest: {
    colors: ['#4ade80', '#a3e635', '#2dd4bf', '#fde047', '#86efac', '#34d399'],
  },
  candy: {
    colors: ['#f472b6', '#c084fc', '#60a5fa', '#fcd34d', '#34d399', '#fb923c'],
  },
  mono: {
    colors: ['#e5e7eb', '#9ca3af', '#6b7280', '#d1d5db', '#4b5563', '#f3f4f6'],
  },
};
export type PaletteId = 'style' | keyof typeof PALETTES;
