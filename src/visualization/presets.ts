import type { DeepPartial, VisualizationConfig } from '@/visualization/config';
import { BASE_CONFIG, mergeConfig } from '@/visualization/config';
import { t } from '@/i18n/i18n';

/**
 * Visual presets: a **starting point**, never a lock.
 *
 * A preset is a partial style laid over PULSE's base and the chart's own
 * defaults. Choosing one sets every option it names; the user can then change
 * anything, at which point the chart reads *Custom (from <preset>)*.
 *
 * Presets never set a scale, a precision, colour thresholds or series names —
 * those belong to the metric, not to a look — and only Compact changes the
 * renderer, because being a micro-graph is what Compact *is*.
 */
export interface VisualizationPreset {
  readonly id: string;
  /**
   * A saved preset's own name. Built-in presets have none: theirs are
   * translations keyed by id (`visualization.presets.<id>.name`).
   */
  readonly name?: string;
  readonly description?: string;
  readonly style: DeepPartial<VisualizationConfig>;
  /** Saved by the user rather than shipped. */
  readonly custom?: boolean;
}

export const DEFAULT_PRESET_ID = 'clean';

export const BUILT_IN_PRESETS: readonly VisualizationPreset[] = [
  {
    id: 'clean',
    style: {
      line: { width: 2, curve: 'smooth', points: 'none' },
      fill: { mode: 'gradient', opacity: 0.28 },
      colors: { mode: 'theme' },
      background: { mode: 'solid', opacity: 1 },
      frame: { border: 'thin', radius: 10, shadow: 'none' },
      text: { scale: 1, weight: 'medium', showLabel: true, showUnit: true },
      axes: { x: true, y: true, grid: true },
      smoothing: 'none',
      display: {
        legend: true,
        tooltip: true,
        current: true,
        min: true,
        max: true,
        average: true,
        compact: false,
      },
    },
  },
  {
    id: 'minimal',
    style: {
      line: { width: 1.25, curve: 'smooth', points: 'none' },
      fill: { mode: 'none', opacity: 0 },
      colors: { mode: 'theme' },
      background: { mode: 'none', opacity: 0 },
      frame: { border: 'none', radius: 0, shadow: 'none' },
      text: { scale: 0.92, weight: 'regular', showLabel: true, showUnit: true },
      axes: { x: false, y: false, grid: false },
      smoothing: 'light',
      display: {
        legend: false,
        tooltip: true,
        current: true,
        min: false,
        max: false,
        average: false,
        compact: false,
      },
    },
  },
  {
    id: 'technical',
    style: {
      line: { width: 1.5, curve: 'straight', points: 'small' },
      fill: { mode: 'none', opacity: 0 },
      colors: {
        mode: 'manual',
        primary: '#7cc4fa',
        secondary: '#f6c177',
        text: '#c9d4e0',
        grid: '#2c3642',
        background: '#0f1318',
        border: '#3a4552',
      },
      background: { mode: 'solid', opacity: 1 },
      frame: { border: 'thin', radius: 4, shadow: 'none' },
      text: { scale: 0.9, weight: 'regular', showLabel: true, showUnit: true },
      axes: { x: true, y: true, grid: true },
      smoothing: 'none',
      display: {
        legend: true,
        tooltip: true,
        current: true,
        min: true,
        max: true,
        average: true,
        compact: false,
      },
    },
  },
  {
    id: 'gaming',
    style: {
      line: { width: 3, curve: 'smooth', points: 'none' },
      fill: { mode: 'gradient', opacity: 0.55 },
      colors: {
        mode: 'manual',
        primary: '#ff4d6d',
        secondary: '#ffd166',
        text: '#fff1f3',
        grid: '#3a2330',
        background: '#1a0f18',
        border: '#5c2a3d',
        gradientStart: '#2a1020',
        gradientEnd: '#0d1013',
      },
      background: { mode: 'gradient', opacity: 1 },
      frame: { border: 'thin', radius: 14, shadow: 'glow' },
      text: { scale: 1.1, weight: 'bold', showLabel: true, showUnit: true },
      axes: { x: false, y: true, grid: true },
      smoothing: 'light',
      display: {
        legend: true,
        tooltip: true,
        current: true,
        min: false,
        max: true,
        average: false,
        compact: false,
      },
    },
  },
  {
    id: 'compact',
    style: {
      renderer: 'sparkline',
      size: { preset: 'small', width: null, height: null },
      line: { width: 1.5, curve: 'smooth', points: 'none' },
      fill: { mode: 'solid', opacity: 0.16 },
      colors: { mode: 'theme' },
      background: { mode: 'solid', opacity: 0.6 },
      frame: { border: 'none', radius: 6, shadow: 'none' },
      text: { scale: 0.8, weight: 'medium', showLabel: true, showUnit: true },
      axes: { x: false, y: false, grid: false },
      smoothing: 'none',
      display: {
        legend: false,
        tooltip: true,
        current: true,
        min: false,
        max: false,
        average: false,
        compact: true,
      },
    },
  },
  {
    id: 'neon',
    style: {
      line: { width: 2.5, curve: 'smooth', points: 'none' },
      fill: { mode: 'gradient', opacity: 0.4 },
      colors: {
        mode: 'manual',
        primary: '#00e5ff',
        secondary: '#ff3df2',
        text: '#e8fbff',
        grid: '#10303a',
        background: '#05080b',
        border: '#0f4a57',
      },
      background: { mode: 'solid', opacity: 1 },
      frame: { border: 'thin', radius: 12, shadow: 'glow' },
      text: { scale: 1, weight: 'medium', showLabel: true, showUnit: true },
      axes: { x: false, y: true, grid: true },
      smoothing: 'light',
    },
  },
  {
    id: 'transparent',
    style: {
      line: { width: 2, curve: 'smooth', points: 'none' },
      fill: { mode: 'solid', opacity: 0.18 },
      colors: { mode: 'theme' },
      background: { mode: 'none', opacity: 0 },
      frame: { border: 'none', radius: 0, shadow: 'none' },
      axes: { x: false, y: false, grid: false },
      display: {
        legend: false,
        tooltip: true,
        current: true,
        min: false,
        max: false,
        average: false,
        compact: false,
      },
    },
  },
];

/** A preset's name as shown: a built-in one translated, a saved one verbatim. */
export function presetName(preset: VisualizationPreset): string {
  return preset.custom ? (preset.name ?? '') : t(`visualization.presets.${preset.id}.name`);
}

export function presetDescription(preset: VisualizationPreset): string {
  return preset.custom
    ? t('visualization.savedByYou')
    : t(`visualization.presets.${preset.id}.description`);
}

/** Finds a preset among the built-in ones and `custom`. */
export function findPreset(
  id: string,
  custom: readonly VisualizationPreset[] = [],
): VisualizationPreset | undefined {
  return BUILT_IN_PRESETS.find((preset) => preset.id === id) ?? custom.find((p) => p.id === id);
}

/**
 * The configuration a chart has with `preset` and nothing else changed:
 * PULSE's base, then the chart's own defaults, then the preset.
 *
 * This is what *Reset* returns to.
 */
export function resolvePreset(
  preset: VisualizationPreset | undefined,
  chartDefaults: DeepPartial<VisualizationConfig>,
): VisualizationConfig {
  return mergeConfig(mergeConfig(BASE_CONFIG, chartDefaults), preset?.style);
}

/**
 * Applies `preset` to a chart the user has already shaped.
 *
 * Keeps what belongs to the chart — its renderer (unless the preset is about
 * the renderer), size, scale, precision, thresholds and series names — and
 * takes everything else from the preset.
 */
export function applyPreset(
  preset: VisualizationPreset,
  chartDefaults: DeepPartial<VisualizationConfig>,
  current: VisualizationConfig,
): VisualizationConfig {
  const kept: DeepPartial<VisualizationConfig> = {
    renderer: current.renderer,
    size: current.size,
    scale: current.scale,
    text: { decimals: current.text.decimals },
    colors: { thresholds: current.colors.thresholds, series: current.colors.series },
  };
  return mergeConfig(mergeConfig(mergeConfig(BASE_CONFIG, chartDefaults), kept), preset.style);
}
