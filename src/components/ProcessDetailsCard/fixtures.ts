import { vi } from 'vitest';
import type { Availability } from '@/types/metrics';
import type {
  ApplicationEntry,
  Capability,
  ProcessActionResult,
  ProcessCapabilities,
  ProcessDetails,
  ProcessEntry,
  ProcessField,
  ProcessQuery,
  ProcessSnapshot,
  Provenance,
} from '@/types/processes';
import * as processService from '@/services/processes';

/**
 * Shared builders for the Phase 9 component tests.
 *
 * Every backend call is replaced by a spy, so each test can assert not only
 * what was shown but exactly which commands were — and were not — sent.
 */

export const AVAILABLE: Availability = { status: 'available' };

export function value<T>(n: T): ProcessField<T> {
  return { value: n, availability: AVAILABLE };
}

export function missing<T>(availability: Availability): ProcessField<T> {
  return { value: null, availability };
}

export function process(overrides: Partial<ProcessEntry> = {}): ProcessEntry {
  const pid = overrides.pid ?? 100;
  return {
    instanceId: `process:${pid}-7`,
    pid,
    parentPid: 1,
    name: 'bash',
    executablePath: value('/usr/bin/bash'),
    state: 'sleepingOrWaiting',
    stateAvailability: AVAILABLE,
    classification: 'userApplication',
    cpuPercent: value(0),
    residentMemoryBytes: value(4 * 1024 * 1024),
    memoryPercent: value(0.1),
    threadCount: value(1),
    readBytesPerSecond: value(0),
    writeBytesPerSecond: value(0),
    applicationKey: 'exe:/usr/bin/bash',
    ...overrides,
  };
}

export function application(overrides: Partial<ApplicationEntry> = {}): ApplicationEntry {
  return {
    key: 'exe:/usr/bin/bash',
    displayName: 'bash',
    identity: 'executable',
    classification: 'userApplication',
    processCount: 1,
    threadCount: value(1),
    cpuPercent: value(0),
    residentMemoryBytes: value(4 * 1024 * 1024),
    readBytesPerSecond: value(0),
    writeBytesPerSecond: value(0),
    ...overrides,
  };
}

export function snapshot(overrides: Partial<ProcessSnapshot> = {}): ProcessSnapshot {
  const processes = overrides.processes ?? [process()];
  return {
    takenAt: 1_700_000_000_000,
    durationMs: 12,
    counts: { total: processes.length, running: 1, threads: processes.length },
    processes,
    applications: overrides.applications ?? [application()],
    unsupportedReason: null,
    ...overrides,
  };
}

const ALLOWED: Capability = { allowed: true, reason: null };

export function capabilities(overrides: Partial<ProcessCapabilities> = {}): ProcessCapabilities {
  return {
    terminate: ALLOWED,
    terminateTree: ALLOWED,
    forceKill: ALLOWED,
    suspend: ALLOWED,
    resume: { allowed: false, reason: 'PULSE did not suspend this process.' },
    setPriority: ALLOWED,
    setAffinity: ALLOWED,
    openLocation: ALLOWED,
    computeHash: ALLOWED,
    ...overrides,
  };
}

export function details(overrides: Partial<ProcessDetails> = {}): ProcessDetails {
  const pid = overrides.pid ?? 100;
  return {
    instanceId: `process:${pid}-7`,
    pid,
    parentPid: 1,
    parentName: value('systemd'),
    name: 'bash',
    state: 'sleepingOrWaiting',
    stateAvailability: AVAILABLE,
    category: 'userApplication',
    startedAt: value(1_700_000_000_000),
    executable: value({
      path: '/usr/bin/bash',
      fileName: 'bash',
      sizeBytes: value(1_400_000),
      modifiedAt: value(1_690_000_000_000),
      replacedOnDisk: false,
    }),
    owner: value({ id: '1000', name: 'alice' }),
    architecture: value('x86_64'),
    priority: value({ kind: 'nice', value: 0 }),
    affinity: value({ cpus: [0, 1, 2, 3], available: [0, 1, 2, 3], limitation: null }),
    versionInfo: missing({ status: 'unsupported', reason: 'Linux executables carry none' }),
    capabilities: capabilities(),
    isSelf: false,
    suspendedByPulse: false,
    priorityKind: 'nice',
    forceKillSupported: true,
    ...overrides,
  };
}

export function ok(reason = 'Done.'): ProcessActionResult {
  return { status: 'success', reason, affectedCount: null, failedCount: null, tree: null };
}

export function outcome(
  status: ProcessActionResult['status'],
  reason = 'reason',
): ProcessActionResult {
  return { status, reason, affectedCount: null, failedCount: null, tree: null };
}

export function query<T>(value: T): ProcessQuery<T> {
  return { outcome: ok('ok'), value };
}

export function failedQuery<T>(result: ProcessActionResult): ProcessQuery<T> {
  return { outcome: result, value: null };
}

export const RPM: Provenance = {
  kind: 'rpmPackage',
  packages: [{ name: 'bash', version: '5.2.26-1.fc39', arch: 'x86_64' }],
};

/**
 * Spies on every process command. Snapshots are served in order and the last
 * one repeats; everything else resolves to a harmless default each test can
 * override.
 */
export function mockBackend(snapshots: readonly ProcessSnapshot[], inspected = details()) {
  const snapshotSpy = vi.spyOn(processService, 'getProcessSnapshot');
  snapshots.forEach((next) => snapshotSpy.mockResolvedValueOnce(next));
  const last = snapshots.at(-1);
  if (last !== undefined) snapshotSpy.mockResolvedValue(last);

  return {
    snapshot: snapshotSpy,
    details: vi.spyOn(processService, 'getProcessDetails').mockResolvedValue(query(inspected)),
    provenance: vi.spyOn(processService, 'getProcessProvenance').mockResolvedValue(query(RPM)),
    sha256: vi.spyOn(processService, 'computeProcessSha256').mockResolvedValue(
      query({
        status: 'computed',
        sha256: 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
        sizeBytes: 3,
        reason: null,
      }),
    ),
    suspend: vi.spyOn(processService, 'suspendProcess').mockResolvedValue(ok()),
    resume: vi.spyOn(processService, 'resumeProcess').mockResolvedValue(ok()),
    terminate: vi.spyOn(processService, 'terminateProcess').mockResolvedValue(ok()),
    terminateTree: vi.spyOn(processService, 'terminateProcessTree').mockResolvedValue(ok()),
    setPriority: vi.spyOn(processService, 'setProcessPriority').mockResolvedValue(ok()),
    setAffinity: vi.spyOn(processService, 'setProcessAffinity').mockResolvedValue(ok()),
    openLocation: vi.spyOn(processService, 'openProcessLocation').mockResolvedValue(ok()),
    webSearch: vi.spyOn(processService, 'openWebSearch').mockResolvedValue(ok()),
    hashLookup: vi.spyOn(processService, 'openHashLookup').mockResolvedValue(ok()),
  };
}

/** Every command that changes a process. */
export function destructiveCalls(backend: ReturnType<typeof mockBackend>): number {
  return [
    backend.suspend,
    backend.resume,
    backend.terminate,
    backend.terminateTree,
    backend.setPriority,
    backend.setAffinity,
  ].reduce((total, spy) => total + spy.mock.calls.length, 0);
}
