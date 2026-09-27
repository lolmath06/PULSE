import type { MetricRef } from '@/types/metrics';

/**
 * The history contract, mirroring `src-tauri/src/history/query.rs` and
 * `service.rs`.
 *
 * Timestamps are **UTC epoch milliseconds**. They are only turned into local
 * wall-clock text at the moment they are displayed.
 */

/** A visible window. The backend picks the resolution for each. */
export type HistoryRange = '15m' | '1h' | '6h' | '24h' | '7d';

export const HISTORY_RANGES: readonly HistoryRange[] = ['15m', '1h', '6h', '24h', '7d'];

export const DEFAULT_HISTORY_RANGE: HistoryRange = '15m';

export function isHistoryRange(value: unknown): value is HistoryRange {
  return typeof value === 'string' && (HISTORY_RANGES as readonly string[]).includes(value);
}

/**
 * One point. Raw answers carry only `t` and `v`; aggregated ones add the
 * bucket's `min`, `max` and sample count `n`, and `v` is the weighted average.
 */
export interface HistoryPoint {
  readonly t: number;
  readonly v: number;
  readonly min?: number;
  readonly max?: number;
  readonly n?: number;
}

/** The last real sample inside the window. */
export interface LatestSample {
  readonly t: number;
  readonly v: number;
}

export interface HistorySeriesData {
  readonly metric: MetricRef;
  /** Ascending by `t`. A gap is an absence of points, never a zero. */
  readonly points: readonly HistoryPoint[];
  readonly latest: LatestSample | null;
  readonly skippedRows?: number;
}

export interface HistoryResponse {
  readonly range: HistoryRange;
  readonly fromMs: number;
  readonly toMs: number;
  /** Width of one point: the cadence for raw answers. */
  readonly bucketMs: number;
  readonly raw: boolean;
  /** A step between consecutive points larger than this is a gap. */
  readonly gapThresholdMs: number;
  readonly series: readonly HistorySeriesData[];
}

export type HistoryQueryResult =
  | ({ readonly status: 'ok' } & HistoryResponse)
  | { readonly status: 'unavailable'; readonly reason: string };

/** The payload of {@link HISTORY_EVENT}: which batch, never its values. */
export interface BatchRecorded {
  readonly batchId: number;
  readonly timestampMs: number;
  readonly rowCount: number;
}

/** Emitted by the backend scheduler after every batch it writes. */
export const HISTORY_EVENT = 'history-sample-recorded';

export interface HistoryTimings {
  readonly ticks: number;
  readonly sampleMedianUs: number;
  readonly sampleMaxUs: number;
  readonly insertMedianUs: number;
  readonly insertMaxUs: number;
}

export interface HistoryDatabaseStats {
  readonly path: string;
  readonly schemaVersion: number;
  readonly journalMode: string;
  readonly synchronous: number;
  readonly busyTimeoutMs: number;
  readonly fileBytes: number;
  readonly walBytes: number;
  readonly pageSize: number;
  readonly pageCount: number;
  readonly freelistCount: number;
  readonly seriesCount: number;
  readonly rawRows: number;
  readonly aggregateRows: number;
  readonly batchCount: number;
  readonly averageRowsPerBatch: number | null;
  readonly firstBatchMs: number | null;
  readonly lastBatchMs: number | null;
}

export interface HistoryStatus {
  readonly state: 'recording' | 'unavailable';
  readonly reason?: string;
  readonly databasePath: string | null;
  readonly schemaVersion: number;
  readonly cadenceMs: number;
  readonly historizedMetricCount: number;
  readonly batchesThisSession: number;
  readonly lastBatch: BatchRecorded | null;
  readonly lastError: string | null;
  readonly timings: HistoryTimings | null;
  readonly lastCompaction: {
    readonly atMs: number;
    readonly durationMs: number;
    readonly report: {
      readonly aggregatesWritten: number;
      readonly rawRowsDeleted: number;
      readonly aggregatesDeleted: number;
      readonly batchesDeleted: number;
    };
  } | null;
  readonly database: HistoryDatabaseStats | null;
}
