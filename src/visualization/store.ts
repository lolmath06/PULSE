import { useCallback, useMemo, useSyncExternalStore } from 'react';
import type { HistoryRange } from '@/types/history';
import { DEFAULT_HISTORY_RANGE, isHistoryRange } from '@/types/history';
import type { DeepPartial, VisualizationConfig } from '@/visualization/config';
import { mergeConfig, normalizeConfig, styleOf } from '@/visualization/config';
import type { VisualizationPreset } from '@/visualization/presets';
import {
  BUILT_IN_PRESETS,
  DEFAULT_PRESET_ID,
  applyPreset as applyPresetTo,
  findPreset,
  resolvePreset,
} from '@/visualization/presets';

/**
 * Where visualization choices live between launches.
 *
 * # Why `localStorage`
 *
 * These are pure presentation preferences — a colour, a curve, a range — of a
 * single-user desktop app. `localStorage` lives in the webview's own data
 * directory inside PULSE's app data, is synchronous (a chart never flashes its
 * default style before the saved one arrives), and is shared by every window
 * of the same origin, which the Phase 11 widgets will be. None of it is sensitive
 * and none of it belongs in the metric database: history rows hold samples,
 * never how they are drawn.
 *
 * # Versioning
 *
 * The key carries the version (`pulse.visualization.v1`) and so does the
 * payload. A future format uses a **new** key and migrates from this one, so
 * it can never be clobbered by an older PULSE. Inside a payload, every chart's
 * configuration is re-read field by field ({@link normalizeConfig}): an
 * unknown property is ignored, an invalid one falls back alone, and a payload
 * that is not version 1 is ignored entirely rather than misread.
 *
 * No device or interface identifier is stored — only chart ids chosen by the
 * code (`cpu.total`, `network`), styles, preset names the user typed, and a
 * range.
 */

export const VISUALIZATION_STORAGE_KEY = 'pulse.visualization.v1';
export const VISUALIZATION_STORE_VERSION = 1;

export interface ChartVisualizationState {
  /** The preset the chart started from. */
  readonly presetId: string;
  /** True once anything was changed after choosing the preset. */
  readonly modified: boolean;
  readonly config: VisualizationConfig;
  readonly range: HistoryRange;
}

interface StoredChart {
  readonly presetId: string;
  readonly modified: boolean;
  readonly config: unknown;
  readonly range: unknown;
}

interface StoreShape {
  readonly version: number;
  readonly charts: Readonly<Record<string, StoredChart>>;
  readonly customPresets: readonly VisualizationPreset[];
}

const EMPTY_STORE: StoreShape = {
  version: VISUALIZATION_STORE_VERSION,
  charts: {},
  customPresets: [],
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Parses a stored payload, tolerating anything. */
export function parseStore(text: string | null): StoreShape {
  if (!text) return EMPTY_STORE;
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return EMPTY_STORE;
  }
  if (!isRecord(raw) || raw.version !== VISUALIZATION_STORE_VERSION) return EMPTY_STORE;

  const charts: Record<string, StoredChart> = {};
  if (isRecord(raw.charts)) {
    for (const [id, chart] of Object.entries(raw.charts)) {
      if (!isRecord(chart)) continue;
      charts[id] = {
        presetId: typeof chart.presetId === 'string' ? chart.presetId : DEFAULT_PRESET_ID,
        modified: chart.modified === true,
        config: chart.config,
        range: chart.range,
      };
    }
  }

  const customPresets: VisualizationPreset[] = [];
  if (Array.isArray(raw.customPresets)) {
    for (const preset of raw.customPresets.slice(0, 32)) {
      if (!isRecord(preset) || typeof preset.id !== 'string' || typeof preset.name !== 'string') {
        continue;
      }
      if (!preset.id.startsWith('custom:')) continue;
      customPresets.push({
        id: preset.id,
        name: preset.name.slice(0, 40),
        description: 'Saved by you.',
        style: styleOf(normalizeConfig(preset.style)),
        custom: true,
      });
    }
  }

  return { version: VISUALIZATION_STORE_VERSION, charts, customPresets };
}

let store: StoreShape | null = null;
const subscribers = new Set<() => void>();

function readStorage(): string | null {
  try {
    return globalThis.localStorage?.getItem(VISUALIZATION_STORAGE_KEY) ?? null;
  } catch {
    return null;
  }
}

function current(): StoreShape {
  if (!store) store = parseStore(readStorage());
  return store;
}

function commit(next: StoreShape) {
  store = next;
  try {
    globalThis.localStorage?.setItem(VISUALIZATION_STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Storage full or disabled: the choice still applies for this session.
  }
  for (const notify of [...subscribers]) notify();
}

function subscribe(listener: () => void) {
  subscribers.add(listener);
  return () => {
    subscribers.delete(listener);
  };
}

/** Forgets the in-memory copy so the next read comes from storage. For tests. */
export function reloadVisualizationStoreForTesting() {
  store = null;
}

/**
 * Reads one chart's state from a store, resolving it against the chart's own
 * defaults. Pure, so the round trip is testable without React.
 */
export function chartStateFrom(
  shape: StoreShape,
  chartId: string,
  chartDefaults: DeepPartial<VisualizationConfig>,
  defaultPresetId: string = DEFAULT_PRESET_ID,
): ChartVisualizationState {
  const stored = shape.charts[chartId];
  const presetId =
    stored && findPreset(stored.presetId, shape.customPresets) ? stored.presetId : defaultPresetId;
  const baseline = resolvePreset(findPreset(presetId, shape.customPresets), chartDefaults);
  return {
    presetId,
    modified: stored?.modified ?? false,
    config: stored ? normalizeConfig(stored.config, baseline) : baseline,
    range: stored && isHistoryRange(stored.range) ? stored.range : DEFAULT_HISTORY_RANGE,
  };
}

export interface ChartVisualization extends ChartVisualizationState {
  /** Every preset the user can pick, built-in first. */
  readonly presets: readonly VisualizationPreset[];
  /** Merges a partial change; the chart becomes *modified*. */
  readonly update: (patch: DeepPartial<VisualizationConfig>) => void;
  readonly setRange: (range: HistoryRange) => void;
  readonly applyPreset: (presetId: string) => void;
  /** Back to the current preset, exactly as it was chosen. */
  readonly reset: () => void;
  /** Saves the current style as a named preset and selects it. */
  readonly saveAsPreset: (name: string) => void;
  readonly deletePreset: (presetId: string) => void;
}

/**
 * One chart's visualization state, persisted.
 *
 * `chartDefaults` must be stable (a module constant) — it is a dependency.
 */
export function useChartVisualization(
  chartId: string,
  chartDefaults: DeepPartial<VisualizationConfig>,
  defaultPresetId: string = DEFAULT_PRESET_ID,
): ChartVisualization {
  const shape = useSyncExternalStore(subscribe, current, current);
  const state = useMemo(
    () => chartStateFrom(shape, chartId, chartDefaults, defaultPresetId),
    [shape, chartId, chartDefaults, defaultPresetId],
  );

  const write = useCallback(
    (next: ChartVisualizationState) => {
      const latest = current();
      commit({
        ...latest,
        charts: {
          ...latest.charts,
          [chartId]: {
            presetId: next.presetId,
            modified: next.modified,
            config: next.config,
            range: next.range,
          },
        },
      });
    },
    [chartId],
  );

  const read = useCallback(
    () => chartStateFrom(current(), chartId, chartDefaults, defaultPresetId),
    [chartId, chartDefaults, defaultPresetId],
  );

  const update = useCallback(
    (patch: DeepPartial<VisualizationConfig>) => {
      const now = read();
      write({
        ...now,
        modified: true,
        config: normalizeConfig(mergeConfig(now.config, patch), now.config),
      });
    },
    [read, write],
  );

  const setRange = useCallback((range: HistoryRange) => write({ ...read(), range }), [read, write]);

  const applyPreset = useCallback(
    (presetId: string) => {
      const now = read();
      const preset = findPreset(presetId, current().customPresets);
      if (!preset) return;
      write({
        ...now,
        presetId,
        modified: false,
        config: applyPresetTo(preset, chartDefaults, now.config),
      });
    },
    [read, write, chartDefaults],
  );

  const reset = useCallback(() => {
    const now = read();
    const preset = findPreset(now.presetId, current().customPresets);
    write({ ...now, modified: false, config: resolvePreset(preset, chartDefaults) });
  }, [read, write, chartDefaults]);

  const saveAsPreset = useCallback(
    (name: string) => {
      const trimmed = name.trim().slice(0, 40);
      if (!trimmed) return;
      const now = read();
      const preset: VisualizationPreset = {
        id: `custom:${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`,
        name: trimmed,
        description: 'Saved by you.',
        style: styleOf(now.config),
        custom: true,
      };
      const latest = current();
      commit({
        ...latest,
        customPresets: [...latest.customPresets, preset],
        charts: {
          ...latest.charts,
          [chartId]: { presetId: preset.id, modified: false, config: now.config, range: now.range },
        },
      });
    },
    [read, chartId],
  );

  const deletePreset = useCallback((presetId: string) => {
    const latest = current();
    commit({ ...latest, customPresets: latest.customPresets.filter((p) => p.id !== presetId) });
  }, []);

  const presets = useMemo(
    () => [...BUILT_IN_PRESETS, ...shape.customPresets],
    [shape.customPresets],
  );

  return { ...state, presets, update, setRange, applyPreset, reset, saveAsPreset, deletePreset };
}
