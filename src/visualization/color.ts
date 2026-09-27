import type { ThresholdBand, VisualizationConfig } from '@/visualization/config';
import type { Domain } from '@/visualization/scale';

/**
 * Colour resolution.
 *
 * - **Theme** — PULSE's own tokens (`--pulse-viz-*` in `theme.css`), so a
 *   future theme change recolours every chart without touching its config.
 * - **Manual** — exactly the colours the user picked.
 * - **Threshold** — the manual colours for the frame, and the user's bands
 *   for the value itself.
 *
 * Colours are applied through CSS (`style`), never as SVG attributes, so a
 * theme token works everywhere a hex value does. Opacity is always a separate
 * property rather than baked into the colour, for the same reason.
 */

export const THEME_COLORS = {
  series: ['var(--pulse-viz-1)', 'var(--pulse-viz-2)', 'var(--pulse-viz-3)', 'var(--pulse-viz-4)'],
  text: 'var(--pulse-text)',
  muted: 'var(--pulse-text-muted)',
  grid: 'var(--pulse-viz-grid)',
  background: 'var(--pulse-bg-elevated)',
  border: 'var(--pulse-border)',
  gradientStart: 'var(--pulse-bg-elevated)',
  gradientEnd: 'var(--pulse-bg-sunken)',
} as const;

/** Extra series colours beyond primary and secondary, in manual mode. */
const MANUAL_EXTRA = ['#f6c177', '#c4a7e7', '#9ccfd8', '#eb6f92'];

export interface ResolvedColors {
  readonly text: string;
  readonly muted: string;
  readonly grid: string;
  readonly background: string;
  readonly border: string;
  readonly gradientStart: string;
  readonly gradientEnd: string;
  /** The colour of series `index`, overrides included. */
  series(index: number): string;
  /** The fill under series `index`. */
  fill(index: number): string;
}

export function resolveColors(config: VisualizationConfig): ResolvedColors {
  const colors = config.colors;
  const theme = colors.mode === 'theme';

  const series = (index: number): string => {
    const override = colors.series[String(index)]?.color;
    if (override) return override;
    if (theme) return THEME_COLORS.series[index % THEME_COLORS.series.length]!;
    if (index === 0) return colors.primary;
    if (index === 1) return colors.secondary;
    return MANUAL_EXTRA[(index - 2) % MANUAL_EXTRA.length]!;
  };

  return {
    text: theme ? THEME_COLORS.text : colors.text,
    muted: theme ? THEME_COLORS.muted : colors.text,
    grid: theme ? THEME_COLORS.grid : colors.grid,
    background: theme ? THEME_COLORS.background : colors.background,
    border: theme ? THEME_COLORS.border : colors.border,
    gradientStart: theme ? THEME_COLORS.gradientStart : colors.gradientStart,
    gradientEnd: theme ? THEME_COLORS.gradientEnd : colors.gradientEnd,
    series,
    fill: (index) => (!theme && colors.fill && index === 0 ? colors.fill : series(index)),
  };
}

/**
 * The colour a value takes under threshold bands, or `fallback` when there
 * are none or threshold mode is off.
 */
export function thresholdColor(
  config: VisualizationConfig,
  value: number | null,
  fallback: string,
): string {
  if (config.colors.mode !== 'threshold' || value === null || !Number.isFinite(value)) {
    return fallback;
  }
  return bandFor(config.colors.thresholds, value)?.color ?? fallback;
}

export function bandFor(bands: readonly ThresholdBand[], value: number): ThresholdBand | undefined {
  return bands.find((band) => band.upTo === null || value <= band.upTo) ?? bands.at(-1);
}

/**
 * Visual threshold bands for a 0–100 metric: a starting point the user can
 * edit, not a verdict about what "high" means on their machine.
 */
export const PERCENT_THRESHOLDS: readonly ThresholdBand[] = [
  { upTo: 60, color: '#38d6c4' },
  { upTo: 85, color: '#e8c46a' },
  { upTo: null, color: '#f0a08f' },
];

/** Hard colour stops for threshold bands, mapped onto the value domain. */
export function thresholdStops(
  config: VisualizationConfig,
  domain: Domain,
  fallback: string,
): { offset: string; color: string }[] {
  const bands = config.colors.thresholds;
  if (bands.length === 0) return [{ offset: '0%', color: fallback }];
  const span = domain.max - domain.min || 1;
  const stops: { offset: string; color: string }[] = [];
  let from = 0;
  for (const band of bands) {
    const to = band.upTo === null ? 1 : Math.max(0, Math.min(1, (band.upTo - domain.min) / span));
    if (to <= from && band.upTo !== null) continue;
    stops.push({ offset: `${(from * 100).toFixed(3)}%`, color: band.color });
    stops.push({ offset: `${(to * 100).toFixed(3)}%`, color: band.color });
    from = to;
    if (from >= 1) break;
  }
  return stops;
}
