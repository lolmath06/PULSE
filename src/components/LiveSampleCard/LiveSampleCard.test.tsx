import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Availability, MetricRef, MetricSample } from '@/types/metrics';
import { LiveSampleCard } from '@/components/LiveSampleCard/LiveSampleCard';
import * as metricsService from '@/services/metrics';
import {
  CPU_USAGE_TOTAL,
  MEMORY_AVAILABLE,
  MEMORY_TOTAL,
  MEMORY_USAGE_PERCENT,
  MEMORY_USED,
} from '@/types/wellknown';

const TIMESTAMP = Date.UTC(2026, 0, 15, 12, 34, 56);

function numberSample(metric: MetricRef, value: number, timestamp = TIMESTAMP): MetricSample {
  return {
    metric,
    timestamp,
    value: { type: 'number', value },
    availability: { status: 'available' },
  };
}

function unavailableSample(metric: MetricRef, availability: Availability): MetricSample {
  return { metric, timestamp: TIMESTAMP, value: null, availability };
}

/**
 * A coherent backend response. Values are arbitrary fixtures, deliberately not
 * this machine's real figures — a UI test must not depend on the host.
 */
function healthyResponse(cpuPercent = 18.4, timestamp = TIMESTAMP): MetricSample[] {
  const total = 34_359_738_368; // 32 GiB
  const available = 21_474_836_480; // 20 GiB
  const used = total - available;

  return [
    numberSample(CPU_USAGE_TOTAL, cpuPercent, timestamp),
    numberSample(MEMORY_TOTAL, total, timestamp),
    numberSample(MEMORY_USED, used, timestamp),
    numberSample(MEMORY_AVAILABLE, available, timestamp),
    numberSample(MEMORY_USAGE_PERCENT, (used / total) * 100, timestamp),
  ];
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('LiveSampleCard', () => {
  it('shows a loading state before the first response', async () => {
    let resolve: ((samples: MetricSample[]) => void) | undefined;
    vi.spyOn(metricsService, 'sampleMetrics').mockReturnValue(
      new Promise((r) => {
        resolve = r;
      }),
    );

    render(<LiveSampleCard />);
    expect(screen.getByText('Sampling…')).toBeInTheDocument();

    // Let the request settle so the component is not left mid-update.
    resolve?.(healthyResponse());
    await screen.findByLabelText('Live system sample');
    await waitFor(() => expect(screen.queryByText('Sampling…')).not.toBeInTheDocument());
  });

  it('renders real values from the backend response', async () => {
    vi.spyOn(metricsService, 'sampleMetrics').mockResolvedValue(healthyResponse());

    render(<LiveSampleCard />);

    const card = await screen.findByLabelText('Live system sample');
    await waitFor(() => expect(card).toHaveTextContent('18.4 %'));

    // 32 GiB total, 12 GiB used, 20 GiB available.
    expect(card).toHaveTextContent('12.0 / 32.0 GiB');
    expect(card).toHaveTextContent('20.0 GiB');
    expect(card).toHaveTextContent('37.5 %');
  });

  it('displays the sample timestamp', async () => {
    vi.spyOn(metricsService, 'sampleMetrics').mockResolvedValue(healthyResponse());

    render(<LiveSampleCard />);

    const card = await screen.findByLabelText('Live system sample');
    await waitFor(() => expect(card).toHaveTextContent(/Updated \d{1,2}[:.]\d{2}[:.]\d{2}/));
  });

  it('says CPU is waiting rather than showing 0 %', async () => {
    // The first sample after startup has no delta yet. Reporting 0% would be
    // indistinguishable from a genuinely idle machine.
    const response = healthyResponse();
    response[0] = unavailableSample(CPU_USAGE_TOTAL, {
      status: 'temporarilyUnavailable',
      reason: 'CPU usage is measured between two samples',
    });
    vi.spyOn(metricsService, 'sampleMetrics').mockResolvedValue(response);

    render(<LiveSampleCard />);

    const card = await screen.findByLabelText('Live system sample');
    await waitFor(() => expect(card).toHaveTextContent('Waiting for next sample'));
    expect(card).not.toHaveTextContent('0.0 %');

    // Memory is unaffected: it is an instantaneous reading.
    expect(card).toHaveTextContent('12.0 / 32.0 GiB');
  });

  it('explains a provider error instead of showing a value', async () => {
    const response = healthyResponse();
    response[1] = unavailableSample(MEMORY_TOTAL, {
      status: 'providerError',
      error: { code: 'io', message: '/proc/meminfo unreadable', recoverable: true },
    });
    vi.spyOn(metricsService, 'sampleMetrics').mockResolvedValue(response);

    render(<LiveSampleCard />);

    const card = await screen.findByLabelText('Live system sample');
    await waitFor(() => expect(card).toHaveTextContent('/proc/meminfo unreadable'));
  });

  it('surfaces a permission problem as something the user can act on', async () => {
    const response = healthyResponse();
    response[4] = unavailableSample(MEMORY_USAGE_PERCENT, {
      status: 'permissionDenied',
      reason: 'needs elevation',
    });
    vi.spyOn(metricsService, 'sampleMetrics').mockResolvedValue(response);

    render(<LiveSampleCard />);

    const card = await screen.findByLabelText('Live system sample');
    await waitFor(() => expect(card).toHaveTextContent('Permission required: needs elevation'));
  });

  it('requests a new sample when Refresh is clicked', async () => {
    const sampleMetrics = vi
      .spyOn(metricsService, 'sampleMetrics')
      .mockResolvedValueOnce(healthyResponse(18.4, TIMESTAMP))
      .mockResolvedValueOnce(healthyResponse(62.1, TIMESTAMP + 5_000));

    render(<LiveSampleCard />);

    const card = await screen.findByLabelText('Live system sample');
    await waitFor(() => expect(card).toHaveTextContent('18.4 %'));
    expect(sampleMetrics).toHaveBeenCalledTimes(1);

    await userEvent.click(screen.getByRole('button', { name: 'Refresh sample' }));

    await waitFor(() => expect(card).toHaveTextContent('62.1 %'));
    expect(sampleMetrics).toHaveBeenCalledTimes(2);
    expect(card).not.toHaveTextContent('18.4 %');
  });

  it('does not poll on its own', async () => {
    // PULSE has no scheduler yet; a hidden interval here would be one.
    const sampleMetrics = vi
      .spyOn(metricsService, 'sampleMetrics')
      .mockResolvedValue(healthyResponse());

    render(<LiveSampleCard />);
    const card = await screen.findByLabelText('Live system sample');
    await waitFor(() => expect(card).toHaveTextContent('18.4 %'));

    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      await vi.advanceTimersByTimeAsync(30_000);
    } finally {
      vi.useRealTimers();
    }

    expect(sampleMetrics).toHaveBeenCalledTimes(1);
  });

  it('degrades gracefully when the backend is unreachable', async () => {
    vi.spyOn(metricsService, 'sampleMetrics').mockRejectedValue(
      new Error('PULSE backend is not available'),
    );

    render(<LiveSampleCard />);

    expect(await screen.findByText(/Backend unavailable/)).toBeInTheDocument();
  });

  it('never fabricates a hardware reading', async () => {
    // Every metric unavailable: the card must show reasons, not zeros.
    const response = [
      CPU_USAGE_TOTAL,
      MEMORY_TOTAL,
      MEMORY_USED,
      MEMORY_AVAILABLE,
      MEMORY_USAGE_PERCENT,
    ].map((metric) => unavailableSample(metric, { status: 'notDetected', reason: 'no source' }));
    vi.spyOn(metricsService, 'sampleMetrics').mockResolvedValue(response);

    render(<LiveSampleCard />);

    const card = await screen.findByLabelText('Live system sample');
    await waitFor(() => expect(card).toHaveTextContent('Not detected: no source'));
    expect(card).not.toHaveTextContent('0.0 %');
    expect(card).not.toHaveTextContent('0 B');
  });
});
