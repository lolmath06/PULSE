/**
 * Mirror of the Rust process snapshot contract.
 *
 * Keep in sync with `src-tauri/src/processes/`. The Rust side pins every
 * payload shape with serialisation tests, so a change there fails `cargo test`
 * before it can reach this file silently.
 *
 * # Why processes are not metrics
 *
 * Everything else in PULSE's interface is built from `MetricDefinition` and
 * `MetricSample`, discovered from the catalog. Processes deliberately are not:
 * a machine runs several hundred of them, most for less than a second, and a
 * catalog entry is a promise that a reference still resolves in six months.
 * The three machine-wide *counts* go through the engine like everything else;
 * the rows come through `get_process_snapshot`.
 *
 * See `docs/metrics/processes.md`.
 */

import type { Availability } from '@/types/metrics';

/**
 * One datum with its own availability.
 *
 * Availability is per field, never per row: a process can be visible, named
 * and counted while its I/O counters are refused. The row stays; the two cells
 * say why they are empty.
 */
export interface ProcessField<T> {
  /** `null` whenever `availability.status` is not `available`. */
  readonly value: T | null;
  readonly availability: Availability;
}

/**
 * What a process is doing, reduced to what both platforms can honestly mean.
 *
 * `other` includes *every* process on Windows, which reports no process-level
 * state at all.
 */
export type ProcessState = 'running' | 'sleepingOrWaiting' | 'stopped' | 'zombie' | 'other';

/** What kind of thing a process is. `unknown` where no reliable signal exists. */
export type ProcessClassification = 'userApplication' | 'systemProcess' | 'unknown';

/** How confident an application grouping is. */
export type ApplicationIdentity = 'executable' | 'name';

/** One process, as the table renders it. */
export interface ProcessEntry {
  /** `process:<pid>-<start token>`, unique to this incarnation of this PID. */
  readonly instanceId: string;
  readonly pid: number;
  readonly parentPid: number | null;
  /**
   * The short name the operating system knows this process by.
   *
   * **Never a command line.** Arguments carry paths, URLs and secrets; PULSE
   * does not collect them.
   */
  readonly name: string;
  /** For disambiguating same-named programs. Shown only as a tooltip. */
  readonly executablePath: ProcessField<string>;
  readonly state: ProcessState;
  /** Why the state is unknown, where the platform reports none. */
  readonly stateAvailability: Availability;
  readonly classification: ProcessClassification;
  /** Share of the **whole machine's** CPU capacity, 0–100. */
  readonly cpuPercent: ProcessField<number>;
  /** Linux resident set / Windows working set, in bytes. */
  readonly residentMemoryBytes: ProcessField<number>;
  readonly memoryPercent: ProcessField<number>;
  readonly threadCount: ProcessField<number>;
  readonly readBytesPerSecond: ProcessField<number>;
  readonly writeBytesPerSecond: ProcessField<number>;
  /** Which application this process was grouped into. */
  readonly applicationKey: string;
}

/** Several processes of one program, summed. */
export interface ApplicationEntry {
  readonly key: string;
  readonly displayName: string;
  readonly identity: ApplicationIdentity;
  readonly classification: ProcessClassification;
  readonly processCount: number;
  readonly threadCount: ProcessField<number>;
  readonly cpuPercent: ProcessField<number>;
  readonly residentMemoryBytes: ProcessField<number>;
  readonly readBytesPerSecond: ProcessField<number>;
  readonly writeBytesPerSecond: ProcessField<number>;
}

export interface ProcessCounts {
  readonly total: number;
  readonly running: number;
  readonly threads: number;
}

/** One pass over the process table. */
export interface ProcessSnapshot {
  /** Unix epoch milliseconds — usable directly as `new Date(takenAt)`. */
  readonly takenAt: number;
  /** How long the backend's collection took. */
  readonly durationMs: number;
  readonly counts: ProcessCounts;
  readonly processes: readonly ProcessEntry[];
  readonly applications: readonly ApplicationEntry[];
  /** Set when the platform has no process collector at all. */
  readonly unsupportedReason: string | null;
}
