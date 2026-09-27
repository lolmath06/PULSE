import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { MetricVisualization } from '@/visualization/MetricVisualization';
import type { VisualizationData } from '@/visualization/types';
import { PERCENT_THRESHOLDS } from '@/visualization/color';
import {
  CELSIUS_META,
  PERCENT_META,
  RATE_META,
  T0,
  configWith,
  dataOf,
  points,
  series,
} from '@/test/visualization';

function draw(
  data: VisualizationData,
  patch: Parameters<typeof configWith>[0] = {},
  size: { width?: number; height?: number } = { width: 600, height: 240 },
  meta = PERCENT_META,
) {
  return render(
    <MetricVisualization data={data} meta={meta} config={configWith(patch)} {...size} />,
  );
}

const lines = (container: HTMLElement) => container.querySelectorAll('path.viz-chart__line');

function expectNoNaN(container: HTMLElement) {
  expect(container.innerHTML).not.toMatch(/NaN|Infinity|undefined/);
}

describe('MetricVisualization — data states', () => {
  it('0 points: says it is collecting, draws nothing', () => {
    const { container } = draw(dataOf([series('cpu', 'CPU', [])]));
    expect(screen.getByText('Collecting history…')).toBeInTheDocument();
    expect(container.querySelector('svg')).toBeNull();
  });

  it('1 point: shows the current value but no fake line', () => {
    const { container } = draw(
      dataOf([
        series(
          'cpu',
          'CPU',
          points(1, () => 27.4),
        ),
      ]),
    );
    expect(screen.getAllByText(/27\.4/).length).toBeGreaterThan(0);
    expect(screen.getByText(/one sample so far/)).toBeInTheDocument();
    expect(lines(container)).toHaveLength(0);
  });

  it('2 points: draws a line', () => {
    const { container } = draw(dataOf([series('cpu', 'CPU', points(2))]));
    expect(lines(container)).toHaveLength(1);
    expect(lines(container)[0]!.getAttribute('d')).toMatch(/^M/);
    expectNoNaN(container);
  });

  it('many points: one path, bounded DOM', () => {
    const { container } = draw(dataOf([series('cpu', 'CPU', points(720))]));
    expect(lines(container)).toHaveLength(1);
    expect(container.querySelectorAll('*').length).toBeLessThan(150);
    expectNoNaN(container);
  });

  it('a gap splits the line instead of bridging it', () => {
    const gapped = [...points(3), ...points(3).map((p) => ({ ...p, t: p.t + 7_200_000 }))];
    const { container } = draw(dataOf([series('cpu', 'CPU', gapped)]));
    expect(lines(container)).toHaveLength(2);
  });

  it('unavailable history says why and never shows zero', () => {
    const { container } = draw(
      dataOf([series('cpu', 'CPU', [])], { status: 'unavailable', message: 'disk is read-only' }),
    );
    expect(screen.getByText('History unavailable: disk is read-only')).toBeInTheDocument();
    expect(container.textContent).not.toMatch(/\b0(\.0)? ?%/);
  });

  it('while history is unavailable an instant renderer still shows a live value', () => {
    const data = dataOf([{ id: 'cpu', label: 'CPU', points: [], latest: { t: T0, v: 42 } }], {
      status: 'unavailable',
      message: 'no disk',
    });
    draw(data, { renderer: 'value' });
    expect(screen.getByText('42.0')).toBeInTheDocument();
  });
});

describe('MetricVisualization — multi-series', () => {
  it('draws each series and survives one being empty', () => {
    const { container } = draw(
      dataOf([series('rx', 'Download', points(10)), series('tx', 'Upload', [])]),
      {},
      { width: 600, height: 240 },
      RATE_META,
    );
    expect(lines(container)).toHaveLength(1);
    expect(container.querySelector('.viz__legend')!.textContent).toContain('Download');
    expect(screen.getByText(/no data/)).toBeInTheDocument();
    expectNoNaN(container);
  });

  it('uses per-series colours and labels', () => {
    const { container } = draw(
      dataOf([
        series('r', 'Read', points(5)),
        series(
          'w',
          'Write',
          points(5, () => 3),
        ),
      ]),
      {
        colors: {
          mode: 'manual',
          primary: '#111111',
          series: { '1': { color: '#ff00aa', label: 'Disk writes' } },
        },
      },
      { width: 600, height: 240 },
      RATE_META,
    );
    const strokes = [...lines(container)].map((path) => (path as SVGElement).style.stroke);
    expect(strokes).toEqual(['rgb(17, 17, 17)', 'rgb(255, 0, 170)']);
    expect(container.querySelector('.viz__legend')!.textContent).toContain('Disk writes');
  });
});

describe('MetricVisualization — chrome', () => {
  const data = dataOf([series('cpu', 'CPU', points(30))]);

  it('shows axes and grid by default and hides them on request', () => {
    const { container, rerender } = draw(data);
    expect(container.querySelectorAll('.viz-chart__tick').length).toBeGreaterThan(2);
    expect(container.querySelectorAll('.viz-chart__grid').length).toBeGreaterThan(1);

    rerender(
      <MetricVisualization
        data={data}
        meta={PERCENT_META}
        config={configWith({ axes: { x: false, y: false, grid: false } })}
        width={600}
        height={240}
      />,
    );
    expect(container.querySelectorAll('.viz-chart__tick')).toHaveLength(0);
    expect(container.querySelectorAll('.viz-chart__grid')).toHaveLength(0);
  });

  it('compact mode strips axes, grid, legend and statistics', () => {
    const { container } = draw(
      dataOf([series('a', 'A', points(10)), series('b', 'B', points(10))]),
      { display: { compact: true } },
    );
    expect(container.querySelectorAll('.viz-chart__tick')).toHaveLength(0);
    expect(container.querySelector('.viz__legend')).toBeNull();
    expect(container.querySelector('.viz__stats')).toBeNull();
  });

  it('summary statistics are shown and individually hideable', () => {
    const summaryData = dataOf([
      series(
        'cpu',
        'CPU',
        points(3, (i) => [10, 50, 30][i]!),
      ),
    ]);
    const { container, rerender } = draw(summaryData);
    const stats = container.querySelector('.viz__stats')!;
    expect(stats.textContent).toContain('Min10.0 %');
    expect(stats.textContent).toContain('Max50.0 %');
    expect(stats.textContent).toContain('Avg30.0 %');
    expect(stats.textContent).toContain('Current30.0 %');

    rerender(
      <MetricVisualization
        data={summaryData}
        meta={PERCENT_META}
        config={configWith({ display: { min: false, average: false } })}
        width={600}
        height={240}
      />,
    );
    expect(container.querySelector('.viz__stats')!.textContent).not.toContain('Min');
    expect(container.querySelector('.viz__stats')!.textContent).not.toContain('Avg');
  });

  it('legend can be hidden', () => {
    const two = dataOf([series('a', 'A', points(4)), series('b', 'B', points(4))]);
    const { container } = draw(two, { display: { legend: false } });
    expect(container.querySelector('.viz__legend')).toBeNull();
  });

  it('a gradient fill defines a gradient, a solid one does not', () => {
    const { container, rerender } = draw(data, {
      renderer: 'area',
      fill: { mode: 'gradient', opacity: 0.5 },
    });
    const area = container.querySelector('path.viz-chart__area') as SVGElement;
    expect(area.style.fill).toMatch(/^url\("?#/);
    expect(container.querySelector('linearGradient stop')).not.toBeNull();

    rerender(
      <MetricVisualization
        data={data}
        meta={PERCENT_META}
        config={configWith({ renderer: 'area', fill: { mode: 'solid', opacity: 0.25 } })}
        width={600}
        height={240}
      />,
    );
    const solid = container.querySelector('path.viz-chart__area') as SVGElement;
    expect(solid.style.fillOpacity).toBe('0.25');
  });

  it('a line chart never fills', () => {
    const { container } = draw(data, { renderer: 'line', fill: { mode: 'solid', opacity: 0.8 } });
    expect(container.querySelector('path.viz-chart__area')).toBeNull();
  });

  it('background opacity and transparency apply to the backdrop only', () => {
    const { container } = draw(data, { background: { mode: 'none', opacity: 0 } });
    const backdrop = container.querySelector('.viz__backdrop') as HTMLElement;
    expect(backdrop.style.opacity).toBe('0');
    expect(backdrop.style.background).toBe('transparent');
  });

  it('threshold mode strokes the line with band stops', () => {
    const { container } = draw(data, {
      colors: { mode: 'threshold', thresholds: PERCENT_THRESHOLDS },
      scale: { mode: 'fixed', min: 0, max: 100 },
    });
    const line = lines(container)[0] as SVGElement;
    expect(line.style.stroke).toMatch(/^url\("?#.*threshold"?\)$/);
  });

  it('shows the real sample in the tooltip, even with smoothing on', () => {
    const spiky = dataOf(
      [
        series(
          'cpu',
          'CPU',
          points(9, (i) => (i === 4 ? 91.3 : 5)),
        ),
      ],
      {
        window: { fromMs: T0, toMs: T0 + 40_000 },
      },
    );
    const { container } = draw(spiky, { smoothing: 'smooth' });
    const hit = container.querySelector('.viz-chart__hit')!;
    const svg = container.querySelector('svg')!;
    svg.getBoundingClientRect = () => ({ left: 0, top: 0, width: 600, height: 240 }) as DOMRect;
    const layoutLeft = Number(hit.getAttribute('x'));
    const plotWidth = Number(hit.getAttribute('width'));

    fireEvent.pointerMove(hit, { clientX: layoutLeft + plotWidth / 2, clientY: 50 });

    const tooltip = screen.getByRole('tooltip');
    expect(tooltip.textContent).toContain('91.3 %');
    expect(tooltip.textContent).toMatch(/\d{2}:\d{2}:\d{2}/);

    fireEvent.pointerLeave(hit);
    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  it('the tooltip can be disabled', () => {
    const { container } = draw(data, { display: { tooltip: false } });
    fireEvent.pointerMove(container.querySelector('.viz-chart__hit')!, { clientX: 300 });
    expect(screen.queryByRole('tooltip')).toBeNull();
  });
});

describe('MetricVisualization — sizes', () => {
  const data = dataOf([series('cpu', 'CPU', points(180))]);

  it.each([
    [60, 20],
    [80, 24],
    [120, 36],
    [200, 50],
  ])('a %i×%i sparkline stays clean', (width, height) => {
    const { container } = draw(data, { renderer: 'sparkline' }, { width, height });
    const root = container.querySelector('.viz') as HTMLElement;
    expect(root.style.width).toBe(`${width}px`);
    expect(root.style.height).toBe(`${height}px`);
    expect(container.querySelectorAll('.viz-chart__tick')).toHaveLength(0);
    expect(container.querySelectorAll('.viz-chart__grid')).toHaveLength(0);
    expect(container.querySelector('.viz__legend')).toBeNull();
    expect(lines(container)).toHaveLength(1);
    expect(container.querySelectorAll('*').length).toBeLessThan(40);
    const svg = container.querySelector('svg')!;
    expect(Number(svg.getAttribute('width'))).toBeLessThanOrEqual(width);
    expect(Number(svg.getAttribute('height'))).toBeLessThanOrEqual(height);
    expectNoNaN(container);
  });

  it.each([
    [800, 300],
    [1200, 450],
  ])('a %i×%i chart has axes and sensible spacing', (width, height) => {
    const { container } = draw(data, {}, { width, height });
    const svg = container.querySelector('svg')!;
    expect(Number(svg.getAttribute('width'))).toBe(width);
    expect(svg.getAttribute('viewBox')).toBe(`0 0 ${width} ${svg.getAttribute('height')}`);
    const hit = container.querySelector('.viz-chart__hit')!;
    expect(Number(hit.getAttribute('width'))).toBeGreaterThan(width * 0.85);
    expect(container.querySelectorAll('.viz-chart__tick').length).toBeGreaterThan(4);
    expectNoNaN(container);
  });

  it('named sizes change the height, custom width is honoured', () => {
    const { container, rerender } = render(
      <MetricVisualization
        data={data}
        meta={PERCENT_META}
        config={configWith({ size: { preset: 'small' } })}
      />,
    );
    expect((container.querySelector('.viz__body') as HTMLElement).style.height).toBe('72px');

    rerender(
      <MetricVisualization
        data={data}
        meta={PERCENT_META}
        config={configWith({ size: { preset: 'custom', width: 420, height: 260 } })}
      />,
    );
    expect((container.querySelector('.viz__body') as HTMLElement).style.height).toBe('260px');
    const root = container.querySelector('.viz') as HTMLElement;
    expect(root.style.width).toBe('420px');
    expect(root.style.maxWidth).toBe('100%');
  });
});

describe('MetricVisualization — instant renderers', () => {
  const at = (value: number) =>
    dataOf([
      series(
        'cpu',
        'CPU',
        points(3, () => value),
      ),
    ]);

  it.each([0, 50, 100])('a gauge at %i %% draws a correct ring', (value) => {
    const { container } = draw(at(value), { renderer: 'gauge' }, { width: 200, height: 200 });
    const meter = screen.getByRole('meter');
    expect(meter.getAttribute('aria-valuenow')).toBe(String(value));
    expect(container.querySelector('.viz-gauge__track')!.getAttribute('d')).toMatch(/^M/);
    const arc = container.querySelector('.viz-gauge__value');
    if (value === 0) expect(arc).toBeNull();
    else expect(arc!.getAttribute('d')).toMatch(/A/);
    expect(container.textContent).toContain(value.toFixed(1));
    expectNoNaN(container);
  });

  it('an unavailable gauge shows no value at all — not 0', () => {
    const { container } = draw(dataOf([series('cpu', 'CPU', [])]), { renderer: 'gauge' });
    expect(screen.getByText('Collecting history…')).toBeInTheDocument();
    expect(container.querySelector('.viz-gauge__value')).toBeNull();
    expect(container.textContent).not.toMatch(/\b0\.0\b/);
  });

  it('a temperature gauge is refused without bounds instead of faking 0–100', () => {
    draw(at(67), { renderer: 'gauge' }, { width: 200, height: 200 }, CELSIUS_META);
    expect(screen.getByText(/needs known bounds/)).toBeInTheDocument();
    expect(screen.queryByRole('meter')).toBeNull();
  });

  it('value shows the number with or without its label and unit', () => {
    const { container, rerender } = draw(
      at(27.4),
      { renderer: 'value' },
      { width: 160, height: 60 },
    );
    expect(container.textContent).toContain('CPU');
    expect(container.textContent).toContain('27.4');
    expect(container.textContent).toContain('%');

    rerender(
      <MetricVisualization
        data={at(27.4)}
        meta={PERCENT_META}
        config={configWith({
          renderer: 'value',
          text: { showLabel: false, showUnit: false, decimals: 0 },
        })}
        width={80}
        height={24}
      />,
    );
    expect(container.querySelector('.viz-value')!.textContent).toBe('27');
  });

  it('bar draws the share of a bounded metric', () => {
    const { container } = draw(at(72), { renderer: 'bar' }, { width: 300, height: 60 });
    const fill = container.querySelector('.viz-bar__fill') as HTMLElement;
    expect(parseFloat(fill.style.width)).toBe(72);
    expect(container.textContent).toContain('72.0 %');
  });

  it('bar scales an unbounded metric to the window peak and says so', () => {
    const rising = dataOf([
      series(
        'rx',
        'Download',
        points(4, (i) => (i + 1) * 1024),
      ),
    ]);
    const { container } = draw(rising, { renderer: 'bar' }, { width: 300, height: 90 }, RATE_META);
    const fill = container.querySelector('.viz-bar__fill') as HTMLElement;
    expect(parseFloat(fill.style.width)).toBe(100);
    expect(container.textContent).toMatch(/window's peak/);
  });

  it('switching renderer never changes the current value shown', () => {
    const data = at(33.3);
    for (const renderer of ['area', 'value', 'bar', 'gauge'] as const) {
      const { container, unmount } = draw(data, { renderer }, { width: 300, height: 200 });
      expect(container.textContent).toContain('33.3');
      unmount();
    }
  });
});
