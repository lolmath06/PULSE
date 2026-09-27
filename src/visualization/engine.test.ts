import { describe, expect, it } from 'vitest';
import {
  BASE_CONFIG,
  mergeConfig,
  normalizeConfig,
  scaleError,
  styleOf,
  VISUALIZATION_CONFIG_VERSION,
} from '@/visualization/config';
import type { VisualizationConfig } from '@/visualization/config';
import { BUILT_IN_PRESETS, applyPreset, findPreset, resolvePreset } from '@/visualization/presets';
import { nearestPoint, smoothValues, splitSegments, summarize } from '@/visualization/series';
import { niceTicks, timeTicks, yDomain } from '@/visualization/scale';
import { formatParts, formatValue } from '@/visualization/format';
import { bandFor, PERCENT_THRESHOLDS, resolveColors, thresholdStops } from '@/visualization/color';
import { gaugeBounds, rendererSupport, RENDERERS } from '@/visualization/registry';
import {
  CELSIUS_META,
  PERCENT_META,
  RATE_META,
  T0,
  configWith,
  points,
  series,
} from '@/test/visualization';

describe('visualization config', () => {
  it('round-trips through JSON to exactly the same configuration', () => {
    const config = configWith({
      renderer: 'area',
      line: { width: 3.5, curve: 'stepped', points: 'visible' },
      fill: { mode: 'gradient', opacity: 0.4 },
      colors: {
        mode: 'manual',
        primary: '#123456',
        secondary: '#abcdef',
        series: { '1': { color: '#ff0000', label: 'Upload' } },
        thresholds: PERCENT_THRESHOLDS,
      },
      background: { mode: 'gradient', opacity: 0.3 },
      scale: { mode: 'fixed', min: 0, max: 100 },
      text: { decimals: 2, scale: 1.2 },
      size: { preset: 'custom', width: 320, height: 90 },
    });

    const reread = normalizeConfig(JSON.parse(JSON.stringify(config)));

    expect(reread).toEqual(config);
  });

  it('ignores unknown future properties and keeps the known ones', () => {
    const stored = {
      ...JSON.parse(JSON.stringify(configWith({ renderer: 'bar' }))),
      hologram: { enabled: true },
      line: { width: 4, curve: 'smooth', points: 'none', glowRadius: 12 },
    };

    const config = normalizeConfig(stored);

    expect(config.renderer).toBe('bar');
    expect(config.line.width).toBe(4);
    expect(config).not.toHaveProperty('hologram');
    expect(config.line).not.toHaveProperty('glowRadius');
  });

  it('falls back field by field for invalid values', () => {
    const config = normalizeConfig({
      renderer: 'pie3d',
      line: { width: 400, curve: 'wiggly' },
      fill: { opacity: -3 },
      colors: { primary: 'red', text: '#FFFFFF' },
      text: { decimals: 2.5 },
    });

    expect(config.renderer).toBe(BASE_CONFIG.renderer);
    expect(config.line.width).toBe(8);
    expect(config.line.curve).toBe(BASE_CONFIG.line.curve);
    expect(config.fill.opacity).toBe(0);
    expect(config.colors.primary).toBe(BASE_CONFIG.colors.primary);
    expect(config.colors.text).toBe('#ffffff');
    expect(config.text.decimals).toBe(BASE_CONFIG.text.decimals);
    expect(config.version).toBe(VISUALIZATION_CONFIG_VERSION);
  });

  it('refuses a fixed scale whose maximum is not above its minimum', () => {
    expect(scaleError(10, 10)).toMatch(/greater/);
    expect(scaleError(50, 10)).toMatch(/greater/);
    expect(scaleError(null, 10)).toMatch(/both/i);
    expect(scaleError(0, 100)).toBeNull();

    const config = normalizeConfig({ scale: { mode: 'fixed', min: 80, max: 20 } });
    expect(config.scale.mode).toBe('auto');
  });

  it('treats garbage as the defaults, never throwing', () => {
    for (const raw of [null, 42, 'config', [], { size: 'huge' }]) {
      expect(normalizeConfig(raw)).toEqual(BASE_CONFIG);
    }
  });

  it('keeps only style when saving a preset', () => {
    const style = styleOf(
      configWith({ scale: { mode: 'fixed', min: 0, max: 100 }, text: { decimals: 3 } }),
    );
    expect(style).not.toHaveProperty('scale');
    expect(style.text).not.toHaveProperty('decimals');
    expect(style.colors).not.toHaveProperty('thresholds');
    expect(style.line).toEqual(BASE_CONFIG.line);
  });
});

describe('presets', () => {
  const resolved = (id: string) => resolvePreset(findPreset(id), {});

  it('ships Clean, Minimal, Technical, Gaming and Compact, and more', () => {
    const ids = BUILT_IN_PRESETS.map((preset) => preset.id);
    expect(ids).toEqual(
      expect.arrayContaining(['clean', 'minimal', 'technical', 'gaming', 'compact']),
    );
  });

  it('are genuinely different in real characteristics', () => {
    const signature = (config: VisualizationConfig) =>
      JSON.stringify([
        config.line,
        config.fill,
        config.axes,
        config.background,
        config.frame,
        config.text.scale,
        config.colors.mode === 'theme' ? 'theme' : config.colors.primary,
        config.display.compact,
      ]);
    const signatures = BUILT_IN_PRESETS.map((preset) => signature(resolved(preset.id)));
    expect(new Set(signatures).size).toBe(BUILT_IN_PRESETS.length);

    expect(resolved('minimal').axes).toEqual({ x: false, y: false, grid: false });
    expect(resolved('minimal').background.mode).toBe('none');
    expect(resolved('technical').line.points).toBe('small');
    expect(resolved('technical').line.curve).toBe('straight');
    expect(resolved('gaming').line.width).toBeGreaterThan(resolved('clean').line.width);
    expect(resolved('gaming').frame.shadow).toBe('glow');
    expect(resolved('compact').renderer).toBe('sparkline');
    expect(resolved('compact').display.compact).toBe(true);
    expect(resolved('transparent').background.opacity).toBe(0);
  });

  it('applying a preset keeps what belongs to the chart', () => {
    const current = configWith({
      renderer: 'gauge',
      scale: { mode: 'fixed', min: 0, max: 100 },
      text: { decimals: 2 },
      colors: { thresholds: PERCENT_THRESHOLDS, series: { '0': { label: 'Total' } } },
    });

    const next = applyPreset(findPreset('gaming')!, { renderer: 'area' }, current);

    expect(next.renderer).toBe('gauge');
    expect(next.scale).toEqual(current.scale);
    expect(next.text.decimals).toBe(2);
    expect(next.colors.thresholds).toEqual(PERCENT_THRESHOLDS);
    expect(next.colors.series['0']?.label).toBe('Total');
    expect(next.line.width).toBe(3);
  });

  it('only Compact changes the renderer', () => {
    const current = configWith({ renderer: 'bar' });
    for (const preset of BUILT_IN_PRESETS) {
      const next = applyPreset(preset, {}, current);
      expect(next.renderer).toBe(preset.id === 'compact' ? 'sparkline' : 'bar');
    }
  });

  it('a chart default lies under the preset', () => {
    expect(resolvePreset(findPreset('clean'), { renderer: 'area' }).renderer).toBe('area');
  });
});

describe('series', () => {
  it('splits 00, 05, 10, +2h, +2h05 into two segments', () => {
    const twoHours = 2 * 3_600_000;
    const data = [
      { t: T0, v: 1 },
      { t: T0 + 5_000, v: 2 },
      { t: T0 + 10_000, v: 3 },
      { t: T0 + 10_000 + twoHours, v: 4 },
      { t: T0 + 15_000 + twoHours, v: 5 },
    ];

    const segments = splitSegments(data, 15_000);

    expect(segments).toHaveLength(2);
    expect(segments[0]!.map((p) => p.v)).toEqual([1, 2, 3]);
    expect(segments[1]!.map((p) => p.v)).toEqual([4, 5]);
  });

  it('breaks the line when the clock went backwards', () => {
    const segments = splitSegments(
      [
        { t: T0, v: 1 },
        { t: T0 + 5_000, v: 2 },
        { t: T0 - 60_000, v: 3 },
      ],
      15_000,
    );
    expect(segments).toHaveLength(2);
  });

  it('smoothing is visual only and never crosses a gap', () => {
    const data = points(9, (i) => (i === 4 ? 100 : 0));
    const before = JSON.stringify(data);

    const smoothed = smoothValues(data, 'light');

    expect(JSON.stringify(data)).toBe(before);
    expect(smoothed[4]).toBeCloseTo(100 / 3);
    expect(smoothValues(data, 'none')).toEqual(data.map((p) => p.v));
  });

  it('summaries keep bucket extremes and weight averages by sample count', () => {
    const summary = summarize(
      series('cpu', 'CPU', [
        { t: T0, v: 10, min: 5, max: 90, n: 24 },
        { t: T0 + 120_000, v: 40, min: 30, max: 50, n: 8 },
      ]),
    );

    expect(summary.min).toBe(5);
    expect(summary.max).toBe(90);
    expect(summary.average).toBeCloseTo((10 * 24 + 40 * 8) / 32);
    expect(summary.current).toBe(40);
  });

  it('an empty series summarises to nothing, not to zero', () => {
    const summary = summarize({ id: 'x', label: 'X', points: [], latest: null });
    expect(summary).toMatchObject({ current: null, min: null, max: null, average: null });
  });

  it('nearestPoint shows nothing inside a gap', () => {
    const data = [
      { t: T0, v: 1 },
      { t: T0 + 3_600_000, v: 2 },
    ];
    expect(nearestPoint(data, T0 + 2_000, 7_500)?.v).toBe(1);
    expect(nearestPoint(data, T0 + 1_800_000, 7_500)).toBeNull();
  });
});

describe('scales', () => {
  it('a fixed scale is used exactly', () => {
    const config = configWith({ scale: { mode: 'fixed', min: 0, max: 100 } });
    expect(yDomain([series('a', 'A', points(3))], config, PERCENT_META)).toEqual({
      min: 0,
      max: 100,
    });
  });

  it('auto keeps a percentage inside 0–100 and starts rates at zero', () => {
    const auto = configWith();
    const percent = yDomain(
      [
        series(
          'a',
          'A',
          points(5, () => 99),
        ),
      ],
      auto,
      PERCENT_META,
    );
    expect(percent.max).toBeLessThanOrEqual(100);
    expect(percent.min).toBe(0);

    const rate = yDomain(
      [
        series(
          'a',
          'A',
          points(5, (i) => 1000 + i),
        ),
      ],
      auto,
      RATE_META,
    );
    expect(rate.min).toBe(0);
  });

  it('auto does not force a temperature down to zero', () => {
    const domain = yDomain(
      [
        series(
          't',
          'T',
          points(5, (i) => 60 + i),
        ),
      ],
      configWith(),
      CELSIUS_META,
    );
    expect(domain.min).toBeGreaterThan(50);
    expect(domain.max).toBeGreaterThan(64);
  });

  it('produces round ticks', () => {
    expect(niceTicks(0, 100, 4)).toEqual([0, 25, 50, 75, 100]);
    expect(niceTicks(10, 10, 4)).toEqual([]);
    const ticks = timeTicks(T0, T0 + 15 * 60_000, 4);
    expect(ticks.length).toBeGreaterThanOrEqual(3);
    for (const tick of ticks) expect(tick % 60_000).toBe(0);
  });
});

describe('format', () => {
  it('formats every family consistently', () => {
    expect(formatValue(27.44, 'percent')).toBe('27.4 %');
    expect(formatValue(67.4, 'celsius')).toBe('67 °C');
    expect(formatValue(12.8 * 1024 * 1024, 'bytesPerSecond')).toBe('12.8 MiB/s');
    expect(formatValue(512, 'bytesPerSecond')).toBe('512 B/s');
    expect(formatValue(1234, 'count')).toBe('1,234');
    expect(formatValue(27.44, 'percent', 0, false)).toBe('27');
    expect(formatParts(Number.NaN, 'percent')).toEqual({ value: '—', unit: '' });
  });
});

describe('colours and renderers', () => {
  it('theme mode uses tokens, manual mode the picked colours', () => {
    expect(resolveColors(configWith()).series(0)).toBe('var(--pulse-viz-1)');
    const manual = resolveColors(
      configWith({
        colors: { mode: 'manual', primary: '#112233', secondary: '#445566' },
      }),
    );
    expect(manual.series(0)).toBe('#112233');
    expect(manual.series(1)).toBe('#445566');
    expect(
      resolveColors(configWith({ colors: { series: { '1': { color: '#abcdef' } } } })).series(1),
    ).toBe('#abcdef');
  });

  it('threshold bands pick the first band a value fits', () => {
    expect(bandFor(PERCENT_THRESHOLDS, 12)?.color).toBe('#38d6c4');
    expect(bandFor(PERCENT_THRESHOLDS, 70)?.color).toBe('#e8c46a');
    expect(bandFor(PERCENT_THRESHOLDS, 99)?.color).toBe('#f0a08f');
    const stops = thresholdStops(
      configWith({ colors: { mode: 'threshold', thresholds: PERCENT_THRESHOLDS } }),
      { min: 0, max: 100 },
      '#000000',
    );
    expect(stops.map((s) => s.offset)).toEqual([
      '0.000%',
      '60.000%',
      '60.000%',
      '85.000%',
      '85.000%',
      '100.000%',
    ]);
  });

  it('a gauge needs bounds: a percentage has them, a temperature does not', () => {
    expect(rendererSupport('gauge', PERCENT_META, configWith()).ok).toBe(true);
    const temperature = rendererSupport('gauge', CELSIUS_META, configWith());
    expect(temperature.ok).toBe(false);
    expect(gaugeBounds(CELSIUS_META, configWith())).toBeNull();

    const fixed = configWith({ scale: { mode: 'fixed', min: 20, max: 110 } });
    expect(rendererSupport('gauge', CELSIUS_META, fixed).ok).toBe(true);
  });

  it('lists the six renderers', () => {
    expect(RENDERERS.map((r) => r.kind)).toEqual([
      'line',
      'area',
      'sparkline',
      'value',
      'bar',
      'gauge',
    ]);
  });

  it('mergeConfig replaces arrays and merges objects', () => {
    const merged = mergeConfig(BASE_CONFIG, {
      line: { width: 5 },
      colors: { thresholds: [{ upTo: null, color: '#000000' }] },
    });
    expect(merged.line).toEqual({ ...BASE_CONFIG.line, width: 5 });
    expect(merged.colors.thresholds).toHaveLength(1);
  });
});
