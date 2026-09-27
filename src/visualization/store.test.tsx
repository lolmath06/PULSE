import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import {
  VISUALIZATION_STORAGE_KEY,
  chartStateFrom,
  parseStore,
  reloadVisualizationStoreForTesting,
  useChartVisualization,
} from '@/visualization/store';
import type { DeepPartial, VisualizationConfig } from '@/visualization/config';

const CPU_DEFAULTS: DeepPartial<VisualizationConfig> = {
  renderer: 'area',
  scale: { mode: 'fixed', min: 0, max: 100 },
};

function relaunch() {
  // A fresh process: the in-memory copy is gone, storage remains.
  reloadVisualizationStoreForTesting();
  return renderHook(() => useChartVisualization('cpu.total', CPU_DEFAULTS));
}

beforeEach(() => {
  localStorage.clear();
  reloadVisualizationStoreForTesting();
});

afterEach(() => {
  localStorage.clear();
});

describe('visualization preferences', () => {
  it('start from the chart defaults under the Clean preset', () => {
    const { result } = relaunch();
    expect(result.current.presetId).toBe('clean');
    expect(result.current.modified).toBe(false);
    expect(result.current.config.renderer).toBe('area');
    expect(result.current.range).toBe('15m');
  });

  it('survive a relaunch exactly: style, colours, size, range', () => {
    const first = relaunch();
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
    first.unmount();

    const second = relaunch();

    expect(second.result.current.config).toEqual(saved);
    expect(second.result.current.config.colors.primary).toBe('#ff8800');
    expect(second.result.current.config.axes.grid).toBe(false);
    expect(second.result.current.range).toBe('6h');
    expect(second.result.current.modified).toBe(true);
  });

  it('a modified preset reads as modified, and Reset returns to it', () => {
    const { result } = relaunch();
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

  it('saves a named preset that other charts can use', () => {
    const { result } = relaunch();
    act(() => result.current.update({ colors: { mode: 'manual', primary: '#00ff00' } }));
    act(() => result.current.saveAsPreset('My green'));

    const other = renderHook(() => useChartVisualization('memory', { renderer: 'line' }));
    const mine = other.result.current.presets.find((preset) => preset.name === 'My green');
    expect(mine?.custom).toBe(true);

    act(() => other.result.current.applyPreset(mine!.id));
    expect(other.result.current.config.colors.primary).toBe('#00ff00');
    expect(other.result.current.config.renderer).toBe('area');
  });

  it('never stores a source identifier', () => {
    const { result } = relaunch();
    act(() => result.current.update({ line: { width: 3 } }));
    const stored = localStorage.getItem(VISUALIZATION_STORAGE_KEY) ?? '';
    expect(stored).not.toMatch(/mac-|serial-|wwid-|network:|storage:/);
  });

  it('an unknown store version falls back to defaults instead of misreading', () => {
    localStorage.setItem(
      VISUALIZATION_STORAGE_KEY,
      JSON.stringify({ version: 99, charts: { 'cpu.total': { config: { renderer: 'bar' } } } }),
    );
    const { result } = relaunch();
    expect(result.current.config.renderer).toBe('area');
  });

  it('tolerates unknown properties and corrupt JSON', () => {
    localStorage.setItem(VISUALIZATION_STORAGE_KEY, '{not json');
    expect(relaunch().result.current.config.renderer).toBe('area');

    const shape = parseStore(
      JSON.stringify({
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
      }),
    );
    const state = chartStateFrom(shape, 'cpu.total', CPU_DEFAULTS);
    expect(state.config.renderer).toBe('gauge');
    expect(state.presetId).toBe('minimal');
    expect(state.range).toBe('24h');
    expect(state.config).not.toHaveProperty('sparkles');
  });

  it('an unknown preset id falls back to the default preset', () => {
    const shape = parseStore(
      JSON.stringify({ version: 1, charts: { x: { presetId: 'vaporwave', config: {} } } }),
    );
    expect(chartStateFrom(shape, 'x', {}).presetId).toBe('clean');
  });
});
