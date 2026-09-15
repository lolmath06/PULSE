/**
 * Mirror of the Rust metrics contract.
 *
 * Keep in sync with `src-tauri/src/metrics/model/` and
 * `src-tauri/src/metrics/engine/status.rs`. The Rust side pins every payload
 * shape with serialisation tests (camelCase field names, enum discriminants,
 * exact field sets), so a change there fails `cargo test` before it can reach
 * this file silently.
 *
 * See `docs/metrics/model.md`.
 */

/**
 * Version of the metrics contract this frontend expects.
 *
 * Compared against `EngineStatus.schemaVersion`. A mismatch means the backend
 * speaks a different contract — the UI warns rather than misreading payloads.
 */
export const METRICS_SCHEMA_VERSION = 1;

// --- identity -------------------------------------------------------------

/**
 * Semantic name of *what* is measured, e.g. `cpu.usage.total`.
 *
 * Lowercase, dot-separated, stable across releases. Never a translated or
 * user-visible string.
 */
export type MetricKey = string;

/**
 * Stable identity of *which component* is measured, as `kind:instance` —
 * e.g. `gpu:pci-0000-01-00-0`, `storage:nvme0n1`, `cpu:0`.
 *
 * Derived from something stable across reboots (PCI address, kernel device
 * name), never from the product name shown to the user.
 */
export type SourceId = string;

/** Identifies the provider that owns a metric, e.g. `linux.cpu`. */
export type ProviderId = string;

/**
 * The full identity of one concrete measurement.
 *
 * This pair — and only this pair — is what saved dashboards store.
 */
export interface MetricRef {
  readonly key: MetricKey;
  readonly sourceId: SourceId;
}

// --- classification -------------------------------------------------------

export type MetricCategory =
  | 'system'
  | 'cpu'
  | 'gpu'
  | 'memory'
  | 'storage'
  | 'network'
  | 'sensors'
  | 'fan'
  | 'power'
  | 'battery'
  | 'process'
  | 'other';

/**
 * Canonical units. **The backend always reports values in these units.**
 *
 * Hertz, not gigahertz. Bytes, not gibibytes. Celsius, not Fahrenheit.
 * Converting for display is the frontend's job; the wire format stays
 * canonical so history, thresholds and configuration remain comparable.
 */
export type MetricUnit =
  | 'percent'
  | 'ratio'
  | 'celsius'
  | 'hertz'
  | 'bytes'
  | 'bytesPerSecond'
  | 'watts'
  | 'volts'
  | 'rpm'
  | 'milliseconds'
  | 'seconds'
  | 'count'
  | 'none';

export type MetricValueType = 'number' | 'boolean' | 'text';

/**
 * How a metric's values behave over time.
 *
 * - `gauge` — an instantaneous reading that rises and falls; may be averaged.
 * - `counter` — a monotonically increasing total; must be differenced.
 * - `state` — a discrete condition.
 */
export type MetricKind = 'gauge' | 'counter' | 'state';

// --- values ---------------------------------------------------------------

/** A single measured value, as a discriminated union. */
export type MetricValue =
  | { readonly type: 'number'; readonly value: number }
  | { readonly type: 'boolean'; readonly value: boolean }
  | { readonly type: 'text'; readonly value: string };

// --- errors ---------------------------------------------------------------

/**
 * Machine-readable error classification.
 *
 * The frontend switches on this; it never parses `message`.
 */
export type MetricErrorCode =
  | 'unknownMetric'
  | 'duplicateMetric'
  | 'unsupported'
  | 'notDetected'
  | 'permissionDenied'
  | 'io'
  | 'parse'
  | 'timeout'
  | 'providerUnavailable'
  | 'missingSample'
  | 'internal';

export interface MetricError {
  readonly code: MetricErrorCode;
  /** Human-readable detail. For display and logs only. */
  readonly message: string;
  /** Whether retrying later might succeed. */
  readonly recoverable: boolean;
}

// --- availability ---------------------------------------------------------

/**
 * Why a metric can or cannot currently produce a value.
 *
 * The distinctions matter to the user: "your machine has no such sensor" is a
 * different message from "PULSE needs elevated rights" or "the driver
 * hiccupped". Collapsing them into a generic error is exactly what PULSE
 * refuses to do.
 */
export type Availability =
  | { readonly status: 'available' }
  /** No implementation on this platform or in this PULSE build. */
  | { readonly status: 'unsupported'; readonly reason: string }
  /** The capability exists, but this machine has no such component. */
  | { readonly status: 'notDetected'; readonly reason: string }
  /** Blocked by OS permissions — needs root or administrator. */
  | { readonly status: 'permissionDenied'; readonly reason: string }
  /** Normally available; expected to recover on its own. */
  | { readonly status: 'temporarilyUnavailable'; readonly reason: string }
  /** The provider failed while producing this metric. */
  | { readonly status: 'providerError'; readonly error: MetricError }
  /** The reference is not in the catalog — a configuration problem. */
  | { readonly status: 'notRegistered'; readonly reason: string };

export type AvailabilityStatus = Availability['status'];

// --- catalog and samples --------------------------------------------------

/** Everything PULSE knows about a metric before any value is read. */
export interface MetricDefinition {
  /** Stable identity. The only part a saved dashboard may store. */
  readonly metric: MetricRef;
  /** Human-readable component name. Presentation only, never an identifier. */
  readonly sourceLabel: string;
  readonly displayName: string;
  readonly description: string;
  readonly category: MetricCategory;
  readonly unit: MetricUnit;
  readonly valueType: MetricValueType;
  readonly kind: MetricKind;
  readonly availability: Availability;
  readonly providerId: ProviderId;
}

/** A single measurement. */
export interface MetricSample {
  readonly metric: MetricRef;
  /** Unix epoch milliseconds — usable directly as `new Date(timestamp)`. */
  readonly timestamp: number;
  /** `null` whenever `availability.status` is not `available`. */
  readonly value: MetricValue | null;
  readonly availability: Availability;
}

// --- engine status --------------------------------------------------------

/**
 * - `ready` — at least one provider is registered.
 * - `empty` — no providers. Correct and expected until PULSE ships system
 *   collectors; reported honestly rather than dressed up as ready.
 */
export type EngineState = 'ready' | 'empty';

export interface ProviderSummary {
  readonly id: ProviderId;
  readonly metricCount: number;
  readonly availableMetricCount: number;
}

export interface EngineStatus {
  readonly schemaVersion: number;
  readonly state: EngineState;
  readonly providerCount: number;
  readonly metricCount: number;
  readonly availableMetricCount: number;
  /** Per-provider breakdown, in registration order. */
  readonly providers: readonly ProviderSummary[];
}
