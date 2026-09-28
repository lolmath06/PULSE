import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { readSection, subscribeUiConfig, writeSection } from '@/config/uiConfig';
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
 * # Phase 11: the shared configuration
 *
 * Phase 10 kept these preferences in `localStorage`. With several windows
 * (main, Mini, overlays) and different webview origins in development and
 * release builds, that would have meant several diverging copies. They now
 * live in the `visualization` section of the shared UI configuration
 * (`src/config/uiConfig.ts`, backed by one file in the app's config
 * directory) and every window sees the same thing.
 *
 * The first time a window loads a configuration without that section, the
 * Phase 10 `localStorage` payload (`pulse.visualization.v1`) is migrated into
 * it — see `src/config/migrations.ts`. The old key is left in place, unread.
 *
 * # Versioning
 *
 * The payload carries `version: 1`. Every chart's configuration is re-read
 * field by field ({@link normalizeConfig}): an unknown property is ignored, an
 * invalid one falls back alone, and a payload that is not version 1 is ignored
 * entirely rather than misread.
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

/** Parses a stored payload — JSON text or an object — tolerating anything. */
export function parseStore(input: unknown): StoreShape {
  if (input === null || input === undefined || input === '') return EMPTY_STORE;
  let raw: unknown = input;
  if (typeof input === 'string') {
    try {
      raw = JSON.parse(input);
    } catch {
      return EMPTY_STORE;
    }
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

let parsed: { raw: unknown; shape: StoreShape } | null = null;

function current(): StoreShape {
  const raw = readSection('visualization');
  if (!parsed || parsed.raw !== raw) parsed = { raw, shape: parseStore(raw) };
  return parsed.shape;
}

function commit(next: StoreShape) {
  writeSection('visualization', next);
}

function subscribe(listener: () => void) {
  return subscribeUiConfig(listener);
}

/** Forgets the parsed copy. For tests. */
export function reloadVisualizationStoreForTesting() {
  parsed = null;
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

/** Saves `config`'s style as a named preset available to every chart and widget. */
export function saveCustomPreset(name: string, config: VisualizationConfig): string | null {
  const trimmed = name.trim().slice(0, 40);
  if (!trimmed) return null;
  const preset: VisualizationPreset = {
    id: `custom:${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`,
    name: trimmed,
    description: 'Saved by you.',
    style: styleOf(config),
    custom: true,
  };
  const latest = current();
  commit({ ...latest, customPresets: [...latest.customPresets, preset] });
  return preset.id;
}

/** Deletes a user preset. Built-in presets cannot be deleted. */
export function deleteCustomPreset(presetId: string) {
  const latest = current();
  commit({ ...latest, customPresets: latest.customPresets.filter((p) => p.id !== presetId) });
}

/** Renames a user preset. */
export function renameCustomPreset(presetId: string, name: string) {
  const trimmed = name.trim().slice(0, 40);
  if (!trimmed) return;
  const latest = current();
  commit({
    ...latest,
    customPresets: latest.customPresets.map((p) =>
      p.id === presetId ? { ...p, name: trimmed } : p,
    ),
  });
}

/** Every preset: built-in first, then the user's. */
export function usePresets(): readonly VisualizationPreset[] {
  const shape = useSyncExternalStore(subscribe, current, current);
  return useMemo(() => [...BUILT_IN_PRESETS, ...shape.customPresets], [shape.customPresets]);
}

/** Looks a preset up among built-in and user presets. */
export function lookupPreset(presetId: string): VisualizationPreset | undefined {
  return findPreset(presetId, current().customPresets);
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
