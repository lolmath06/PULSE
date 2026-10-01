import type { CSSProperties } from 'react';
import type { DeepPartial, VisualizationConfig } from '@/visualization/config';
import { mergeConfig } from '@/visualization/config';
import type { WidgetFrame, WidgetInstance } from '@/dashboard/model';
import type { OverlayChrome } from '@/overlay/model';
import { DEFAULT_FRAME } from '@/dashboard/model';
import { alpha, mix, onColor } from '@/design/color';
import type { AppearanceSection, Customization, FontChoice, Motion } from '@/design/appearance';
import { DEFAULT_CUSTOMIZATION, effectiveStyle } from '@/design/appearance';
import type {
  Density,
  FontKey,
  OverlayLook,
  StyleId,
  Surface,
  VisualStyle,
  WidgetLook,
} from '@/design/styles';
import { DENSITY_SCALE, FONT_STACKS, PALETTES, styleById } from '@/design/styles';
import { findMode } from '@/modes/modes';

/**
 * A style with the user's customization applied: everything a surface needs
 * to draw itself, resolved to plain values.
 */
export interface Look {
  readonly style: VisualStyle;
  readonly custom: Customization;
  readonly accent: string;
  readonly accent2: string;
  readonly viz: readonly string[];
  readonly density: Density;
  readonly spacing: number;
  readonly radius: number;
  readonly surfaceAlpha: number;
  readonly blur: number;
  readonly borderStrength: number;
  readonly shadow: number;
  readonly font: string;
  readonly fontDisplay: string;
  readonly fontNumeric: string;
  readonly fontScale: number;
  /** The chart treatment for widgets that follow their preset. */
  readonly chart: DeepPartial<VisualizationConfig>;
  readonly widget: WidgetLook;
  readonly overlay: OverlayLook;
  /** Gap between dashboard widgets, px. */
  readonly gridGap: number;
  readonly showTitles: boolean;
  readonly microLabels: boolean;
  readonly motion: Motion;
}

function fontFor(choice: FontChoice, fallback: FontKey): string {
  return FONT_STACKS[choice === 'style' ? fallback : choice];
}

function scaled(value: number, scale: number): number {
  return Math.round(value * scale * 10) / 10;
}

export function resolveLook(styleId: StyleId, custom: Customization = DEFAULT_CUSTOMIZATION): Look {
  const style = styleById(styleId);
  const t = style.tokens;
  const density = custom.density ?? t.density;
  const spacing = DENSITY_SCALE[density];
  const accent = custom.accent ?? t.accent;
  const palette =
    custom.palette !== 'style' && PALETTES[custom.palette]
      ? PALETTES[custom.palette]!.colors
      : t.viz;
  // A chosen accent leads the series, so single-series charts wear it.
  const viz = custom.accent
    ? [accent, ...palette.filter((c) => c !== accent)].slice(0, 6)
    : [...palette];
  const radius = scaled(t.radius, custom.radiusScale);
  const chartPatch: DeepPartial<VisualizationConfig> = {
    line: {
      ...(custom.lineWidth !== null ? { width: custom.lineWidth } : {}),
      ...(custom.curve !== null ? { curve: custom.curve } : {}),
    },
    fill: custom.fillOpacity !== null ? { opacity: custom.fillOpacity } : {},
  };
  const widgetPadding = custom.widgetPadding ?? Math.round(style.widget.padding * spacing);
  return {
    style,
    custom,
    accent,
    accent2: t.accent2,
    viz,
    density,
    spacing,
    radius,
    surfaceAlpha: custom.surfaceOpacity ?? t.surface.alpha,
    blur: custom.blur ?? t.blur,
    borderStrength: custom.borderStrength ?? 1,
    shadow: t.shadow * (custom.shadowStrength ?? 1),
    font: fontFor(custom.font, t.font),
    fontDisplay: fontFor(custom.font, t.fontDisplay),
    fontNumeric: fontFor(custom.font, t.fontNumeric),
    fontScale: Math.round(t.fontScale * custom.fontScale * 1000) / 1000,
    chart: mergeConfig(style.chart, chartPatch),
    widget: {
      ...style.widget,
      radius: scaled(style.widget.radius, custom.radiusScale),
      padding: widgetPadding,
    },
    overlay: {
      ...style.overlay,
      radius: scaled(style.overlay.radius, custom.radiusScale),
      padding: custom.overlayPadding ?? style.overlay.padding,
      gap: custom.overlayGap ?? style.overlay.gap,
    },
    gridGap: custom.gap ?? Math.round(10 * spacing),
    showTitles: custom.showTitles,
    microLabels: custom.microLabels,
    motion: custom.motion,
  };
}

/**
 * The look the whole app wears: the active mode's style (and density, unless
 * the user chose one), or the chosen style.
 */
export function appLook(section: AppearanceSection): Look {
  const custom =
    section.activeMode && section.custom.density === null
      ? { ...section.custom, density: findMode(section.activeMode)!.density }
      : section.custom;
  return resolveLook(effectiveStyle(section), custom);
}

function surface(value: Surface, alphaValue = value.alpha): string {
  return alpha(value.color, alphaValue);
}

/** A border colour at `strength` × its own visibility. */
function border(value: Surface, strength: number, text: string): string {
  if (value.alpha >= 1 && strength > 1)
    return mix(value.color, text, Math.min(1, (strength - 1) * 0.35));
  return alpha(value.color, Math.min(1, value.alpha * strength));
}

function shadowFor(depth: number, glow: number, accent: string): string {
  const layers: string[] = [];
  if (depth > 0) {
    layers.push(`0 1px 2px rgba(0, 0, 0, ${(0.22 * Math.min(1.5, depth)).toFixed(3)})`);
    layers.push(
      `0 ${Math.round(14 * depth)}px ${Math.round(36 * depth)}px -${Math.round(16 * depth)}px rgba(0, 0, 0, ${(0.55 * Math.min(1.5, depth)).toFixed(3)})`,
    );
  }
  if (glow > 0)
    layers.push(
      `0 0 ${Math.round(26 * glow)}px -${Math.round(10 * glow)}px ${alpha(accent, 0.5 * glow)}`,
    );
  return layers.length > 0 ? layers.join(', ') : '0 0 0 0 transparent';
}

/**
 * The CSS custom properties of a look. Applied to `:root` for the app, or to
 * any container to give just that part another style.
 */
export function tokensFor(look: Look): Record<string, string> {
  const t = look.style.tokens;
  const s = look.spacing;
  const r = look.radius;
  const surfaceAlpha = look.surfaceAlpha;
  const raisedAlpha = Math.min(
    1,
    Math.max(surfaceAlpha, t.raised.alpha * (surfaceAlpha / Math.max(0.01, t.surface.alpha))),
  );
  const glow = t.glow;
  const motionScale = look.motion === 'none' ? 0 : look.motion === 'reduced' ? 0.5 : 1;
  const ms = (value: number) => `${Math.round(value * motionScale)}ms`;
  return {
    '--pulse-bg': t.bg,
    '--pulse-backdrop': t.backdrop,
    '--pulse-surface': surface(t.surface, surfaceAlpha),
    '--pulse-surface-raised': surface(t.raised, raisedAlpha),
    '--pulse-bg-elevated': surface(t.surface, surfaceAlpha),
    '--pulse-bg-sunken': surface(t.sunken),
    '--pulse-bg-hover': alpha(t.text, 0.06),
    '--pulse-border': border(t.border, look.borderStrength, t.text),
    '--pulse-border-strong': border(t.borderStrong, look.borderStrength, t.text),
    '--pulse-text': t.text,
    '--pulse-text-muted': t.textMuted,
    '--pulse-text-faint': t.textFaint,
    '--pulse-accent': look.accent,
    '--pulse-accent-2': look.accent2,
    '--pulse-accent-dim': mix(look.accent, t.bg, 0.52),
    '--pulse-accent-soft': alpha(look.accent, 0.14),
    '--pulse-accent-contrast': onColor(look.accent),
    '--pulse-viz-1': look.viz[0] ?? look.accent,
    '--pulse-viz-2': look.viz[1] ?? look.accent2,
    '--pulse-viz-3': look.viz[2] ?? look.accent,
    '--pulse-viz-4': look.viz[3] ?? look.accent2,
    '--pulse-viz-5': look.viz[4] ?? look.accent,
    '--pulse-viz-6': look.viz[5] ?? look.accent2,
    '--pulse-viz-grid': surface(t.grid),
    '--pulse-font': look.font,
    '--pulse-font-display': look.fontDisplay,
    '--pulse-font-numeric': look.fontNumeric,
    '--pulse-font-scale': String(look.fontScale),
    '--pulse-weight-title': String(t.weightTitle),
    '--pulse-weight-value': String(t.weightValue),
    '--pulse-title-case': t.titleCase,
    '--pulse-title-tracking': `${t.titleTracking}em`,
    '--pulse-radius-sm': `${Math.round(r * 0.55 * 10) / 10}px`,
    '--pulse-radius': `${r}px`,
    '--pulse-radius-lg': `${Math.round(r * 1.4 * 10) / 10}px`,
    '--pulse-radius-xl': `${Math.round(r * 1.9 * 10) / 10}px`,
    '--pulse-blur': `${look.blur}px`,
    '--pulse-shadow': shadowFor(look.shadow, glow * 0.6, look.accent),
    '--pulse-shadow-hover': shadowFor(Math.max(0.6, look.shadow * 1.35), glow, look.accent),
    '--pulse-shadow-pop': shadowFor(Math.max(1, look.shadow), glow * 0.4, look.accent),
    '--pulse-highlight':
      t.highlight > 0
        ? `inset 0 1px 0 rgba(255, 255, 255, ${t.highlight})`
        : 'inset 0 0 0 0 transparent',
    '--pulse-glow': String(glow),
    '--pulse-glow-color': alpha(look.accent, 0.55 * glow),
    '--pulse-line-glow':
      glow > 0 ? `drop-shadow(0 0 ${Math.round(2 + 5 * glow)}px currentColor)` : 'none',
    '--pulse-chamfer': `${Math.round(t.chamfer * look.custom.radiusScale)}px`,
    '--pulse-value-shadow': t.valueHalo
      ? '0 1px 2px rgba(0, 0, 0, 0.95), 0 0 10px rgba(0, 0, 0, 0.7)'
      : glow >= 0.7
        ? `0 0 14px ${alpha(look.accent, 0.5)}`
        : 'none',
    '--pulse-space-1': `${Math.round(4 * s)}px`,
    '--pulse-space-2': `${Math.round(8 * s)}px`,
    '--pulse-space-3': `${Math.round(12 * s)}px`,
    '--pulse-space-4': `${Math.round(16 * s)}px`,
    '--pulse-space-5': `${Math.round(24 * s)}px`,
    '--pulse-space-6': `${Math.round(32 * s)}px`,
    '--pulse-motion-fast': ms(120),
    '--pulse-motion': ms(200),
    '--pulse-motion-slow': ms(380),
  };
}

/** A look's tokens as a React style object, plus the attributes CSS keys on. */
export function scopeProps(look: Look): {
  style: CSSProperties;
  'data-pulse-style': StyleId;
  'data-density': Density;
  'data-motion': Motion;
} {
  return {
    style: tokensFor(look) as CSSProperties,
    'data-pulse-style': look.style.id,
    'data-density': look.density,
    'data-motion': look.motion,
  };
}

/** Whether a widget frame is untouched — then its shape follows the style. */
export function frameFollowsStyle(frame: WidgetFrame): boolean {
  return (
    frame.padding === DEFAULT_FRAME.padding &&
    frame.radius === DEFAULT_FRAME.radius &&
    frame.border === DEFAULT_FRAME.border &&
    frame.background === DEFAULT_FRAME.background &&
    frame.opacity === DEFAULT_FRAME.opacity
  );
}

/**
 * A widget as it is drawn under `look`, never as it is stored:
 *
 * - a chart still on its preset (`visual.modified === false`) takes the
 *   style's treatment — line, fill, grid, axes — and the user's global chart
 *   tuning; renderer, size, scale, precision, thresholds and series names
 *   always stay the widget's own;
 * - an untouched frame takes the style's radius, padding and border;
 * - titles and micro labels follow the global switches.
 *
 * A widget the user customised keeps exactly what they chose.
 */
export function styleWidget(widget: WidgetInstance, look: Look): WidgetInstance {
  let config = widget.visual.config;
  if (!widget.visual.modified) {
    const own = config;
    const merged = mergeConfig(config, look.chart);
    // A style may hide chart parts, never bring back one the widget hides:
    // a bare micro chart stays bare in Technical.
    config = {
      ...merged,
      axes: {
        x: own.axes.x && merged.axes.x,
        y: own.axes.y && merged.axes.y,
        grid: own.axes.grid && merged.axes.grid,
      },
      display: {
        ...merged.display,
        legend: own.display.legend && merged.display.legend,
        current: own.display.current && merged.display.current,
        min: own.display.min && merged.display.min,
        max: own.display.max && merged.display.max,
        average: own.display.average && merged.display.average,
        compact: own.display.compact,
      },
    };
  }
  const micro =
    widget.kind === 'value' || config.renderer === 'sparkline' || config.display.compact;
  if (!look.microLabels && micro && config.text.showLabel) {
    config = mergeConfig(config, { text: { showLabel: false } });
  }
  if (look.fontScale !== 1) {
    config = mergeConfig(config, {
      text: { scale: Math.min(2, Math.max(0.6, config.text.scale * look.fontScale)) },
    });
  }
  const follows = frameFollowsStyle(widget.frame);
  const frame: WidgetFrame = {
    ...widget.frame,
    ...(follows
      ? { radius: look.widget.radius, padding: look.widget.padding, border: look.widget.border }
      : {}),
    showTitle: widget.frame.showTitle && look.showTitles,
  };
  if (config === widget.visual.config && frame.showTitle === widget.frame.showTitle && !follows) {
    return widget;
  }
  return { ...widget, visual: { ...widget.visual, config }, frame };
}

/** The chrome and gap a look gives an overlay (see `applyOverlayStyle`). */
export function overlayStyleChrome(look: Look): { chrome: OverlayChrome; gap: number } {
  const o = look.overlay;
  return {
    chrome: {
      background: o.background,
      opacity: o.background === null ? 0 : o.opacity,
      border: o.border,
      shadow: o.shadow,
      radius: Math.round(o.radius),
      padding: Math.round(o.padding),
    },
    gap: Math.round(o.gap),
  };
}
