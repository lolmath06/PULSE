import { describe, expect, it } from 'vitest';
import type { EngineStatus } from '@/types/metrics';
import { METRICS_SCHEMA_VERSION } from '@/types/metrics';
import {
  getMetricCatalog,
  getMetricsEngineStatus,
  isSchemaCompatible,
  sampleMetrics,
} from '@/services/metrics';

function status(schemaVersion: number): EngineStatus {
  return {
    schemaVersion,
    state: 'empty',
    providerCount: 0,
    metricCount: 0,
    availableMetricCount: 0,
    providers: [],
  };
}

describe('isSchemaCompatible', () => {
  it('accepts a backend on the expected contract version', () => {
    expect(isSchemaCompatible(status(METRICS_SCHEMA_VERSION))).toBe(true);
  });

  it('rejects any other version rather than guessing', () => {
    expect(isSchemaCompatible(status(METRICS_SCHEMA_VERSION + 1))).toBe(false);
    expect(isSchemaCompatible(status(0))).toBe(false);
  });
});

describe('metrics service outside the Tauri runtime', () => {
  // Tests run in jsdom, where no backend exists. Every call must reject
  // cleanly rather than throw synchronously or hang.
  it('rejects instead of crashing', async () => {
    await expect(getMetricsEngineStatus()).rejects.toThrow(/not available/i);
    await expect(getMetricCatalog()).rejects.toThrow(/not available/i);
    await expect(sampleMetrics([{ key: 'cpu.usage.total', sourceId: 'cpu:0' }])).rejects.toThrow(
      /not available/i,
    );
  });
});
