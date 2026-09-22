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

/**
 * What kind of thing a process is. `unknown` where no reliable signal exists.
 *
 * `kernelThread` is set only from the kernel's own `PF_KTHREAD` flag on
 * Linux — never guessed from a name.
 */
export type ProcessClassification =
  'userApplication' | 'systemProcess' | 'kernelThread' | 'unknown';

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

// --- Phase 9: inspector and controls -----------------------------------------
//
// Mirrors `src-tauri/src/processes/{action,control,inspector,hash}.rs`.

/** What happened to one requested action. Distinct because the next step is. */
export type ProcessActionStatus =
  | 'success'
  | 'permissionDenied'
  | 'staleProcess'
  | 'processGone'
  | 'unsupported'
  | 'partialFailure'
  | 'invalidRequest'
  | 'platformError';

/** Per-target tally of an *End process tree*. Buckets sum to `requested`. */
export interface TreeTerminationSummary {
  readonly requested: number;
  readonly terminated: number;
  readonly alreadyGone: number;
  readonly permissionDenied: number;
  readonly staleSkipped: number;
  readonly skippedSelf: number;
  readonly failed: number;
}

export interface ProcessActionResult {
  readonly status: ProcessActionStatus;
  readonly reason: string;
  readonly affectedCount: number | null;
  readonly failedCount: number | null;
  readonly tree: TreeTerminationSummary | null;
}

/** A read-only query's answer: a value when the identity held, else a reason. */
export interface ProcessQuery<T> {
  readonly outcome: ProcessActionResult;
  readonly value: T | null;
}

export type WindowsPriorityClass =
  'idle' | 'belowNormal' | 'normal' | 'aboveNormal' | 'high' | 'realtime';

/** A priority in the platform's own terms — never normalised onto one scale. */
export type ProcessPriority =
  | { readonly kind: 'nice'; readonly value: number }
  | { readonly kind: 'windowsClass'; readonly class: WindowsPriorityClass };

export interface ProcessAffinity {
  readonly cpus: readonly number[];
  readonly available: readonly number[];
  readonly limitation: string | null;
}

export interface Capability {
  readonly allowed: boolean;
  readonly reason: string | null;
}

export interface ProcessCapabilities {
  readonly terminate: Capability;
  readonly terminateTree: Capability;
  readonly forceKill: Capability;
  readonly suspend: Capability;
  readonly resume: Capability;
  readonly setPriority: Capability;
  readonly setAffinity: Capability;
  readonly openLocation: Capability;
  readonly computeHash: Capability;
}

export interface ExecutableInfo {
  readonly path: string;
  readonly fileName: string;
  readonly sizeBytes: ProcessField<number>;
  readonly modifiedAt: ProcessField<number>;
  /** Linux: the file was deleted or replaced on disk after the process started. */
  readonly replacedOnDisk: boolean;
}

export interface ProcessOwner {
  /** Linux UID or Windows SID. */
  readonly id: string;
  readonly name: string | null;
}

/** Windows version resource. Self-declared: description, never evidence. */
export interface VersionInfo {
  readonly fileDescription: string | null;
  readonly productName: string | null;
  readonly companyName: string | null;
  readonly fileVersion: string | null;
  readonly productVersion: string | null;
}

export interface ProcessDetails {
  readonly instanceId: string;
  readonly pid: number;
  readonly parentPid: number | null;
  readonly parentName: ProcessField<string>;
  readonly name: string;
  readonly state: ProcessState;
  readonly stateAvailability: Availability;
  readonly category: ProcessClassification;
  /** Wall-clock start, epoch ms. The identity token lives in `instanceId`. */
  readonly startedAt: ProcessField<number>;
  readonly executable: ProcessField<ExecutableInfo>;
  readonly owner: ProcessField<ProcessOwner>;
  readonly architecture: ProcessField<string>;
  readonly priority: ProcessField<ProcessPriority>;
  readonly affinity: ProcessField<ProcessAffinity>;
  readonly versionInfo: ProcessField<VersionInfo>;
  readonly capabilities: ProcessCapabilities;
  readonly isSelf: boolean;
  readonly suspendedByPulse: boolean;
  readonly priorityKind: 'nice' | 'windowsClass' | null;
  readonly forceKillSupported: boolean;
}

export interface PackageRef {
  readonly name: string;
  readonly version: string;
  readonly arch: string;
}

export type TrustStatus =
  'trusted' | 'signedButUntrusted' | 'invalid' | 'unsigned' | 'unavailable' | 'permissionDenied';

export interface SignatureInfo {
  readonly trust: TrustStatus;
  readonly source: 'embedded' | 'catalog' | null;
  readonly publisher: string | null;
  readonly detail: string;
}

/** Where an executable comes from. Evidence, never a verdict. */
export type Provenance =
  | { readonly kind: 'rpmPackage'; readonly packages: readonly PackageRef[] }
  | { readonly kind: 'notPackaged'; readonly location: 'userHome' | 'localInstall' | 'other' }
  | { readonly kind: 'signature'; readonly signature: SignatureInfo }
  | { readonly kind: 'unavailable'; readonly reason: string };

export type HashStatus =
  'computed' | 'changedWhileHashing' | 'permissionDenied' | 'notFound' | 'notAFile' | 'readError';

export interface FileHash {
  readonly status: HashStatus;
  readonly sha256: string | null;
  readonly sizeBytes: number | null;
  readonly reason: string | null;
}
