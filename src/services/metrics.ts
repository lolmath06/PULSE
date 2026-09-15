import type { EngineStatus, MetricDefinition, MetricRef, MetricSample } from '@/types/metrics';
import { METRICS_SCHEMA_VERSION } from '@/types/metrics';
import { invokeCommand } from '@/services/tauri';

/**
 * The metrics boundary.
 *
 * Every metrics call crosses here and nowhere else — components and hooks use
 * this module, never `invoke` directly. That keeps the IPC surface enumerable
 * and mockable, and it is what lets the UI stay ignorant of whether a number
 * came from `/proc`, WMI, NVML or SMART.
 */

/** Reports schema version, provider count and metric counts. */
export function getMetricsEngineStatus(): Promise<EngineStatus> {
  return invokeCommand<EngineStatus>('get_metrics_engine_status');
}

/**
 * Returns the metadata of every registered metric, in deterministic order.
 *
 * Empty until PULSE ships real system providers.
 */
export function getMetricCatalog(): Promise<MetricDefinition[]> {
  return invokeCommand<MetricDefinition[]>('get_metric_catalog');
}

/**
 * Samples the requested metrics.
 *
 * Returns one sample per requested reference, in the same order. Unknown
 * references and failing providers come back as unavailable samples rather
 * than failing the whole call.
 */
export function sampleMetrics(metrics: readonly MetricRef[]): Promise<MetricSample[]> {
  return invokeCommand<MetricSample[]>('sample_metrics', { metrics });
}

/**
 * Whether the backend speaks the contract version this build expects.
 *
 * A mismatch means payload shapes may have changed incompatibly; the UI should
 * say so rather than silently misread data.
 */
export function isSchemaCompatible(status: EngineStatus): boolean {
  return status.schemaVersion === METRICS_SCHEMA_VERSION;
}
