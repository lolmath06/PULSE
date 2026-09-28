import { beforeEach, describe, expect, it } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import {
  VISUALIZATION_STORAGE_KEY,
  chartStateFrom,
  parseStore,
  reloadVisualizationStoreForTesting,
  useChartVisualization,
} from '@/visualization/store';
import type { DeepPartial, VisualizationConfig } from '@/visualization/config';
import {
  flushUiConfig,
  initUiConfig,
  memoryBackend,
  readSection,
  resetUiConfigForTesting,
} from '@/config/uiConfig';
import { migrateLocalVisualizationPreferences } from '@/config/migrations';

const CPU_DEFAULTS: DeepPartial<VisualizationConfig> = {
  renderer: 'area',
  scale: { mode: 'fixed', min: 0, max: 100 },
};

type Backend = ReturnType<typeof memoryBackend>;

/** A fresh process on the same saved configuration. */
async function relaunch(backend: Backend) {
  reloadVisualizationStoreForTesting();
  await act(() => initUiConfig(backend));
  return renderHook(() => useChartVisualization('cpu.total', CPU_DEFAULTS));
}

let backend: Backend;

beforeEach(() => {
  localStorage.clear();
  resetUiConfigForTesting();
  reloadVisualizationStoreForTesting();
  backend = memoryBackend();
});

describe('visualization preferences', () => {
  it('start from the chart defaults under the Clean preset', async () => {
    const { result } = await relaunch(backend);
    expect(result.current.presetId).toBe('clean');
    expect(result.current.modified).toBe(false);
    expect(result.current.config.renderer).toBe('area');
    expect(result.current.range).toBe('15m');
  });

  it('survive a relaunch exactly: style, colours, size, range', async () => {
    const first = await relaunch(backend);
    act(() => {
      first.result.current.update({
        renderer: 'area',
        colors: { mode: 'manual', primary: '#ff8800' },
        axes: { grid: false },
        size: { preset: 'large' },
        fill: { mode: 'solid', opacity: 0.6 },
      });
    });
    act(() => first.result.current.setRange('6h'));
    const saved = first.result.current.config;
    await act(() => flushUiConfig());
    first.unmount();

    resetUiConfigForTesting();
    const second = await relaunch(backend);

    expect(second.result.current.config).toEqual(saved);
    expect(second.result.current.config.colors.primary).toBe('#ff8800');
    expect(second.result.current.config.axes.grid).toBe(false);
    expect(second.result.current.range).toBe('6h');
    expect(second.result.current.modified).toBe(true);
  });

  it('are shared: a change from another window reaches this one', async () => {
    const { result } = await relaunch(backend);
    act(() =>
      backend.peer('visualization', {
        version: 1,
        charts: { 'cpu.total': { presetId: 'gaming', modified: false, config: {}, range: '1h' } },
        customPresets: [],
      }),
    );
    expect(result.current.presetId).toBe('gaming');
    expect(result.current.range).toBe('1h');
  });

  it('a burst of changes is saved once', async () => {
    const { result } = await relaunch(backend);
    for (let width = 1; width <= 8; width += 1) {
      act(() => result.current.update({ line: { width } }));
    }
    await act(() => flushUiConfig());
    expect(backend.saves).toBe(1);
  });

  it('a modified preset reads as modified, and Reset returns to it', async () => {
    const { result } = await relaunch(backend);
    act(() => result.current.applyPreset('gaming'));
    expect(result.current.presetId).toBe('gaming');
    expect(result.current.modified).toBe(false);
    const pristine = result.current.config;

    act(() => result.current.update({ line: { width: 1 } }));
    expect(result.current.modified).toBe(true);
    expect(result.current.presetId).toBe('gaming');

    act(() => result.current.reset());
    expect(result.current.modified).toBe(false);
    expect(result.current.config).toEqual(pristine);
  });

  it('saves a named preset that other charts can use', async () => {
    const { result } = await relaunch(backend);
    act(() => result.current.update({ colors: { mode: 'manual', primary: '#00ff00' } }));
    act(() => result.current.saveAsPreset('My green'));

    const other = renderHook(() => useChartVisualization('memory', { renderer: 'line' }));
    const mine = other.result.current.presets.find((preset) => preset.name === 'My green');
    expect(mine?.custom).toBe(true);

    act(() => other.result.current.applyPreset(mine!.id));
    expect(other.result.current.config.colors.primary).toBe('#00ff00');
    expect(other.result.current.config.renderer).toBe('area');
  });

  it('never stores a source identifier', async () => {
    const { result } = await relaunch(backend);
    act(() => result.current.update({ line: { width: 3 } }));
    expect(JSON.stringify(readSection('visualization'))).not.toMatch(
      /mac-|serial-|wwid-|network:|storage:/,
    );
  });

  it('an unknown store version falls back to defaults instead of misreading', async () => {
    backend = memoryBackend({
      visualization: { version: 99, charts: { 'cpu.total': { config: { renderer: 'bar' } } } },
    });
    const { result } = await relaunch(backend);
    expect(result.current.config.renderer).toBe('area');
  });

  it('tolerates unknown properties and corrupt JSON', () => {
    expect(parseStore('{not json').charts).toEqual({});

    const shape = parseStore({
      version: 1,
      futureField: true,
      charts: {
        'cpu.total': {
          presetId: 'minimal',
          modified: true,
          range: '24h',
          config: { renderer: 'gauge', sparkles: 3 },
        },
      },
    });
    const state = chartStateFrom(shape, 'cpu.total', CPU_DEFAULTS);
    expect(state.config.renderer).toBe('gauge');
    expect(state.presetId).toBe('minimal');
    expect(state.range).toBe('24h');
    expect(state.config).not.toHaveProperty('sparkles');
  });

  it('an unknown preset id falls back to the default preset', () => {
    const shape = parseStore({ version: 1, charts: { x: { presetId: 'vaporwave', config: {} } } });
    expect(chartStateFrom(shape, 'x', {}).presetId).toBe('clean');
  });
});

describe('Phase 10 migration', () => {
  const phase10 = {
    version: 1,
    charts: {
      'cpu.total': {
        presetId: 'neon',
        modified: true,
        config: { renderer: 'gauge', line: { width: 4 } },
        range: '24h',
      },
    },
    customPresets: [{ id: 'custom:abc', name: 'Mine', style: { line: { width: 6 } } }],
  };

  it('moves localStorage preferences into the shared configuration once', async () => {
    localStorage.setItem(VISUALIZATION_STORAGE_KEY, JSON.stringify(phase10));
    const { result } = await relaunch(backend);

    expect(result.current.presetId).toBe('neon');
    expect(result.current.config.renderer).toBe('gauge');
    expect(result.current.range).toBe('24h');
    expect(result.current.presets.some((preset) => preset.name === 'Mine')).toBe(true);
    expect(localStorage.getItem(VISUALIZATION_STORAGE_KEY)).not.toBeNull();
  });

  it('never overwrites a shared section that already exists', () => {
    const writes: unknown[] = [];
    const storage = { getItem: () => JSON.stringify(phase10) };
    const migrated = migrateLocalVisualizationPreferences(
      () => ({ version: 1, charts: {}, customPresets: [] }),
      (_section, value) => writes.push(value),
      storage,
    );
    expect(migrated).toBe(false);
    expect(writes).toEqual([]);
  });

  it('ignores an empty or unreadable Phase 10 payload', () => {
    const writes: unknown[] = [];
    for (const text of [null, '{bad', JSON.stringify({ version: 1, charts: {} })]) {
      migrateLocalVisualizationPreferences(
        () => undefined,
        (_s, value) => writes.push(value),
        { getItem: () => text },
      );
    }
    expect(writes).toEqual([]);
  });
});
