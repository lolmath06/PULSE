import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { MetricVisualization } from '@/visualization/MetricVisualization';
import { densityOf, fitLine, groupLayout, presentationFor } from '@/visualization/presentation';
import type { VisualizationConfig } from '@/visualization/config';
import { PERCENT_META, configWith, dataOf, points, series } from '@/test/visualization';

/**
 * The responsive presentation policy: one box size in, what is drawn out.
 * Density is decided from the real box and applied to rendering only — the
 * stored configuration is never changed.
 */

const DATA = dataOf([
  series(
    'cpu',
    'CPU',
    points(60, (i) => 40 + (i % 7)),
  ),
]);
const CURRENT = '43'; // the last point: 40 + (59 % 7)

function frozen(patch: Parameters<typeof configWith>[0]): VisualizationConfig {
  const config = configWith(patch);
  return structuredClone(config);
}

function draw(config: VisualizationConfig, width: number, height: number) {
  const before = JSON.stringify(config);
  const view = render(
    <MetricVisualization
      data={DATA}
      meta={PERCENT_META}
      config={config}
      width={width}
      height={height}
    />,
  );
  const root = view.container.querySelector('.viz') as HTMLElement;
  return { ...view, root, unchanged: () => expect(JSON.stringify(config)).toBe(before) };
}

/** Header, body, legend and statistics never add up to more than the box. */
function expectFits(root: HTMLElement, height: number) {
  const parts = [
    root.querySelector<HTMLElement>('.viz__header'),
    root.querySelector<HTMLElement>('.viz__body'),
    root.querySelector<HTMLElement>('.viz__legend'),
  ];
  const used = parts.reduce(
    (sum, part) => sum + (part ? parseFloat(part.style.height) || 0 : 0),
    0,
  );
  expect(used).toBeLessThanOrEqual(height);
  const svg = root.querySelector('svg');
  if (svg) expect(Number(svg.getAttribute('height'))).toBeLessThanOrEqual(height);
  expect(root.innerHTML).not.toMatch(/NaN|Infinity|undefined/);
}

function expectNoSecondary(root: HTMLElement) {
  expect(root.querySelector('.viz__stats')).toBeNull();
  expect(root.querySelector('.viz__legend')).toBeNull();
  expect(root.querySelectorAll('.viz-chart__tick')).toHaveLength(0);
  expect(root.querySelectorAll('.viz-chart__grid')).toHaveLength(0);
  expect(root.querySelector('.viz__secondary')).toBeNull();
}

describe('density', () => {
  it.each([
    [60, 20, 'micro'],
    [110, 44, 'micro'],
    [160, 40, 'micro'],
    [160, 80, 'compact'],
    [320, 140, 'normal'],
    [800, 300, 'large'],
  ] as const)('%i×%i is %s', (width, height, density) => {
    expect(densityOf(width, height)).toBe(density);
  });
});

describe('value widget', () => {
  it.each([
    [60, 20],
    [80, 24],
    [110, 44],
    [160, 40],
  ])('%i×%i keeps the value and nothing secondary', (width, height) => {
    const config = frozen({ renderer: 'value' });
    const { root, unchanged } = draw(config, width, height);
    expect(root.textContent).toContain(CURRENT);
    expectNoSecondary(root);
    expectFits(root, height);
    unchanged();
  });
});

describe('sparkline widget', () => {
  it.each([
    [80, 24],
    [110, 44],
    [120, 32],
    [160, 40],
  ])('%i×%i draws the trend, no axes, no stats', (width, height) => {
    const config = frozen({ renderer: 'sparkline' });
    const { root, unchanged } = draw(config, width, height);
    expect(root.querySelectorAll('path.viz-chart__line')).toHaveLength(1);
    expectNoSecondary(root);
    expect(root.dataset.density).toBe('micro');
    expectFits(root, height);
    const svg = root.querySelector('svg')!;
    expect(Number(svg.getAttribute('width'))).toBeLessThanOrEqual(width);
    unchanged();
  });

  it('beside the text, never on top of it', () => {
    const { root } = draw(frozen({ renderer: 'sparkline' }), 160, 40);
    const strip = root.querySelector('.viz-strip')!;
    expect(strip).not.toBeNull();
    expect(strip.querySelector('.viz-strip__value')!.textContent).toContain(CURRENT);
    // The sparkline is a sibling after the text, not layered over it.
    expect(
      strip.lastElementChild?.querySelector('svg') ?? strip.querySelector(':scope > svg'),
    ).not.toBeNull();
  });
});

describe('line and area widgets', () => {
  it.each(['line', 'area'] as const)('%s at 110×44 is a readable strip', (renderer) => {
    const config = frozen({ renderer });
    const { root, unchanged } = draw(config, 110, 44);
    expect(root.dataset.density).toBe('micro');
    expect(root.querySelector('.viz-strip__value')!.textContent).toContain(CURRENT);
    expect(root.querySelector('.viz__header')).toBeNull();
    expectNoSecondary(root);
    expectFits(root, 44);
    unchanged();
  });

  it.each(['line', 'area'] as const)('%s at 160×80 has an inline header, no stats', (renderer) => {
    const config = frozen({ renderer });
    const { root, unchanged } = draw(config, 160, 80);
    expect(root.querySelector('.viz__header--inline')).not.toBeNull();
    expect(root.textContent).toContain(CURRENT);
    expectNoSecondary(root);
    expectFits(root, 80);
    unchanged();
  });

  it.each(['line', 'area'] as const)('%s at 320×140 brings back the statistics', (renderer) => {
    const config = frozen({ renderer });
    const { root, unchanged } = draw(config, 320, 140);
    expect(root.dataset.density).toBe('normal');
    expect(root.querySelector('.viz__header:not(.viz__header--inline)')).not.toBeNull();
    expect(root.querySelector('.viz__stats')).not.toBeNull();
    expectFits(root, 140);
    unchanged();
  });

  it.each(['line', 'area'] as const)('%s at 800×300 has every detail', (renderer) => {
    const config = frozen({ renderer });
    const { root, unchanged } = draw(config, 800, 300);
    expect(root.dataset.density).toBe('large');
    expect(root.querySelector('.viz__stats')).not.toBeNull();
    expect(root.querySelectorAll('.viz-chart__tick').length).toBeGreaterThan(4);
    expectFits(root, 300);
    unchanged();
  });

  it('shrinking then enlarging the same widget restores the detail', () => {
    const config = frozen({ renderer: 'line' });
    const { root, rerender, unchanged } = draw(config, 800, 300);
    const at = (width: number, height: number) =>
      rerender(
        <MetricVisualization
          data={DATA}
          meta={PERCENT_META}
          config={config}
          width={width}
          height={height}
        />,
      );
    at(110, 44);
    expect(root.querySelector('.viz__stats')).toBeNull();
    at(800, 300);
    expect(root.querySelector('.viz__stats')).not.toBeNull();
    expect(root.querySelectorAll('.viz-chart__tick').length).toBeGreaterThan(4);
    unchanged();
  });
});

describe('renderer degradation (rendering only)', () => {
  it('a gauge too small to read becomes a value', () => {
    const config = frozen({ renderer: 'gauge' });
    const small = presentationFor({ config, width: 110, height: 44, seriesCount: 1, exact: true });
    expect(small.renderer).toBe('value');
    expect(small.fallback).toBe(true);
    expect(config.renderer).toBe('gauge');
    const big = presentationFor({ config, width: 240, height: 200, seriesCount: 1, exact: true });
    expect(big.renderer).toBe('gauge');
  });

  it('a gauge widget at 60×20 still shows its value', () => {
    const config = frozen({ renderer: 'gauge' });
    const { root, unchanged } = draw(config, 60, 20);
    expect(root.dataset.renderer).toBe('value');
    expect(root.textContent).toContain(CURRENT);
    unchanged();
  });

  it('a bar drops its label before its bar or value', () => {
    const { root } = draw(frozen({ renderer: 'bar' }), 140, 30);
    expect(root.querySelector('.viz-bar__label')).toBeNull();
    expect(root.querySelector('.viz-bar__track')).not.toBeNull();
    expect(root.querySelector('.viz-bar__value')!.textContent).toContain(CURRENT);
    const wide = draw(frozen({ renderer: 'bar' }), 300, 60);
    expect(wide.root.querySelector('.viz-bar__label')).not.toBeNull();
  });

  it('a bar too narrow for a track becomes a value', () => {
    const { root } = draw(frozen({ renderer: 'bar' }), 80, 24);
    expect(root.dataset.renderer).toBe('value');
    expect(root.textContent).toContain(CURRENT);
  });
});

describe('text fitting', () => {
  it('drops the label, then the unit, before the value', () => {
    const roomy = fitLine(200, 24, { label: 'CPU', value: '46.0', unit: '%' });
    expect(roomy).toMatchObject({ label: true, unit: true });
    const tight = fitLine(50, 20, { label: 'Temperature', value: '46.0', unit: '°C' });
    expect(tight.label).toBe(false);
    expect(tight.fontPx).toBeGreaterThanOrEqual(8);
  });
});

describe('group layout', () => {
  it.each([
    [110, 44],
    [160, 80],
    [280, 140],
  ])('%i×%i never lays more rows than fit', (width, height) => {
    const layout = groupLayout(3, width, height, 'rows', false, 1);
    expect(layout.visible).toBeGreaterThan(0);
    if (layout.orientation === 'rows') expect(layout.visible * 13).toBeLessThanOrEqual(height);
  });

  it('labels and sparklines come back only with room', () => {
    expect(groupLayout(3, 110, 44, 'rows', true, 1).sparklines).toBe(false);
    const big = groupLayout(3, 280, 140, 'rows', true, 1);
    expect(big).toMatchObject({ visible: 3, labels: true, sparklines: true });
  });
});
