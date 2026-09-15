import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { EngineStatus } from '@/types/metrics';
import { MetricsEngineCard } from '@/components/MetricsEngineCard/MetricsEngineCard';
import * as metricsService from '@/services/metrics';

function status(overrides: Partial<EngineStatus> = {}): EngineStatus {
  return {
    schemaVersion: 1,
    state: 'empty',
    providerCount: 0,
    metricCount: 0,
    availableMetricCount: 0,
    providers: [],
    ...overrides,
  };
}

describe('MetricsEngineCard', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('reports an empty engine honestly instead of claiming it is ready', async () => {
    vi.spyOn(metricsService, 'getMetricsEngineStatus').mockResolvedValue(status());

    render(<MetricsEngineCard />);

    const card = await screen.findByLabelText('Metrics engine');
    expect(card).toHaveTextContent('Empty');
    expect(card).toHaveTextContent('v1');
    expect(card).toHaveTextContent('No providers registered yet');
  });

  it('shows provider and metric counts once providers exist', async () => {
    vi.spyOn(metricsService, 'getMetricsEngineStatus').mockResolvedValue(
      status({
        state: 'ready',
        providerCount: 2,
        metricCount: 7,
        availableMetricCount: 5,
        providers: [
          { id: 'linux.cpu', metricCount: 4, availableMetricCount: 4 },
          { id: 'linux.hwmon', metricCount: 3, availableMetricCount: 1 },
        ],
      }),
    );

    render(<MetricsEngineCard />);

    const card = await screen.findByLabelText('Metrics engine');
    expect(card).toHaveTextContent('Ready');
    expect(card).toHaveTextContent('7 (5 available)');
    expect(card).not.toHaveTextContent('No providers registered yet');
  });

  it('reports the two real providers and five metrics of Phase 2', async () => {
    // What Fedora and Windows both look like once the CPU and memory
    // providers are registered.
    vi.spyOn(metricsService, 'getMetricsEngineStatus').mockResolvedValue(
      status({
        state: 'ready',
        providerCount: 2,
        metricCount: 5,
        availableMetricCount: 5,
        providers: [
          { id: 'linux.cpu', metricCount: 1, availableMetricCount: 1 },
          { id: 'linux.memory', metricCount: 4, availableMetricCount: 4 },
        ],
      }),
    );

    render(<MetricsEngineCard />);

    const card = await screen.findByLabelText('Metrics engine');
    expect(card).toHaveTextContent('Ready');
    expect(card).toHaveTextContent('v1');
    expect(card).toHaveTextContent('2');
    expect(card).toHaveTextContent('5 (5 available)');
    expect(card).not.toHaveTextContent('No providers registered yet');
  });

  it('never fabricates hardware readings', async () => {
    vi.spyOn(metricsService, 'getMetricsEngineStatus').mockResolvedValue(status());

    render(<MetricsEngineCard />);
    const card = await screen.findByLabelText('Metrics engine');

    // Phase 1 collects nothing; the card must not imply otherwise.
    expect(card.textContent).not.toMatch(/°C|CPU\s*\d|GPU\s*\d|%/);
  });

  it('warns when the backend speaks a different contract version', async () => {
    vi.spyOn(metricsService, 'getMetricsEngineStatus').mockResolvedValue(
      status({ schemaVersion: 99 }),
    );

    render(<MetricsEngineCard />);

    expect(await screen.findByText(/schema v99/)).toBeInTheDocument();
  });

  it('degrades gracefully when the backend is unreachable', async () => {
    vi.spyOn(metricsService, 'getMetricsEngineStatus').mockRejectedValue(
      new Error('PULSE backend is not available'),
    );

    render(<MetricsEngineCard />);

    expect(await screen.findByText(/Engine unavailable/)).toBeInTheDocument();
  });
});
