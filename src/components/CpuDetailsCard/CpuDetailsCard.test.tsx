import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Availability, MetricDefinition, MetricRef, MetricSample } from '@/types/metrics';
import {
  CPU_FREQUENCY_CURRENT_KEY,
  CPU_FREQUENCY_MAX_KEY,
  CPU_USAGE_LOGICAL_KEY,
  cpuLogicalSourceId,
} from '@/types/wellknown';
import { CpuDetailsCard } from '@/components/CpuDetailsCard/CpuDetailsCard';
import * as metricsService from '@/services/metrics';

const AVAILABLE: Availability = { status: 'available' };

function definition(key: string, sourceId: string, sourceLabel: string): MetricDefinition {
  return {
    metric: { key, sourceId },
    sourceLabel,
    displayName: key,
    description: '',
    category: 'cpu',
    unit: key.startsWith('cpu.frequency') ? 'hertz' : 'percent',
    valueType: 'number',
    kind: key.startsWith('cpu.count') ? 'state' : 'gauge',
    availability: AVAILABLE,
    providerId: 'linux.cpu',
  };
}

/** A catalog for `count` logical processors, in the backend's own order. */
function catalogFor(count: number): MetricDefinition[] {
  const entries = [...Array(count).keys()].flatMap((ordinal) =>
    [CPU_USAGE_LOGICAL_KEY, CPU_FREQUENCY_CURRENT_KEY, CPU_FREQUENCY_MAX_KEY].map((key) =>
      definition(key, cpuLogicalSourceId(ordinal), `CPU ${ordinal}`),
    ),
  );

  for (const key of ['cpu.count.logical', 'cpu.count.physical', 'cpu.count.package']) {
    entries.push(definition(key, 'cpu:system', 'System CPU'));
  }

  return entries.sort((left, right) =>
    `${left.metric.key}@${left.metric.sourceId}`.localeCompare(
      `${right.metric.key}@${right.metric.sourceId}`,
    ),
  );
}

function number(metric: MetricRef, value: number): MetricSample {
  return {
    metric,
    timestamp: 1_700_000_000_000,
    value: { type: 'number', value },
    availability: AVAILABLE,
  };
}

function unavailable(metric: MetricRef, availability: Availability): MetricSample {
  return { metric, timestamp: 1_700_000_000_000, value: null, availability };
}

/**
 * Answers a sample request the way the backend would: one sample per requested
 * reference, in order.
 */
function respond(
  requested: readonly MetricRef[],
  value: (metric: MetricRef) => number | Availability,
): MetricSample[] {
  return requested.map((metric) => {
    const result = value(metric);
    return typeof result === 'number' ? number(metric, result) : unavailable(metric, result);
  });
}

/** A plausible machine: 4 logical processors on 2 cores in 1 package. */
function defaultValue(metric: MetricRef): number | Availability {
  switch (metric.key) {
    case 'cpu.count.logical':
      return 4;
    case 'cpu.count.physical':
      return 2;
    case 'cpu.count.package':
      return 1;
    case CPU_USAGE_LOGICAL_KEY:
      return 12.5;
    case CPU_FREQUENCY_CURRENT_KEY:
      return 3_200_000_000;
    case CPU_FREQUENCY_MAX_KEY:
      return 4_800_000_000;
    default:
      return { status: 'notRegistered', reason: 'unexpected' };
  }
}

function mockBackend(
  count = 4,
  value: (metric: MetricRef) => number | Availability = defaultValue,
) {
  const catalog = vi.spyOn(metricsService, 'getMetricCatalog').mockResolvedValue(catalogFor(count));
  const sample = vi
    .spyOn(metricsService, 'sampleMetrics')
    .mockImplementation((requested) => Promise.resolve(respond(requested, value)));

  return { catalog, sample };
}

async function rows() {
  const list = await screen.findByLabelText('Logical processors');
  return within(list).getAllByRole('listitem');
}

/** One row, asserted to exist — `noUncheckedIndexedAccess` is on. */
function row(listed: readonly HTMLElement[], index: number): HTMLElement {
  const found = listed[index];
  if (!found) throw new Error(`no row at index ${index}`);
  return found;
}

describe('CpuDetailsCard', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it('shows the real topology counts', async () => {
    mockBackend();

    render(<CpuDetailsCard />);

    const card = await screen.findByLabelText('CPU details');
    await waitFor(() => expect(card).toHaveTextContent('Physical cores'));

    expect(card).toHaveTextContent('Logical processors');
    expect(card).toHaveTextContent('Packages');
    // The values, not just the labels.
    const counts = within(card).getByText('Physical cores').closest('.kv') as HTMLElement;
    expect(counts).toHaveTextContent('2');
    expect(counts).toHaveTextContent('4');
    expect(counts).toHaveTextContent('1');
  });

  it('discovers the processor list from the catalog rather than hardcoding it', async () => {
    mockBackend(8);

    render(<CpuDetailsCard />);

    expect(await rows()).toHaveLength(8);
  });

  it('lists processors in numeric order, not lexicographic', async () => {
    // The bug: CPU 1, CPU 10, CPU 11, CPU 2 …
    mockBackend(12);

    render(<CpuDetailsCard />);

    const labels = (await rows()).map((row) => row.querySelector('.cpu-grid__name')?.textContent);
    expect(labels).toEqual([
      'CPU 0',
      'CPU 1',
      'CPU 2',
      'CPU 3',
      'CPU 4',
      'CPU 5',
      'CPU 6',
      'CPU 7',
      'CPU 8',
      'CPU 9',
      'CPU 10',
      'CPU 11',
    ]);
  });

  it('stays correct on a machine with many logical processors', async () => {
    mockBackend(128);

    render(<CpuDetailsCard />);

    const listed = await rows();
    expect(listed).toHaveLength(128);
    expect(row(listed, 0)).toHaveTextContent('CPU 0');
    expect(row(listed, 127)).toHaveTextContent('CPU 127');
  });

  it('renders frequencies in gigahertz while the contract stays in hertz', async () => {
    mockBackend(1);

    render(<CpuDetailsCard />);

    const only = row(await rows(), 0);
    expect(only).toHaveTextContent('3.20 GHz');
    // The raw hertz value must never reach the screen.
    expect(only).not.toHaveTextContent('3200000000');
  });

  it('shows an unknown frequency as a dash, never as 0 GHz', async () => {
    mockBackend(2, (metric) => {
      if (metric.key === CPU_FREQUENCY_CURRENT_KEY && metric.sourceId === 'cpu:logical-1') {
        return { status: 'unsupported', reason: 'no cpufreq interface' };
      }
      return defaultValue(metric);
    });

    render(<CpuDetailsCard />);

    const second = row(await rows(), 1);
    expect(second).not.toHaveTextContent('0 GHz');
    expect(second).not.toHaveTextContent('0.00 GHz');
    expect(second).toHaveTextContent('—');
    // The reason survives into the tooltip rather than being thrown away.
    expect(within(second).getByTitle(/no cpufreq interface/)).toBeInTheDocument();
  });

  it('keeps usage when a processor has no frequency', async () => {
    // A cpufreq problem on one processor must not cost its usage.
    mockBackend(2, (metric) => {
      if (metric.key.startsWith('cpu.frequency') && metric.sourceId === 'cpu:logical-1') {
        return { status: 'unsupported', reason: 'no cpufreq interface' };
      }
      return defaultValue(metric);
    });

    render(<CpuDetailsCard />);

    expect(row(await rows(), 1)).toHaveTextContent('12.5 %');
  });

  it('says it is waiting rather than claiming a core is idle', async () => {
    mockBackend(1, (metric) =>
      metric.key === CPU_USAGE_LOGICAL_KEY
        ? { status: 'temporarilyUnavailable', reason: 'waiting for the next sample' }
        : defaultValue(metric),
    );

    render(<CpuDetailsCard />);

    const only = row(await rows(), 0);
    expect(only).not.toHaveTextContent('0.0 %');
    expect(within(only).getByTitle(/waiting for the next sample/)).toBeInTheDocument();
  });

  it('explains a topology count it could not obtain', async () => {
    mockBackend(2, (metric) =>
      metric.key === 'cpu.count.physical'
        ? { status: 'notDetected', reason: 'no CPU core topology' }
        : defaultValue(metric),
    );

    render(<CpuDetailsCard />);

    const card = await screen.findByLabelText('CPU details');
    await waitFor(() =>
      expect(within(card).getByTitle(/no CPU core topology/)).toBeInTheDocument(),
    );
    expect(card).toHaveTextContent('Not reported');
  });

  it('requests only the metrics it displays', async () => {
    const { sample } = mockBackend(4);

    render(<CpuDetailsCard />);
    await rows();

    const requested = sample.mock.calls[0]?.[0] ?? [];
    // 3 topology counts + 3 metrics for each of 4 processors.
    expect(requested).toHaveLength(15);
    expect(requested.some((metric) => metric.key.startsWith('memory.'))).toBe(false);
  });

  it('takes a fresh sample when Refresh is clicked', async () => {
    const { sample, catalog } = mockBackend(2);

    render(<CpuDetailsCard />);
    await rows();
    expect(sample).toHaveBeenCalledTimes(1);

    await userEvent.click(screen.getByLabelText('Refresh CPU details'));

    await waitFor(() => expect(sample).toHaveBeenCalledTimes(2));
    // The catalog is stable; only the values are re-read.
    expect(catalog).toHaveBeenCalledTimes(1);
  });

  it('never polls on its own', async () => {
    // A hidden interval here would be a scheduler in disguise. PULSE has none
    // yet, and this card must not smuggle one in.
    const { sample } = mockBackend(4);

    render(<CpuDetailsCard />);
    await rows();
    expect(sample).toHaveBeenCalledTimes(1);

    vi.useFakeTimers();
    await vi.advanceTimersByTimeAsync(60_000);
    vi.useRealTimers();

    expect(sample).toHaveBeenCalledTimes(1);
  });

  it('degrades gracefully when the backend is unreachable', async () => {
    vi.spyOn(metricsService, 'getMetricCatalog').mockRejectedValue(
      new Error('PULSE backend is not available'),
    );

    render(<CpuDetailsCard />);

    expect(await screen.findByText(/Backend unavailable/)).toBeInTheDocument();
  });

  it('handles a machine that exposes no individual processors', async () => {
    vi.spyOn(metricsService, 'getMetricCatalog').mockResolvedValue([
      definition('cpu.count.logical', 'cpu:system', 'System CPU'),
    ]);
    vi.spyOn(metricsService, 'sampleMetrics').mockImplementation((requested) =>
      Promise.resolve(respond(requested, defaultValue)),
    );

    render(<CpuDetailsCard />);

    expect(await screen.findByText(/No individual logical processor/)).toBeInTheDocument();
  });

  it('keeps logical and physical distinct in what it tells the user', async () => {
    mockBackend(4);

    render(<CpuDetailsCard />);
    const card = await screen.findByLabelText('CPU details');
    await rows();

    expect(card).toHaveTextContent('Physical cores');
    expect(card).toHaveTextContent('Logical processors');
    expect(card).toHaveTextContent(/several logical processors/);
  });
});
