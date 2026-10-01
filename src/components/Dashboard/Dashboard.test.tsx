import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { MetricRef } from '@/types/metrics';
import * as metricsService from '@/services/metrics';
import * as historyService from '@/services/history';
import { resetMetricCatalogForTesting } from '@/hooks/useMetricCatalog';
import {
  flushUiConfig,
  initUiConfig,
  memoryBackend,
  readSection,
  resetUiConfigForTesting,
} from '@/config/uiConfig';
import { reloadVisualizationStoreForTesting } from '@/visualization/store';
import { deliverLiveTickForTesting, setLiveBackendForTesting } from '@/live/liveFeed';
import type { LiveBackend } from '@/live/liveFeed';
import { setSourceRefsForTesting, sourceRefsFrom } from '@/dashboard/bindings';
import { DashboardPage } from '@/components/Dashboard/DashboardPage';
import { WidgetContent } from '@/components/Dashboard/WidgetContent';
import { WidgetCard } from '@/components/Dashboard/WidgetCard';
import { normalizeDashboards } from '@/dashboard/dashboards';
import { CATALOG, SOURCE_REFS, widgetFrom } from '@/test/dashboard';
import { T0, historyResponse } from '@/test/visualization';

let subscriptions: MetricRef[][] = [];

function liveBackend(): LiveBackend {
  return {
    setSubscription: (metrics) => {
      subscriptions.push([...metrics]);
      return Promise.resolve({
        refused: metrics
          .filter((metric) => metric.key.startsWith('storage.health.'))
          .map((metric) => ({ metric, reason: 'read on demand, never every second' })),
      });
    },
    buffer: () => Promise.resolve([]),
    onTick: () => () => undefined,
  };
}

function tick(t: number, values: [MetricRef, number | null][]) {
  act(() => deliverLiveTickForTesting({ t, values: values.map(([metric, v]) => ({ metric, v })) }));
}

const CPU: MetricRef = { key: 'cpu.usage.total', sourceId: 'cpu:system' };

beforeEach(() => {
  subscriptions = [];
  resetUiConfigForTesting();
  reloadVisualizationStoreForTesting();
  resetMetricCatalogForTesting();
  setSourceRefsForTesting(sourceRefsFrom(SOURCE_REFS));
  setLiveBackendForTesting(liveBackend());
  vi.spyOn(metricsService, 'getMetricCatalog').mockResolvedValue(CATALOG);
  vi.spyOn(metricsService, 'sampleMetrics').mockResolvedValue([]);
  vi.spyOn(historyService, 'getMetricHistory').mockImplementation((metrics, range) =>
    Promise.resolve(
      historyResponse(
        metrics.map((metric) => ({
          metric,
          points: [0, 1, 2, 3].map((i) => ({ t: T0 + i * 5000, v: 20 + i })),
        })),
        { range },
      ),
    ),
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  setLiveBackendForTesting(null);
  setSourceRefsForTesting(null);
});

const dashboards = () => normalizeDashboards(readSection('dashboards'));
const active = () => dashboards().items.find((item) => item.id === dashboards().activeId)!;

describe('dashboard page', () => {
  it('shows the default dashboard, locked', async () => {
    render(<DashboardPage />);
    const grid = await screen.findByRole('list', { name: 'Default widgets' });
    expect(grid.querySelectorAll(':scope > [role="listitem"]')).toHaveLength(7);
    expect(screen.queryByRole('button', { name: /Drag to move/ })).toBeNull();
    expect(screen.getByRole('button', { name: 'Edit layout' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('unlock shows handles; lock hides them again', async () => {
    const user = userEvent.setup();
    render(<DashboardPage />);
    await user.click(screen.getByRole('button', { name: 'Edit layout' }));
    expect(screen.getAllByRole('button', { name: /Drag to move/ }).length).toBe(7);
    await user.click(screen.getByRole('button', { name: 'Lock layout' }));
    expect(screen.queryByRole('button', { name: /Drag to move/ })).toBeNull();
  });

  it('adds CPU Total from the library; disabled entries explain why', async () => {
    const user = userEvent.setup();
    render(<DashboardPage />);
    await user.click(screen.getAllByRole('button', { name: 'Add widget' })[0]!);
    const library = await screen.findByRole('dialog', { name: 'Add widget' });

    await user.click(within(library).getByRole('button', { name: 'GPU' }));
    expect(within(library).getByRole('button', { name: 'Add GPU usage' })).toBeDisabled();
    expect(within(library).getAllByText(/libnvidia-ml/).length).toBeGreaterThan(0);

    await user.click(within(library).getByRole('button', { name: 'CPU' }));
    await user.click(within(library).getByRole('button', { name: 'Add CPU Total — value' }));
    const widgets = active().widgets;
    expect(widgets).toHaveLength(8);
    expect(widgets.at(-1)!.bindings[0]!.key).toBe('cpu.usage.total');
  });

  it('keyboard: arrows move, Shift+arrows resize, Delete removes', async () => {
    const user = userEvent.setup();
    render(<DashboardPage />);
    await user.click(screen.getByRole('button', { name: 'Edit layout' }));
    const before = active().widgets.find((w) => w.id === 'w-default-cpu-total')!;
    const card = document.querySelector('[data-widget-id="w-default-cpu-total"]') as HTMLElement;

    fireEvent.keyDown(card, { key: 'ArrowRight' });
    expect(active().widgets.find((w) => w.id === 'w-default-cpu-total')!.layout.x).toBe(
      before.layout.x + 1,
    );
    fireEvent.keyDown(card, { key: 'ArrowDown', shiftKey: true });
    expect(active().widgets.find((w) => w.id === 'w-default-cpu-total')!.layout.h).toBe(
      before.layout.h + 1,
    );
    fireEvent.keyDown(card, { key: 'Delete' });
    expect(active().widgets.some((w) => w.id === 'w-default-cpu-total')).toBe(false);
  });

  it('drag handle moves a widget by whole cells; the chart is not a drag surface', async () => {
    const user = userEvent.setup();
    render(<DashboardPage />);
    await user.click(screen.getByRole('button', { name: 'Edit layout' }));
    const handle = screen.getByRole('button', { name: 'Drag to move GPU' });
    const before = active().widgets.find((w) => w.id === 'w-default-gpu')!.layout;

    fireEvent.pointerDown(handle, { button: 0, clientX: 100, clientY: 100, pointerId: 1 });
    fireEvent.pointerMove(window, { clientX: 100, clientY: 100 + 2 * 54 + 5 });
    fireEvent.pointerUp(window, { clientX: 100, clientY: 100 + 2 * 54 + 5 });

    const after = active().widgets.find((w) => w.id === 'w-default-gpu')!.layout;
    expect(after.y).toBeGreaterThanOrEqual(before.y);
    const chart = document.querySelector('[data-widget-id="w-default-cpu-total"] .widget__body')!;
    fireEvent.pointerDown(chart, { button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(window, { clientX: 400, clientY: 400 });
    fireEvent.pointerUp(window);
    expect(active().widgets.find((w) => w.id === 'w-default-cpu-total')!.layout).toEqual({
      x: 0,
      y: 2,
      w: 6,
      h: 4,
    });
  });

  it('duplicate and customize a widget: title, renderer, colour', async () => {
    const user = userEvent.setup();
    render(<DashboardPage />);
    await user.click(screen.getByRole('button', { name: 'Edit layout' }));
    const card = document.querySelector('[data-widget-id="w-default-memory"]') as HTMLElement;
    await user.click(within(card).getByRole('button', { name: 'Duplicate' }));
    expect(
      active().widgets.filter((w) => w.bindings[0]!.key === 'memory.usage.percent'),
    ).toHaveLength(2);

    await user.click(within(card).getByRole('button', { name: 'Customize' }));
    const panel = await screen.findByRole('dialog', { name: /Customize/ });
    await user.type(within(panel).getByLabelText('Widget title'), 'RAM big');
    await user.click(
      within(within(panel).getByRole('group', { name: 'Visualization type' })).getByRole('button', {
        name: 'Gauge',
      }),
    );
    fireEvent.change(within(panel).getByLabelText('Primary colour'), {
      target: { value: '#ff3366' },
    });

    const memory = active().widgets.find((w) => w.id === 'w-default-memory')!;
    expect(memory.title).toBe('RAM big');
    expect(memory.visual.config.renderer).toBe('gauge');
    expect(memory.visual.config.colors.primary).toBe('#ff3366');
    expect(memory.visual.modified).toBe(true);
  });

  it('multiple dashboards: create, rename, delete', async () => {
    const user = userEvent.setup();
    render(<DashboardPage />);
    await user.click(screen.getByRole('button', { name: 'New' }));
    const input = screen.getByLabelText('Dashboard name');
    await user.clear(input);
    await user.type(input, 'Gaming');
    await user.click(screen.getByRole('button', { name: 'Blank dashboard' }));
    expect(active().name).toBe('Gaming');
    expect(await screen.findByText('This dashboard is empty.')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Rename' }));
    const dialog = screen.getByRole('alertdialog');
    const rename = within(dialog).getByLabelText('Dashboard name');
    await user.clear(rename);
    await user.type(rename, 'Development');
    await user.click(within(dialog).getByRole('button', { name: 'Rename' }));
    expect(active().name).toBe('Development');

    await user.click(screen.getByRole('button', { name: 'Delete' }));
    await user.click(screen.getByRole('button', { name: 'Delete dashboard' }));
    expect(dashboards().items).toHaveLength(1);
  });

  it('persists across a relaunch', async () => {
    const backend = memoryBackend();
    await act(() => initUiConfig(backend));
    const user = userEvent.setup();
    const first = render(<DashboardPage />);
    await user.click(screen.getByRole('button', { name: 'Edit layout' }));
    fireEvent.keyDown(document.querySelector('[data-widget-id="w-default-gpu"]')!, {
      key: 'ArrowDown',
      shiftKey: true,
    });
    const saved = active();
    await act(() => flushUiConfig());
    first.unmount();

    resetUiConfigForTesting();
    await act(() => initUiConfig(backend));
    render(<DashboardPage />);
    expect(active()).toEqual(saved);
    expect(screen.getByRole('button', { name: 'Lock layout' })).toBeInTheDocument();
  });
});

describe('edit tools of a small widget', () => {
  it('a tiny widget gets a handle and one "…" menu, never a row of buttons', async () => {
    const user = userEvent.setup();
    const calls: string[] = [];
    const actions = {
      onCustomize: () => calls.push('customize'),
      onDuplicate: () => calls.push('duplicate'),
      onSendToOverlay: () => calls.push('overlay'),
      onRemove: () => calls.push('remove'),
      onMoveStart: () => undefined,
      onResizeStart: () => undefined,
    };
    const { container } = render(
      <WidgetCard
        widget={widgetFrom('cpu-value')}
        width={110}
        height={44}
        editing
        actions={actions}
      />,
    );
    const tools = container.querySelector('.widget__tools')!;
    expect(tools).toHaveClass('widget__tools--compact');
    expect(within(tools as HTMLElement).queryByRole('button', { name: 'Customize' })).toBeNull();
    expect(container.querySelector('.widget__resize')).not.toBeNull();

    await user.click(screen.getByRole('button', { name: /actions menu/ }));
    const menu = screen.getByRole('menu');
    expect(
      within(menu)
        .getAllByRole('menuitem')
        .map((item) => item.textContent),
    ).toEqual(['Customize', 'Duplicate', 'To overlay', 'Remove']);
    await user.click(within(menu).getByRole('menuitem', { name: 'Duplicate' }));
    expect(calls).toEqual(['duplicate']);
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('a roomy widget keeps its full toolbar', () => {
    const { container } = render(
      <WidgetCard
        widget={widgetFrom('cpu-value')}
        width={420}
        height={200}
        editing
        actions={{ onCustomize: () => undefined }}
      />,
    );
    expect(container.querySelector('.widget__tools--compact')).toBeNull();
    expect(screen.getByRole('button', { name: 'Customize' })).toBeInTheDocument();
  });
});

describe('widgets', () => {
  it('CPU Total value renders from the live feed, subscribing to cpu.usage.total only', async () => {
    render(<WidgetContent widget={widgetFrom('cpu-value')} width={110} height={44} />);
    await waitFor(() => expect(subscriptions.at(-1)).toEqual([CPU]));
    tick(T0, [[CPU, 23.4]]);
    expect(document.body.textContent).toContain('23');
  });

  it.each([
    [60, 20],
    [80, 24],
    [120, 32],
  ])('a %i×%i value widget stays clean', async (width, height) => {
    const { container } = render(
      <WidgetContent widget={widgetFrom('cpu-value')} width={width} height={height} />,
    );
    await waitFor(() => expect(subscriptions.length).toBeGreaterThan(0));
    tick(T0, [[CPU, 71]]);
    expect(container.textContent).toContain('71');
    expect(container.innerHTML).not.toMatch(/NaN|undefined/);
  });

  it.each([
    [80, 24],
    [120, 32],
    [160, 40],
  ])('a %i×%i sparkline draws a live trend without chrome', async (width, height) => {
    const { container } = render(
      <WidgetContent widget={widgetFrom('cpu-sparkline')} width={width} height={height} />,
    );
    await waitFor(() => expect(subscriptions.length).toBeGreaterThan(0));
    for (let i = 0; i < 20; i += 1) tick(T0 + i * 1000, [[CPU, 10 + (i % 5) * 7]]);
    expect(container.querySelectorAll('path.viz-chart__line')).toHaveLength(1);
    expect(container.querySelectorAll('.viz-chart__tick')).toHaveLength(0);
    expect(container.innerHTML).not.toMatch(/NaN/);
  });

  it.each([
    [110, 44],
    [160, 80],
    [280, 140],
  ])('a %i×%i group never overflows and keeps its values', async (width, height) => {
    const widget = widgetFrom('cpu-group');
    const before = JSON.stringify(widget);
    const { container } = render(<WidgetContent widget={widget} width={width} height={height} />);
    await waitFor(() => expect(subscriptions.at(-1)?.length).toBe(2));
    tick(T0, [[CPU, 37]]);
    const group = container.querySelector('[data-group-layout]') as HTMLElement;
    expect(group).not.toBeNull();
    expect(container.textContent).toContain('37');
    expect(container.querySelectorAll('.viz-chart__tick')).toHaveLength(0);
    expect(container.innerHTML).not.toMatch(/NaN|undefined/);
    expect(JSON.stringify(widget)).toBe(before);
  });

  it('a group shows each value with its unit, and — for an unreadable one', async () => {
    render(<WidgetContent widget={widgetFrom('gpu-group')} width={220} height={96} />);
    await waitFor(() => expect(screen.getByText('Usage')).toBeInTheDocument());
    expect(screen.getAllByText('—').length).toBeGreaterThan(0);
    expect(document.body.textContent).not.toMatch(/\b0 %/);
  });

  it('a CPU group shows usage and temperature', async () => {
    render(<WidgetContent widget={widgetFrom('cpu-group')} width={220} height={96} />);
    await waitFor(() => expect(subscriptions.at(-1)?.length).toBe(2));
    tick(T0, [
      [CPU, 37],
      [{ key: 'cpu.temperature.package', sourceId: 'cpu:package-0' }, 68],
    ]);
    expect(document.body.textContent).toContain('37');
    expect(document.body.textContent).toContain('68 °C');
  });

  it('memory, thermal, storage, network and process widgets resolve and render', async () => {
    for (const id of ['memory', 'thermal', 'storage', 'network', 'processes', 'summary']) {
      const { container, unmount } = render(
        <WidgetContent widget={widgetFrom(id)} width={300} height={140} />,
      );
      await waitFor(() => expect(container.innerHTML).not.toBe(''));
      expect(container.innerHTML).not.toMatch(/NaN/);
      unmount();
    }
  });

  it('a drive temperature widget is refused live and says why, never 0', async () => {
    // Micro: a glyph on screen, the reason kept as the accessible name and tooltip.
    const { unmount } = render(
      <WidgetContent widget={widgetFrom('ssd-temp')} width={120} height={40} />,
    );
    const reason = await screen.findByLabelText(/on demand/);
    expect(reason).toHaveAttribute('title', expect.stringMatching(/on demand/));
    expect(document.body.textContent).not.toMatch(/\b0 ?°/);
    unmount();
    // Roomy: the reason is written out.
    render(<WidgetContent widget={widgetFrom('ssd-temp')} width={320} height={140} />);
    expect(await screen.findByText(/on demand/)).toBeInTheDocument();
  });

  it('a missing source says "Source unavailable" and does not crash', async () => {
    const widget = widgetFrom('storage');
    const orphan = {
      ...widget,
      bindings: widget.bindings.map((binding) => ({
        ...binding,
        source: { mode: 'fixed' as const, ref: 'storage:ffffffffffffffff' },
      })),
    };
    render(<WidgetContent widget={orphan} width={300} height={140} />);
    expect(await screen.findByText(/Source unavailable/)).toBeInTheDocument();
  });

  it('20 widgets with 10 charts render in reasonable time', async () => {
    const ids = [
      'cpu-total',
      'memory',
      'network',
      'storage',
      'thermal',
      'cpu-sparkline',
      'cpu-value',
      'memory-value',
      'cpu-group',
      'summary',
    ];
    const widgets = [...ids, ...ids].map((id) => widgetFrom(id));
    const began = performance.now();
    const { container } = render(
      <div>
        {widgets.map((widget) => (
          <WidgetContent key={widget.id} widget={widget} width={300} height={140} />
        ))}
      </div>,
    );
    await waitFor(() => expect(container.querySelectorAll('.viz, .widget-group').length).toBe(20));
    for (let i = 0; i < 10; i += 1) tick(T0 + i * 1000, [[CPU, i]]);
    const elapsed = performance.now() - began;
    // jsdom is far slower than WebKit; this only guards against runaway work.
    expect(elapsed).toBeLessThan(5_000);
    console.info(
      `dashboard: 20 widgets rendered + 10 live ticks in ${elapsed.toFixed(0)} ms (jsdom)`,
    );
  });
});
