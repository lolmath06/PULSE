import { describe, expect, it } from 'vitest';
import type { MetricDefinition } from '@/types/metrics';
import {
  CPU_FREQUENCY_CURRENT_KEY,
  CPU_FREQUENCY_MAX_KEY,
  CPU_USAGE_LOGICAL_KEY,
  CPU_USAGE_TOTAL_KEY,
  cpuLogicalOrdinal,
  cpuLogicalSourceId,
} from '@/types/wellknown';
import { discoverLogicalProcessors, logicalProcessorMetrics } from '@/utils/cpu';

function definition(key: string, sourceId: string, sourceLabel = sourceId): MetricDefinition {
  return {
    metric: { key, sourceId },
    sourceLabel,
    displayName: key,
    description: '',
    category: 'cpu',
    unit: key.startsWith('cpu.frequency') ? 'hertz' : 'percent',
    valueType: 'number',
    kind: 'gauge',
    availability: { status: 'available' },
    providerId: 'linux.cpu',
  };
}

/** A catalog for `count` logical processors, in the order the backend sends it. */
function catalogFor(count: number): MetricDefinition[] {
  const ordinals = [...Array(count).keys()];
  const entries = ordinals.flatMap((ordinal) =>
    [CPU_USAGE_LOGICAL_KEY, CPU_FREQUENCY_CURRENT_KEY, CPU_FREQUENCY_MAX_KEY].map((key) =>
      definition(key, cpuLogicalSourceId(ordinal), `CPU ${ordinal}`),
    ),
  );

  entries.push(definition(CPU_USAGE_TOTAL_KEY, 'cpu:system', 'System CPU'));
  entries.push(definition('cpu.count.logical', 'cpu:system', 'System CPU'));
  entries.push(definition('memory.used', 'memory:system', 'System memory'));

  // The backend sorts by (key, sourceId) as strings — which is exactly what
  // puts logical-10 before logical-2.
  return entries.sort((left, right) =>
    `${left.metric.key}@${left.metric.sourceId}`.localeCompare(
      `${right.metric.key}@${right.metric.sourceId}`,
    ),
  );
}

describe('cpuLogicalOrdinal', () => {
  it('round-trips a source identifier', () => {
    for (const ordinal of [0, 1, 7, 31, 64, 127]) {
      expect(cpuLogicalOrdinal(cpuLogicalSourceId(ordinal))).toBe(ordinal);
    }
  });

  it('rejects every source that is not a logical processor', () => {
    for (const sourceId of [
      'cpu:system',
      'memory:system',
      'gpu:logical-0',
      'cpu:logical-',
      'cpu:logical-abc',
      'cpu:logical-1x',
      'cpu:logical--1',
      'cpu:logical-1.5',
      'cpu:0',
      '',
    ]) {
      expect(cpuLogicalOrdinal(sourceId)).toBeNull();
    }
  });

  it('refuses a padded ordinal rather than folding it onto a real processor', () => {
    // `cpu:logical-01` is not a source the backend emits; accepting it would
    // give CPU 1 a second, duplicate row.
    expect(cpuLogicalOrdinal('cpu:logical-01')).toBeNull();
  });
});

describe('discoverLogicalProcessors', () => {
  it('sorts numerically, not lexicographically', () => {
    // The bug this exists to prevent: 1, 10, 11, 2 in the CPU table.
    const processors = discoverLogicalProcessors(catalogFor(12));

    expect(processors.map((processor) => processor.ordinal)).toEqual([
      0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    ]);
  });

  it('scales to a large machine without any hardcoded list', () => {
    for (const count of [1, 4, 8, 16, 32, 64, 128]) {
      const processors = discoverLogicalProcessors(catalogFor(count));

      expect(processors).toHaveLength(count);
      expect(processors.map((processor) => processor.ordinal)).toEqual([...Array(count).keys()]);
    }
  });

  it('keeps the ordinals in order across the two-digit boundary', () => {
    const processors = discoverLogicalProcessors(catalogFor(128));
    const ordinals = processors.map((processor) => processor.ordinal);

    expect(ordinals).toEqual([...Array(128).keys()]);
    // Explicitly: the places a string sort goes wrong.
    expect(ordinals.indexOf(2)).toBeLessThan(ordinals.indexOf(10));
    expect(ordinals.indexOf(9)).toBeLessThan(ordinals.indexOf(100));
  });

  it('ignores the machine-wide and non-CPU metrics', () => {
    const processors = discoverLogicalProcessors(catalogFor(4));

    expect(processors).toHaveLength(4);
    expect(processors.every((processor) => processor.sourceId.startsWith('cpu:logical-'))).toBe(
      true,
    );
  });

  it('lists a processor discovered through any one of its metrics', () => {
    // A processor whose frequency is unsupported still gets a row: losing its
    // usage because of a missing cpufreq directory would be the bug.
    const catalog = [
      definition(CPU_USAGE_LOGICAL_KEY, 'cpu:logical-0', 'CPU 0'),
      definition(CPU_USAGE_LOGICAL_KEY, 'cpu:logical-1', 'CPU 1'),
      definition(CPU_FREQUENCY_CURRENT_KEY, 'cpu:logical-0', 'CPU 0'),
    ];

    expect(discoverLogicalProcessors(catalog).map((p) => p.ordinal)).toEqual([0, 1]);
  });

  it('handles non-contiguous ordinals', () => {
    // A machine with CPUs 4-7 offlined.
    const catalog = [0, 1, 2, 3, 8, 9, 10, 11].map((ordinal) =>
      definition(CPU_USAGE_LOGICAL_KEY, cpuLogicalSourceId(ordinal), `CPU ${ordinal}`),
    );

    expect(discoverLogicalProcessors(catalog).map((p) => p.ordinal)).toEqual([
      0, 1, 2, 3, 8, 9, 10, 11,
    ]);
  });

  it('takes the label from the backend rather than inventing one', () => {
    const processors = discoverLogicalProcessors(catalogFor(3));

    expect(processors.map((processor) => processor.label)).toEqual(['CPU 0', 'CPU 1', 'CPU 2']);
  });

  it('lists each processor once even though it has three metrics', () => {
    const processors = discoverLogicalProcessors(catalogFor(8));
    const ordinals = processors.map((processor) => processor.ordinal);

    expect(new Set(ordinals).size).toBe(ordinals.length);
  });

  it('returns nothing for an empty or CPU-less catalog', () => {
    expect(discoverLogicalProcessors([])).toEqual([]);
    expect(discoverLogicalProcessors([definition('memory.used', 'memory:system')])).toEqual([]);
  });
});

describe('logicalProcessorMetrics', () => {
  it('requests exactly three references per processor', () => {
    const processors = discoverLogicalProcessors(catalogFor(32));
    const metrics = logicalProcessorMetrics(processors);

    expect(metrics).toHaveLength(96);
    expect(new Set(metrics.map((metric) => `${metric.key}@${metric.sourceId}`)).size).toBe(96);
  });

  it('requests only the processors it was given', () => {
    const metrics = logicalProcessorMetrics([
      { ordinal: 3, sourceId: 'cpu:logical-3', label: 'CPU 3' },
    ]);

    expect(metrics.map((metric) => metric.key).sort()).toEqual([
      CPU_FREQUENCY_CURRENT_KEY,
      CPU_FREQUENCY_MAX_KEY,
      CPU_USAGE_LOGICAL_KEY,
    ]);
    expect(metrics.every((metric) => metric.sourceId === 'cpu:logical-3')).toBe(true);
  });

  it('asks for nothing when there is nothing to show', () => {
    expect(logicalProcessorMetrics([])).toEqual([]);
  });
});
