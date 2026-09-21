import { describe, expect, it } from 'vitest';
import type { Availability, MetricDefinition, MetricSample } from '@/types/metrics';
import {
  GPU_FREQUENCY_CORE_KEY,
  GPU_MEMORY_TOTAL_KEY,
  GPU_PERFORMANCE_KEYS,
  GPU_PER_DEVICE_KEYS,
  GPU_TEMPERATURE_CORE_KEY,
  GPU_USAGE_CORE_KEY,
  isGpuDeviceSource,
  metricRefId,
} from '@/types/wellknown';
import { disambiguateLabels, discoverGpus, gpuMetrics, gpuTelemetryState } from '@/utils/gpu';

function definition(key: string, sourceId: string, sourceLabel = sourceId): MetricDefinition {
  return {
    metric: { key, sourceId },
    sourceLabel,
    displayName: key,
    description: '',
    category: 'gpu',
    unit: 'percent',
    valueType: 'number',
    kind: 'gauge',
    availability: { status: 'available' },
    providerId: 'linux.gpu',
  };
}

/** A catalog for the given GPU sources, in the backend's own order. */
function catalogFor(devices: readonly { sourceId: string; label: string }[]): MetricDefinition[] {
  const entries = devices.flatMap((device) =>
    GPU_PER_DEVICE_KEYS.map((key) => definition(key, device.sourceId, device.label)),
  );

  entries.push(definition('gpu.count', 'gpu:system', 'Graphics'));
  entries.push(definition('cpu.usage.total', 'cpu:system', 'System CPU'));
  entries.push(definition('memory.used', 'memory:system', 'System memory'));

  return entries.sort((left, right) =>
    `${left.metric.key}@${left.metric.sourceId}`.localeCompare(
      `${right.metric.key}@${right.metric.sourceId}`,
    ),
  );
}

describe('isGpuDeviceSource', () => {
  it('accepts device sources whatever identity mechanism produced them', () => {
    // The interface treats these as opaque: NVML UUID, PCI address, device
    // model tuple — all equally valid keys.
    for (const sourceId of [
      'gpu:nvidia-11111111-2222-3333-4444-555555555555',
      'gpu:pci-0000-01-00-0',
      'gpu:amd-73ff-12345678-a1',
      'gpu:amd-73ff-12345678-a1-n1',
    ]) {
      expect(isGpuDeviceSource(sourceId)).toBe(true);
    }
  });

  it('excludes the machine-wide aggregate and other domains', () => {
    for (const sourceId of ['gpu:system', 'cpu:system', 'memory:system', 'cpu:logical-0', '']) {
      expect(isGpuDeviceSource(sourceId)).toBe(false);
    }
  });
});

describe('discoverGpus', () => {
  it('finds nothing on a machine with no GPU', () => {
    expect(discoverGpus(catalogFor([]))).toEqual([]);
    expect(discoverGpus([])).toEqual([]);
  });

  it('finds a single GPU', () => {
    const devices = discoverGpus(
      catalogFor([{ sourceId: 'gpu:nvidia-aaaa', label: 'NVIDIA GeForce RTX 4070' }]),
    );

    expect(devices).toHaveLength(1);
    expect(devices[0]?.sourceId).toBe('gpu:nvidia-aaaa');
    expect(devices[0]?.label).toBe('NVIDIA GeForce RTX 4070');
  });

  it('finds several GPUs without any hardcoded list', () => {
    for (const count of [1, 2, 3, 4]) {
      const devices = discoverGpus(
        catalogFor(
          Array.from({ length: count }, (_, index) => ({
            sourceId: `gpu:nvidia-${String(index).padStart(4, '0')}`,
            label: 'NVIDIA GeForce RTX 4090',
          })),
        ),
      );

      expect(devices).toHaveLength(count);
    }
  });

  it('lists each GPU once even though it has seven metrics', () => {
    const devices = discoverGpus(
      catalogFor([
        { sourceId: 'gpu:nvidia-aaaa', label: 'A' },
        { sourceId: 'gpu:nvidia-bbbb', label: 'B' },
      ]),
    );

    expect(devices).toHaveLength(2);
    expect(new Set(devices.map((device) => device.sourceId)).size).toBe(2);
  });

  it('ignores the machine-wide and non-GPU metrics', () => {
    const devices = discoverGpus(catalogFor([{ sourceId: 'gpu:nvidia-aaaa', label: 'A' }]));

    expect(devices).toHaveLength(1);
    expect(devices.every((device) => device.sourceId !== 'gpu:system')).toBe(true);
  });

  it('lists a GPU discovered through any one of its metrics', () => {
    // A card whose telemetry is entirely unsupported still gets a row — the
    // whole point of inventorying a GPU with no backend.
    const catalog = [
      definition(GPU_USAGE_CORE_KEY, 'gpu:pci-0000-01-00-0', 'NVIDIA AD106M'),
      definition(GPU_MEMORY_TOTAL_KEY, 'gpu:pci-0000-01-00-0', 'NVIDIA AD106M'),
    ];

    expect(discoverGpus(catalog)).toHaveLength(1);
  });

  it('orders devices deterministically', () => {
    const devices = [
      { sourceId: 'gpu:nvidia-cccc', label: 'C' },
      { sourceId: 'gpu:nvidia-aaaa', label: 'A' },
      { sourceId: 'gpu:nvidia-bbbb', label: 'B' },
    ];

    const first = discoverGpus(catalogFor(devices)).map((device) => device.sourceId);
    const second = discoverGpus(catalogFor([...devices].reverse())).map(
      (device) => device.sourceId,
    );

    expect(first).toEqual(second);
    expect(first).toEqual(['gpu:nvidia-aaaa', 'gpu:nvidia-bbbb', 'gpu:nvidia-cccc']);
  });

  it('takes the label from the backend rather than inventing one', () => {
    const devices = discoverGpus(
      catalogFor([{ sourceId: 'gpu:amd-73ff-0-0', label: 'AMD Radeon RX 6600' }]),
    );

    expect(devices[0]?.label).toBe('AMD Radeon RX 6600');
  });
});

describe('gpuMetrics', () => {
  it('requests exactly seven references per device', () => {
    const devices = discoverGpus(
      catalogFor([
        { sourceId: 'gpu:nvidia-aaaa', label: 'A' },
        { sourceId: 'gpu:nvidia-bbbb', label: 'B' },
      ]),
    );
    const metrics = gpuMetrics(devices);

    expect(metrics).toHaveLength(14);
    expect(new Set(metrics.map((metric) => `${metric.key}@${metric.sourceId}`)).size).toBe(14);
  });

  it('requests only the devices it was given', () => {
    const metrics = gpuMetrics([{ sourceId: 'gpu:nvidia-aaaa', label: 'A' }]);

    expect(metrics.every((metric) => metric.sourceId === 'gpu:nvidia-aaaa')).toBe(true);
    expect(metrics.map((metric) => metric.key).sort()).toEqual([...GPU_PER_DEVICE_KEYS].sort());
  });

  it('asks for nothing when there is nothing to show', () => {
    expect(gpuMetrics([])).toEqual([]);
  });

  it('never requests a non-GPU metric', () => {
    const metrics = gpuMetrics([{ sourceId: 'gpu:nvidia-aaaa', label: 'A' }]);

    expect(metrics.some((metric) => metric.key.startsWith('cpu.'))).toBe(false);
    expect(metrics.some((metric) => metric.key === GPU_FREQUENCY_CORE_KEY)).toBe(true);
  });
});

describe('disambiguateLabels', () => {
  it('leaves a unique label untouched', () => {
    const labels = disambiguateLabels([
      { sourceId: 'gpu:a', label: 'NVIDIA GeForce RTX 4070' },
      { sourceId: 'gpu:b', label: 'AMD Radeon RX 6600' },
    ]);

    expect(labels).toEqual(['NVIDIA GeForce RTX 4070', 'AMD Radeon RX 6600']);
  });

  it('numbers identical labels so the user can tell the rows apart', () => {
    const labels = disambiguateLabels([
      { sourceId: 'gpu:nvidia-aaaa', label: 'NVIDIA GeForce RTX 4090' },
      { sourceId: 'gpu:nvidia-bbbb', label: 'NVIDIA GeForce RTX 4090' },
    ]);

    expect(labels).toEqual(['NVIDIA GeForce RTX 4090 #1', 'NVIDIA GeForce RTX 4090 #2']);
  });

  it('is presentation only and never alters identity', () => {
    // The labels change; the sourceIds they came from do not.
    const devices = [
      { sourceId: 'gpu:nvidia-aaaa', label: 'NVIDIA GeForce RTX 4090' },
      { sourceId: 'gpu:nvidia-bbbb', label: 'NVIDIA GeForce RTX 4090' },
    ];
    const before = devices.map((device) => device.sourceId);

    disambiguateLabels(devices);

    expect(devices.map((device) => device.sourceId)).toEqual(before);
  });

  it('handles an empty list', () => {
    expect(disambiguateLabels([])).toEqual([]);
  });
});

describe('gpuTelemetryState', () => {
  const SOURCE = 'gpu:nvidia-1111';

  function samplesOf(
    entries: readonly { key: string; value?: number; availability?: Availability }[],
  ): Map<string, MetricSample> {
    const map = new Map<string, MetricSample>();

    for (const entry of entries) {
      const metric = { key: entry.key, sourceId: SOURCE };
      map.set(metricRefId(metric), {
        metric,
        timestamp: 1_700_000_000_000,
        value: entry.value === undefined ? null : { type: 'number', value: entry.value },
        availability: entry.availability ?? { status: 'available' },
      });
    }

    return map;
  }

  it('reports performance telemetry as present when one figure arrives', () => {
    const state = gpuTelemetryState(samplesOf([{ key: GPU_USAGE_CORE_KEY, value: 17 }]), SOURCE);

    expect(state.performanceAvailable).toBe(true);
    expect(state.reason).toBeNull();
  });

  it("carries the backend's own reason rather than inventing one", () => {
    const state = gpuTelemetryState(
      samplesOf(
        GPU_PERFORMANCE_KEYS.map((key) => ({
          key,
          availability: {
            status: 'unsupported',
            reason: 'libnvidia-ml.so.1 is not installed',
          } as Availability,
        })),
      ),
      SOURCE,
    );

    expect(state.performanceAvailable).toBe(false);
    expect(state.reason).toBe('libnvidia-ml.so.1 is not installed');
  });

  it('surfaces the reason a user can act on ahead of the others', () => {
    const state = gpuTelemetryState(
      samplesOf([
        {
          key: GPU_USAGE_CORE_KEY,
          availability: { status: 'temporarilyUnavailable', reason: 'a passing hiccup' },
        },
        {
          key: GPU_FREQUENCY_CORE_KEY,
          availability: { status: 'permissionDenied', reason: 'the driver refused this query' },
        },
      ]),
      SOURCE,
    );

    expect(state.reason).toBe('the driver refused this query');
  });

  it('keeps thermal availability separate from performance availability', () => {
    // The `nouveau` shape: no counters, a working temperature sensor. Saying
    // "GPU telemetry unavailable" here would be wrong twice over.
    const state = gpuTelemetryState(
      samplesOf([
        {
          key: GPU_USAGE_CORE_KEY,
          availability: { status: 'unsupported', reason: 'no vendor library' },
        },
        { key: GPU_TEMPERATURE_CORE_KEY, value: 46 },
      ]),
      SOURCE,
    );

    expect(state.performanceAvailable).toBe(false);
    expect(state.thermalAvailable).toBe(true);
    expect(state.reason).toBe('no vendor library');
  });

  it('reports no reason at all rather than guessing one', () => {
    const state = gpuTelemetryState(new Map(), SOURCE);

    expect(state.performanceAvailable).toBe(false);
    expect(state.thermalAvailable).toBe(false);
    expect(state.reason).toBeNull();
  });
});
