import { beforeEach, describe, expect, it } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { CustomizePanel } from '@/visualization/CustomizePanel';
import { MetricVisualization } from '@/visualization/MetricVisualization';
import { resetUiConfigForTesting } from '@/config/uiConfig';
import { reloadVisualizationStoreForTesting, useChartVisualization } from '@/visualization/store';
import type { DeepPartial, VisualizationConfig } from '@/visualization/config';
import type { VisualizationMeta } from '@/visualization/types';
import { CELSIUS_META, PERCENT_META, dataOf, points, series } from '@/test/visualization';

const DEFAULTS: DeepPartial<VisualizationConfig> = {
  renderer: 'line',
  scale: { mode: 'fixed', min: 0, max: 100 },
};
const TEMPERATURE_DEFAULTS: DeepPartial<VisualizationConfig> = { renderer: 'line' };

function Harness({
  meta = PERCENT_META,
  defaults = DEFAULTS,
}: {
  meta?: VisualizationMeta;
  defaults?: DeepPartial<VisualizationConfig>;
}) {
  const chart = useChartVisualization('test.chart', defaults);
  const [open, setOpen] = useState(true);
  const data = dataOf([
    series(
      'cpu',
      'CPU',
      points(40, (i) => 30 + (i % 7)),
    ),
  ]);
  return (
    <>
      <div data-testid="chart">
        <MetricVisualization
          data={data}
          meta={meta}
          config={chart.config}
          width={600}
          height={240}
        />
      </div>
      {open && (
        <CustomizePanel
          title="CPU history"
          chart={chart}
          data={data}
          meta={meta}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}

const chart = () => screen.getByTestId('chart').querySelector('.viz') as HTMLElement;
const panel = () => screen.getByRole('dialog', { name: 'Customize CPU history' });

beforeEach(() => {
  resetUiConfigForTesting();
  reloadVisualizationStoreForTesting();
});

describe('CustomizePanel', () => {
  it('switches line → area → sparkline → gauge → value → bar, live', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const types = within(panel()).getByRole('group', { name: 'Visualization type' });

    for (const [label, kind] of [
      ['Area', 'area'],
      ['Sparkline', 'sparkline'],
      ['Gauge', 'gauge'],
      ['Value', 'value'],
      ['Bar', 'bar'],
      ['Line', 'line'],
    ] as const) {
      await user.click(within(types).getByRole('button', { name: label }));
      expect(chart().dataset.renderer).toBe(kind);
    }
  });

  it('offers Gauge only when the metric has bounds', () => {
    render(<Harness meta={CELSIUS_META} defaults={TEMPERATURE_DEFAULTS} />);
    const gauge = within(panel()).getByRole('button', { name: 'Gauge' });
    expect(gauge).toBeDisabled();
    expect(gauge.getAttribute('title')).toMatch(/needs known bounds/);
  });

  it('changes a colour live and switches to manual colours', () => {
    render(<Harness />);
    fireEvent.change(within(panel()).getByLabelText('Primary colour'), {
      target: { value: '#ff3366' },
    });
    const line = screen.getByTestId('chart').querySelector('path.viz-chart__line') as SVGElement;
    expect(line.style.stroke).toBe('rgb(255, 51, 102)');
    expect(within(panel()).getByText('Custom (from Clean)')).toBeInTheDocument();
  });

  it('applies line width, curve, fill, background, grid and axes immediately', async () => {
    const user = userEvent.setup();
    render(<Harness />);

    fireEvent.change(within(panel()).getByLabelText('Line width'), { target: { value: '5' } });
    const line = () =>
      screen.getByTestId('chart').querySelector('path.viz-chart__line') as SVGElement;
    expect(line().style.strokeWidth).toBe('5');

    await user.click(within(panel()).getByText('Line & fill'));
    const curve = within(panel()).getByRole('group', { name: 'Curve' });
    const smoothPath = line().getAttribute('d');
    await user.click(within(curve).getByRole('button', { name: 'Stepped' }));
    expect(line().getAttribute('d')).not.toBe(smoothPath);

    fireEvent.change(within(panel()).getByLabelText('Background opacity'), {
      target: { value: '0.2' },
    });
    expect((chart().querySelector('.viz__backdrop') as HTMLElement).style.opacity).toBe('0.2');

    await user.click(within(panel()).getByLabelText('Grid'));
    expect(chart().querySelectorAll('.viz-chart__grid')).toHaveLength(0);
    await user.click(within(panel()).getByLabelText('Y axis'));
    await user.click(within(panel()).getByLabelText('X axis'));
    expect(chart().querySelectorAll('.viz-chart__tick')).toHaveLength(0);
  });

  it('presets are applied, then Reset undoes later edits', async () => {
    const user = userEvent.setup();
    render(<Harness />);

    await user.click(within(panel()).getByRole('button', { name: 'Technical' }));
    expect(within(panel()).getByRole('button', { name: 'Technical' })).toHaveClass(
      'customize__preset--active',
    );
    const technical = chart().innerHTML;

    fireEvent.change(within(panel()).getByLabelText('Line width'), { target: { value: '7' } });
    expect(within(panel()).getByText('Custom (from Technical)')).toBeInTheDocument();

    await user.click(within(panel()).getByRole('button', { name: 'Reset visualization' }));
    expect(chart().innerHTML).toBe(technical);
  });

  it('refuses a fixed scale whose maximum is not above its minimum', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(within(panel()).getByText('Axes, grid & scale'));

    const min = within(panel()).getByLabelText('Min');
    const max = within(panel()).getByLabelText('Max');
    await user.clear(min);
    await user.type(min, '80');
    await user.clear(max);
    await user.type(max, '20');

    expect(within(panel()).getByRole('alert')).toHaveTextContent(/greater than the minimum/);
    expect(within(panel()).getByRole('button', { name: 'Apply' })).toBeDisabled();
  });

  it('closes on Escape', () => {
    render(<Harness />);
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
