import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Availability, MetricDefinition, MetricRef, MetricSample } from '@/types/metrics';
import {
  GPU_FREQUENCY_CORE_KEY,
  GPU_FREQUENCY_MEMORY_KEY,
  GPU_MEMORY_FREE_KEY,
  GPU_MEMORY_TOTAL_KEY,
  GPU_MEMORY_USAGE_PERCENT_KEY,
  GPU_MEMORY_USED_KEY,
  GPU_PER_DEVICE_KEYS,
  GPU_USAGE_CORE_KEY,
} from '@/types/wellknown';
import { GpuDetailsCard } from '@/components/GpuDetailsCard/GpuDetailsCard';
import * as metricsService from '@/services/metrics';

const AVAILABLE: Availability = { status: 'available' };
const GIB = 1024 * 1024 * 1024;

interface FakeGpu {
  readonly sourceId: string;
  readonly label: string;
}

function definition(key: string, sourceId: string, sourceLabel: string): MetricDefinition {
  return {
    metric: { key, sourceId },
    sourceLabel,
    displayName: key,
    description: '',
    category: 'gpu',
    unit: key.startsWith('gpu.frequency')
      ? 'hertz'
      : key.startsWith('gpu.memory.usage') || key === GPU_USAGE_CORE_KEY
        ? 'percent'
        : 'bytes',
    valueType: 'number',
    kind: 'gauge',
    availability: AVAILABLE,
    providerId: 'linux.gpu',
  };
}

function catalogFor(gpus: readonly FakeGpu[]): MetricDefinition[] {
  const entries = gpus.flatMap((gpu) =>
    GPU_PER_DEVICE_KEYS.map((key) => definition(key, gpu.sourceId, gpu.label)),
  );
  entries.push(definition('gpu.count', 'gpu:system', 'Graphics'));

  return entries.sort((left, right) =>
    `${left.metric.key}@${left.metric.sourceId}`.localeCompare(
      `${right.metric.key}@${right.metric.sourceId}`,
    ),
  );
}

function respond(
  requested: readonly MetricRef[],
  value: (metric: MetricRef) => number | Availability,
): MetricSample[] {
  return requested.map((metric) => {
    const result = value(metric);

    return typeof result === 'number'
      ? {
          metric,
          timestamp: 1_700_000_000_000,
          value: { type: 'number', value: result },
          availability: AVAILABLE,
        }
      : { metric, timestamp: 1_700_000_000_000, value: null, availability: result };
  });
}

/** A fully telemetered 8 GiB card. */
function healthy(metric: MetricRef, gpuCount: number): number | Availability {
  switch (metric.key) {
    case 'gpu.count':
      return gpuCount;
    case GPU_USAGE_CORE_KEY:
      return 17;
    case GPU_MEMORY_TOTAL_KEY:
      return 8 * GIB;
    case GPU_MEMORY_USED_KEY:
      return 1.8 * GIB;
    case GPU_MEMORY_FREE_KEY:
      return 6.2 * GIB;
    case GPU_MEMORY_USAGE_PERCENT_KEY:
      return 22.5;
    case GPU_FREQUENCY_CORE_KEY:
      return 2_100_000_000;
    case GPU_FREQUENCY_MEMORY_KEY:
      return 8_001_000_000;
    default:
      return { status: 'notRegistered', reason: 'unexpected' };
  }
}

function mockBackend(
  gpus: readonly FakeGpu[],
  value: (metric: MetricRef) => number | Availability = (metric) => healthy(metric, gpus.length),
) {
  const catalog = vi.spyOn(metricsService, 'getMetricCatalog').mockResolvedValue(catalogFor(gpus));
  const sample = vi
    .spyOn(metricsService, 'sampleMetrics')
    .mockImplementation((requested) => Promise.resolve(respond(requested, value)));

  return { catalog, sample };
}

const NVIDIA: FakeGpu = {
  sourceId: 'gpu:nvidia-11111111-2222-3333-4444-555555555555',
  label: 'NVIDIA GeForce RTX 4070',
};

async function entries() {
  const list = await screen.findByLabelText('Graphics adapters');
  return within(list).getAllByRole('listitem');
}

function entry(listed: readonly HTMLElement[], index: number): HTMLElement {
  const found = listed[index];
  if (!found) throw new Error(`no GPU entry at index ${index}`);
  return found;
}

describe('GpuDetailsCard', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it('shows a loading state before the first response', async () => {
    mockBackend([NVIDIA]);

    render(<GpuDetailsCard />);

    expect(screen.getByText(/Discovering graphics adapters/)).toBeInTheDocument();
    // Let the mount request settle, so the state update it causes is not
    // reported as happening outside `act`.
    await entries();
  });

  it('reports a machine with no GPU honestly', async () => {
    mockBackend([]);

    render(<GpuDetailsCard />);

    const card = await screen.findByLabelText('GPU details');
    await waitFor(() =>
      expect(card).toHaveTextContent(/No hardware graphics adapter was detected/),
    );
    // Zero is a fact here, and the count row says so.
    expect(card).toHaveTextContent('Graphics adapters');
  });

  it('discovers one GPU from the catalog rather than hardcoding it', async () => {
    mockBackend([NVIDIA]);

    render(<GpuDetailsCard />);

    const listed = await entries();
    expect(listed).toHaveLength(1);
    expect(entry(listed, 0)).toHaveTextContent('NVIDIA GeForce RTX 4070');
  });

  it('shows several GPUs, in deterministic order', async () => {
    mockBackend([
      { sourceId: 'gpu:nvidia-bbbb', label: 'NVIDIA GeForce RTX 4090' },
      { sourceId: 'gpu:amd-73ff-0-0', label: 'AMD Radeon RX 6600' },
      { sourceId: 'gpu:nvidia-aaaa', label: 'NVIDIA GeForce RTX 4070' },
    ]);

    render(<GpuDetailsCard />);

    const listed = await entries();
    expect(listed).toHaveLength(3);
    // Sorted by sourceId, so the order does not follow driver enumeration.
    expect(entry(listed, 0)).toHaveTextContent('AMD Radeon RX 6600');
    expect(entry(listed, 1)).toHaveTextContent('NVIDIA GeForce RTX 4070');
    expect(entry(listed, 2)).toHaveTextContent('NVIDIA GeForce RTX 4090');
  });

  it('numbers identical cards so their rows can be told apart', async () => {
    mockBackend([
      { sourceId: 'gpu:nvidia-aaaa', label: 'NVIDIA GeForce RTX 4090' },
      { sourceId: 'gpu:nvidia-bbbb', label: 'NVIDIA GeForce RTX 4090' },
    ]);

    render(<GpuDetailsCard />);

    const listed = await entries();
    expect(entry(listed, 0)).toHaveTextContent('NVIDIA GeForce RTX 4090 #1');
    expect(entry(listed, 1)).toHaveTextContent('NVIDIA GeForce RTX 4090 #2');
  });

  it('renders every metric of a fully supported GPU', async () => {
    mockBackend([NVIDIA]);

    render(<GpuDetailsCard />);

    const only = entry(await entries(), 0);
    expect(only).toHaveTextContent('17.0 %');
    expect(only).toHaveTextContent('22.5 %');
    // Bytes formatted for display; hertz rendered as GHz.
    expect(only).toHaveTextContent('8.0 GiB');
    expect(only).toHaveTextContent('2.10 GHz');
    expect(only).toHaveTextContent('8.00 GHz');
  });

  it('never shows raw hertz or raw bytes', async () => {
    mockBackend([NVIDIA]);

    render(<GpuDetailsCard />);

    const only = entry(await entries(), 0);
    expect(only).not.toHaveTextContent('2100000000');
    expect(only).not.toHaveTextContent('8589934592');
  });

  it('shows a dash for a GPU with no telemetry at all, never zeros', async () => {
    // The real case on a machine running an open-source driver: the card is
    // present and named, and nothing can be measured.
    const reason: Availability = {
      status: 'unsupported',
      reason: 'libnvidia-ml.so.1 is not installed',
    };
    mockBackend([NVIDIA], (metric) => (metric.key === 'gpu.count' ? 1 : reason));

    render(<GpuDetailsCard />);

    const only = entry(await entries(), 0);
    expect(only).toHaveTextContent('NVIDIA GeForce RTX 4070');
    expect(only).not.toHaveTextContent('0 %');
    expect(only).not.toHaveTextContent('0 GiB');
    expect(only).not.toHaveTextContent('0 GHz');
    expect(only).toHaveTextContent('—');
    // The reason survives into the tooltip.
    expect(within(only).getAllByTitle(/libnvidia-ml/).length).toBeGreaterThan(0);
  });

  it('shows a partially supported GPU without losing what does work', async () => {
    mockBackend([NVIDIA], (metric) => {
      if (metric.key === GPU_FREQUENCY_MEMORY_KEY) {
        return { status: 'unsupported', reason: 'no memory clock on this driver' };
      }
      return healthy(metric, 1);
    });

    render(<GpuDetailsCard />);

    const only = entry(await entries(), 0);
    expect(only).toHaveTextContent('17.0 %');
    expect(only).toHaveTextContent('2.10 GHz');
    expect(within(only).getByTitle(/no memory clock/)).toBeInTheDocument();
  });

  it('shows a VRAM capacity without inventing a usage figure', async () => {
    // The DXGI-only case: the installed capacity is known, live usage is not,
    // and PULSE must not print a zero in its place.
    mockBackend([NVIDIA], (metric) => {
      if (metric.key === 'gpu.count') return 1;
      if (metric.key === GPU_MEMORY_TOTAL_KEY) return 8 * GIB;
      return {
        status: 'unsupported',
        reason: 'no system-wide figure is available for this adapter',
      };
    });

    render(<GpuDetailsCard />);

    const only = entry(await entries(), 0);
    expect(only).toHaveTextContent('8.0 GiB');
    expect(only).not.toHaveTextContent('0 %');
    expect(only).toHaveTextContent(/Installed VRAM is known/);
  });

  it('explains a permission problem differently from an absent sensor', async () => {
    mockBackend([NVIDIA], (metric) =>
      metric.key === GPU_USAGE_CORE_KEY
        ? { status: 'permissionDenied', reason: 'the driver refused this query' }
        : healthy(metric, 1),
    );

    render(<GpuDetailsCard />);

    const only = entry(await entries(), 0);
    expect(within(only).getByTitle(/Permission required/)).toBeInTheDocument();
  });

  it('requests only the metrics it displays', async () => {
    const { sample } = mockBackend([NVIDIA]);

    render(<GpuDetailsCard />);
    await entries();

    const requested = sample.mock.calls[0]?.[0] ?? [];
    // One count plus seven metrics for the single GPU.
    expect(requested).toHaveLength(8);
    expect(requested.some((metric) => metric.key.startsWith('cpu.'))).toBe(false);
  });

  it('takes a fresh sample when Refresh is clicked', async () => {
    const { sample, catalog } = mockBackend([NVIDIA]);

    render(<GpuDetailsCard />);
    await entries();
    expect(sample).toHaveBeenCalledTimes(1);

    await userEvent.click(screen.getByLabelText('Refresh GPU details'));

    await waitFor(() => expect(sample).toHaveBeenCalledTimes(2));
    // Identity and capabilities are static; only the values are re-read.
    expect(catalog).toHaveBeenCalledTimes(1);
  });

  it('updates the timestamp on refresh', async () => {
    let timestamp = 1_700_000_000_000;
    vi.spyOn(metricsService, 'getMetricCatalog').mockResolvedValue(catalogFor([NVIDIA]));
    vi.spyOn(metricsService, 'sampleMetrics').mockImplementation((requested) => {
      timestamp += 60_000;
      return Promise.resolve(
        requested.map((metric) => ({
          metric,
          timestamp,
          value: { type: 'number' as const, value: 17 },
          availability: AVAILABLE,
        })),
      );
    });

    render(<GpuDetailsCard />);
    const card = await screen.findByLabelText('GPU details');
    await waitFor(() => expect(card).toHaveTextContent(/Updated/));
    const before = card.textContent;

    await userEvent.click(screen.getByLabelText('Refresh GPU details'));

    await waitFor(() => expect(card.textContent).not.toBe(before));
  });

  it('never polls on its own', async () => {
    const { sample } = mockBackend([NVIDIA]);

    render(<GpuDetailsCard />);
    await entries();
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

    render(<GpuDetailsCard />);

    expect(await screen.findByText(/Backend unavailable/)).toBeInTheDocument();
  });

  it('keeps VRAM semantics visible to the user', async () => {
    mockBackend([NVIDIA]);

    render(<GpuDetailsCard />);
    const card = await screen.findByLabelText('GPU details');
    await entries();

    expect(card).toHaveTextContent(/dedicated video memory/);
  });
});
