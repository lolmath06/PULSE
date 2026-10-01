import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { MockInstance } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Availability, MetricDefinition, MetricRef } from '@/types/metrics';
import type { HistoryRange } from '@/types/history';
import {
  CPU_TEMPERATURE_PACKAGE_KEY,
  CPU_USAGE_LOGICAL_KEY,
  CPU_USAGE_TOTAL,
  CPU_USAGE_TOTAL_KEY,
  GPU_TEMPERATURE_CORE_KEY,
  GPU_USAGE_CORE_KEY,
  NETWORK_MTU_KEY,
  NETWORK_RECEIVE_BYTES_KEY,
  NETWORK_TRANSMIT_BYTES_KEY,
  STORAGE_CAPACITY_TOTAL_KEY,
  cpuLogicalSourceId,
  metricRefId,
} from '@/types/wellknown';
import * as metricsService from '@/services/metrics';
import * as historyService from '@/services/history';
import { emitHistorySampleForTesting, historySubscriberCount } from '@/services/history';
import { resetMetricCatalogForTesting } from '@/hooks/useMetricCatalog';
import { resetUiConfigForTesting } from '@/config/uiConfig';
import { reloadVisualizationStoreForTesting } from '@/visualization/store';
import { CpuHistory } from '@/components/History/CpuHistory';
import { GpuHistory } from '@/components/History/GpuHistory';
import { NetworkHistory } from '@/components/History/NetworkHistory';
import { StorageHistory } from '@/components/History/StorageHistory';
import { ThermalHistory } from '@/components/History/ThermalHistory';
import {
  CPU_TOTAL_SERIES,
  defaultInterface,
  networkSeries,
  storageSeries,
  thermalSeries,
} from '@/components/History/series';
import { toVisualizationData } from '@/utils/history';
import { T0, historyResponse } from '@/test/visualization';

const AVAILABLE: Availability = { status: 'available' };

function definition(
  key: string,
  sourceId: string,
  sourceLabel = sourceId,
  availability: Availability = AVAILABLE,
): MetricDefinition {
  return {
    metric: { key, sourceId },
    sourceLabel,
    displayName: key,
    description: '',
    category: 'cpu',
    unit: 'percent',
    valueType: 'number',
    kind: 'gauge',
    availability,
    providerId: 'mock',
  };
}

const CATALOG: MetricDefinition[] = [
  definition(CPU_USAGE_TOTAL_KEY, 'cpu:system', 'CPU'),
  ...[0, 1, 2, 3].map((n) => definition(CPU_USAGE_LOGICAL_KEY, cpuLogicalSourceId(n), `CPU ${n}`)),
  definition(CPU_TEMPERATURE_PACKAGE_KEY, 'cpu:package-0', 'Package 0'),
  definition(GPU_USAGE_CORE_KEY, 'gpu:pci-0000-01-00-0', 'RTX', {
    status: 'unsupported',
    reason: 'no NVML',
  }),
  definition(GPU_TEMPERATURE_CORE_KEY, 'gpu:pci-0000-01-00-0', 'RTX', {
    status: 'unsupported',
    reason: 'no NVML',
  }),
  definition(STORAGE_CAPACITY_TOTAL_KEY, 'storage:nvme0n1', 'Samsung SSD · NVMe'),
  definition(NETWORK_MTU_KEY, 'network:mac-001122334455', 'enp5s0 · Ethernet'),
  definition(NETWORK_MTU_KEY, 'network:mac-66778899aabb', 'wlp3s0 · Wi-Fi'),
  definition(NETWORK_MTU_KEY, 'network:sys-veth0', 'veth0 · Virtual'),
];

/** Answers every request with a gentle ramp, 27.4 % last. */
function answer(metrics: readonly MetricRef[], range: HistoryRange) {
  return Promise.resolve(
    historyResponse(
      metrics.map((metric) => ({
        metric,
        points: [0, 1, 2, 3].map((i) => ({ t: T0 + i * 5_000, v: i === 3 ? 27.4 : 20 + i })),
      })),
      { range },
    ),
  );
}

let historySpy: MockInstance<typeof historyService.getMetricHistory>;

beforeEach(() => {
  resetUiConfigForTesting();
  reloadVisualizationStoreForTesting();
  resetMetricCatalogForTesting();
  vi.spyOn(metricsService, 'getMetricCatalog').mockResolvedValue(CATALOG);
  vi.spyOn(metricsService, 'sampleMetrics').mockResolvedValue([]);
  historySpy = vi.spyOn(historyService, 'getMetricHistory').mockImplementation(answer);
});

afterEach(() => {
  vi.restoreAllMocks();
});

const requested = (call: number) =>
  (historySpy.mock.calls[call]![0] as MetricRef[]).map(metricRefId);

describe('CPU history', () => {
  it('CPU Total plots cpu.usage.total and nothing recomputed from logical processors', async () => {
    render(<CpuHistory />);

    await waitFor(() => expect(historySpy).toHaveBeenCalled());
    expect(requested(0)).toEqual([metricRefId(CPU_USAGE_TOTAL)]);
    expect(requested(0).some((id) => id.startsWith(CPU_USAGE_LOGICAL_KEY))).toBe(false);
    expect(CPU_TOTAL_SERIES.map((spec) => spec.ref)).toEqual([CPU_USAGE_TOTAL]);

    await waitFor(() =>
      expect(screen.getByRole('region', { name: 'CPU history' }).textContent).toContain('27.4'),
    );
  });

  it('never polls: it reloads only when the backend records a batch', async () => {
    // Checked before any `waitFor`, which itself polls with setInterval.
    const intervals = vi.spyOn(globalThis, 'setInterval');
    render(<CpuHistory />);
    await act(async () => {
      for (let i = 0; i < 5; i += 1) await Promise.resolve();
    });
    expect(intervals).not.toHaveBeenCalled();
    intervals.mockRestore();

    await waitFor(() => expect(historySpy).toHaveBeenCalledTimes(1));
    expect(historySubscriberCount()).toBe(1);

    act(() => emitHistorySampleForTesting({ batchId: 2, timestampMs: T0, rowCount: 90 }));
    await waitFor(() => expect(historySpy).toHaveBeenCalledTimes(2));
  });

  it('changing the range re-queries with that range and remembers it', async () => {
    const user = userEvent.setup();
    const { unmount } = render(<CpuHistory />);
    await waitFor(() => expect(historySpy).toHaveBeenCalledTimes(1));

    const panel = screen.getByRole('region', { name: 'CPU history' });
    await user.click(within(panel).getByRole('button', { name: '6h' }));
    await waitFor(() => expect(historySpy.mock.calls.at(-1)![1]).toBe('6h'));

    unmount();
    reloadVisualizationStoreForTesting();
    render(<CpuHistory />);
    await waitFor(() => expect(historySpy.mock.calls.at(-1)![1]).toBe('6h'));
  });

  it('the logical view is small multiples of the most active processors', async () => {
    const user = userEvent.setup();
    render(<CpuHistory />);
    await user.click(await screen.findByRole('button', { name: 'Logical processors' }));

    await waitFor(() =>
      expect(historySpy.mock.calls.some((call) => (call[0] as MetricRef[]).length === 4)).toBe(
        true,
      ),
    );
    const grid = await screen.findByRole('list', { name: 'Logical processor history' });
    expect(within(grid).getAllByRole('listitem')).toHaveLength(4);
  });
});

describe('other history sections', () => {
  it('GPU without telemetry says so instead of drawing an idle chart', async () => {
    render(<GpuHistory />);
    expect(
      await screen.findByText(/Telemetry unavailable — Not supported: no NVML/),
    ).toBeInTheDocument();
    expect(historySpy).not.toHaveBeenCalled();
  });

  it('thermal charts only readable sensors and lists the rest', async () => {
    render(<ThermalHistory />);
    await waitFor(() => expect(historySpy).toHaveBeenCalled());
    expect(requested(0)).toEqual([`${CPU_TEMPERATURE_PACKAGE_KEY}@cpu:package-0`]);
    expect(
      await screen.findByText(/Not charted — GPU: Not supported: no NVML/),
    ).toBeInTheDocument();
  });

  it('storage asks for read and write of the device', async () => {
    render(<StorageHistory />);
    await waitFor(() => expect(historySpy).toHaveBeenCalled());
    expect(requested(0)).toEqual([
      'storage.io.read.bytes_per_second@storage:nvme0n1',
      'storage.io.write.bytes_per_second@storage:nvme0n1',
    ]);
  });

  it('network charts download and upload of a hardware interface by default', async () => {
    render(<NetworkHistory />);
    await waitFor(() =>
      expect(
        historySpy.mock.calls.some(
          (call) => (call[0] as MetricRef[])[1]?.key === NETWORK_TRANSMIT_BYTES_KEY,
        ),
      ).toBe(true),
    );
    const chartCall = historySpy.mock.calls.find(
      (call) => (call[0] as MetricRef[])[1]?.key === NETWORK_TRANSMIT_BYTES_KEY,
    )!;
    const refs = chartCall[0] as MetricRef[];
    expect(refs.map((ref) => ref.key)).toEqual([
      NETWORK_RECEIVE_BYTES_KEY,
      NETWORK_TRANSMIT_BYTES_KEY,
    ]);
    expect(refs[0]!.sourceId).not.toBe('network:sys-veth0');
  });
});

describe('series selection', () => {
  it('thermal never turns an unavailable sensor into a series', () => {
    const { series, unavailable } = thermalSeries(CATALOG);
    expect(series.map((spec) => spec.label)).toEqual(['CPU']);
    expect(unavailable).toHaveLength(1);
  });

  it('dual series are labelled for what they are', () => {
    expect(storageSeries('storage:x').map((s) => s.label)).toEqual(['Read', 'Write']);
    expect(networkSeries('network:x').map((s) => s.label)).toEqual(['Download', 'Upload']);
  });

  it('the default interface is active hardware, never a virtual one', () => {
    const interfaces = [
      { sourceId: 'a', label: 'veth0', kind: 'Virtual' as const, wireless: false },
      { sourceId: 'b', label: 'enp5s0', kind: 'Ethernet' as const, wireless: false },
      { sourceId: 'c', label: 'wlp3s0', kind: 'Wi-Fi' as const, wireless: true },
    ];
    expect(
      defaultInterface(
        interfaces,
        new Map([
          ['c', 500],
          ['a', 9_999],
        ]),
      )?.sourceId,
    ).toBe('c');
    expect(defaultInterface(interfaces, new Map())?.sourceId).toBe('b');
    expect(defaultInterface([interfaces[0]!], new Map())?.sourceId).toBe('a');
  });

  it('a missing series becomes an empty one, matched by reference not position', () => {
    const read = { key: 'storage.io.read.bytes_per_second', sourceId: 'storage:x' };
    const write = { key: 'storage.io.write.bytes_per_second', sourceId: 'storage:x' };
    const data = toVisualizationData(
      historyResponse([{ metric: write, points: [{ t: T0, v: 5 }] }]),
      [
        { ref: read, label: 'Read' },
        { ref: write, label: 'Write' },
      ],
      'ready',
    );
    expect(data.series[0]!.points).toEqual([]);
    expect(data.series[0]!.latest).toBeNull();
    expect(data.series[1]!.points).toEqual([{ t: T0, v: 5 }]);
    expect(data.gapThresholdMs).toBe(15_000);
    expect(data.aggregated).toBe(false);
  });
});
