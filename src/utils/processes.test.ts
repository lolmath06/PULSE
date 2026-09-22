import { describe, expect, it } from 'vitest';
import type { Availability } from '@/types/metrics';
import type { ApplicationEntry, ProcessEntry, ProcessField } from '@/types/processes';
import {
  DEFAULT_SORT,
  TOP_PROCESS_COUNT,
  applicationMatches,
  applicationNames,
  fieldValue,
  formatProcessState,
  isWaitingForBaseline,
  processMatches,
  sortApplications,
  sortProcesses,
} from '@/utils/processes';

const AVAILABLE: Availability = { status: 'available' };
const WAITING: Availability = {
  status: 'temporarilyUnavailable',
  reason: 'waiting for another sample',
};
const DENIED: Availability = { status: 'permissionDenied', reason: '/proc/7/io is not readable' };

function value(n: number): ProcessField<number> {
  return { value: n, availability: AVAILABLE };
}

function waiting(): ProcessField<number> {
  return { value: null, availability: WAITING };
}

function denied(): ProcessField<number> {
  return { value: null, availability: DENIED };
}

function process(overrides: Partial<ProcessEntry> = {}): ProcessEntry {
  return {
    instanceId: 'process:1-1',
    pid: 1,
    parentPid: null,
    name: 'systemd',
    executablePath: { value: '/usr/lib/systemd/systemd', availability: AVAILABLE },
    state: 'sleepingOrWaiting',
    stateAvailability: AVAILABLE,
    classification: 'systemProcess',
    cpuPercent: value(0),
    residentMemoryBytes: value(1024),
    memoryPercent: value(0.1),
    threadCount: value(1),
    readBytesPerSecond: value(0),
    writeBytesPerSecond: value(0),
    applicationKey: 'exe:/usr/lib/systemd/systemd',
    ...overrides,
  };
}

function application(overrides: Partial<ApplicationEntry> = {}): ApplicationEntry {
  return {
    key: 'exe:/usr/bin/app',
    displayName: 'app',
    identity: 'executable',
    classification: 'userApplication',
    processCount: 1,
    threadCount: value(4),
    cpuPercent: value(1),
    residentMemoryBytes: value(2048),
    readBytesPerSecond: value(0),
    writeBytesPerSecond: value(0),
    ...overrides,
  };
}

describe('sortProcesses', () => {
  it('opens on CPU, busiest first', () => {
    expect(DEFAULT_SORT).toBe('cpu');

    const sorted = sortProcesses(
      [
        process({ pid: 1, name: 'idle', cpuPercent: value(0) }),
        process({ pid: 2, name: 'busy', cpuPercent: value(9.5) }),
        process({ pid: 3, name: 'some', cpuPercent: value(1.25) }),
      ],
      'cpu',
    );

    expect(sorted.map((entry) => entry.name)).toEqual(['busy', 'some', 'idle']);
  });

  it.each([
    ['memory', 'residentMemoryBytes'],
    ['read', 'readBytesPerSecond'],
    ['write', 'writeBytesPerSecond'],
  ] as const)('sorts by %s, largest first', (column, field) => {
    const sorted = sortProcesses(
      [
        process({ pid: 1, name: 'small', [field]: value(10) }),
        process({ pid: 2, name: 'large', [field]: value(1000) }),
        process({ pid: 3, name: 'medium', [field]: value(500) }),
      ],
      column,
    );

    expect(sorted.map((entry) => entry.name)).toEqual(['large', 'medium', 'small']);
  });

  it('sorts by name alphabetically', () => {
    const sorted = sortProcesses(
      [
        process({ pid: 1, name: 'zsh', cpuPercent: value(90) }),
        process({ pid: 2, name: 'awk', cpuPercent: value(0) }),
      ],
      'name',
    );

    expect(sorted.map((entry) => entry.name)).toEqual(['awk', 'zsh']);
  });

  it('breaks ties on name and then PID, so the table does not tremble', () => {
    // Hundreds of processes sit at exactly 0 %. Without this, every Refresh
    // would reshuffle them.
    const rows = [
      process({ pid: 900, name: 'beta', cpuPercent: value(0) }),
      process({ pid: 100, name: 'alpha', cpuPercent: value(0) }),
      process({ pid: 50, name: 'beta', cpuPercent: value(0) }),
    ];

    const first = sortProcesses(rows, 'cpu');
    const second = sortProcesses([...rows].reverse(), 'cpu');

    expect(first.map((entry) => entry.pid)).toEqual([100, 50, 900]);
    expect(second.map((entry) => entry.pid)).toEqual(first.map((entry) => entry.pid));
  });

  it('sorts unmeasured values last rather than treating them as zero', () => {
    // A process still waiting for a baseline is not "the least busy process
    // on the machine" — it is one PULSE has not measured yet.
    const sorted = sortProcesses(
      [
        process({ pid: 1, name: 'unknown', cpuPercent: waiting() }),
        process({ pid: 2, name: 'idle', cpuPercent: value(0) }),
        process({ pid: 3, name: 'refused', cpuPercent: denied() }),
      ],
      'cpu',
    );

    expect(sorted[0]?.name).toBe('idle');
    expect(sorted.slice(1).map((entry) => entry.name)).toEqual(['refused', 'unknown']);
  });

  it('never mutates its input', () => {
    const rows = [
      process({ pid: 1, name: 'a', cpuPercent: value(1) }),
      process({ pid: 2, name: 'b', cpuPercent: value(9) }),
    ];
    const before = rows.map((entry) => entry.pid);

    sortProcesses(rows, 'cpu');

    expect(rows.map((entry) => entry.pid)).toEqual(before);
  });

  it('handles zero and one process', () => {
    expect(sortProcesses([], 'cpu')).toEqual([]);
    expect(sortProcesses([process()], 'cpu')).toHaveLength(1);
  });

  it('handles several hundred processes', () => {
    const many = Array.from({ length: 600 }, (_, index) =>
      process({
        pid: index + 1,
        instanceId: `process:${index + 1}-1`,
        name: `worker${index}`,
        cpuPercent: value(index % 7),
      }),
    );

    const sorted = sortProcesses(many, 'cpu');

    expect(sorted).toHaveLength(600);
    expect(sorted[0]?.cpuPercent.value).toBe(6);
  });
});

describe('sortApplications', () => {
  it('sorts by CPU and breaks ties on name then key', () => {
    const sorted = sortApplications(
      [
        application({ key: 'exe:/b', displayName: 'zeta', cpuPercent: value(1) }),
        application({ key: 'exe:/a', displayName: 'alpha', cpuPercent: value(1) }),
        application({ key: 'exe:/c', displayName: 'busy', cpuPercent: value(9) }),
      ],
      'cpu',
    );

    expect(sorted.map((entry) => entry.displayName)).toEqual(['busy', 'alpha', 'zeta']);
  });

  it('sorts by memory, read, write and name', () => {
    const rows = [
      application({
        key: 'exe:/a',
        displayName: 'a',
        residentMemoryBytes: value(1),
        readBytesPerSecond: value(9),
        writeBytesPerSecond: value(1),
      }),
      application({
        key: 'exe:/b',
        displayName: 'b',
        residentMemoryBytes: value(9),
        readBytesPerSecond: value(1),
        writeBytesPerSecond: value(9),
      }),
    ];

    expect(sortApplications(rows, 'memory')[0]?.displayName).toBe('b');
    expect(sortApplications(rows, 'read')[0]?.displayName).toBe('a');
    expect(sortApplications(rows, 'write')[0]?.displayName).toBe('b');
    expect(sortApplications(rows, 'name')[0]?.displayName).toBe('a');
  });
});

describe('search', () => {
  const firefox = process({ pid: 4242, name: 'Web Content', applicationKey: 'exe:/x/firefox' });

  it('matches a process name, case-insensitively', () => {
    expect(processMatches(firefox, 'firefox', 'web')).toBe(true);
    expect(processMatches(firefox, 'firefox', 'CONTENT')).toBe(true);
    expect(processMatches(firefox, 'firefox', 'chromium')).toBe(false);
  });

  it('matches a PID', () => {
    expect(processMatches(firefox, 'firefox', '4242')).toBe(true);
    expect(processMatches(firefox, 'firefox', '42')).toBe(true);
    expect(processMatches(firefox, 'firefox', '9999')).toBe(false);
  });

  it('matches the application a process was grouped into', () => {
    expect(processMatches(firefox, 'firefox', 'firefox')).toBe(true);
    expect(processMatches(firefox, undefined, 'firefox')).toBe(false);
  });

  it('treats an empty or whitespace query as no filter', () => {
    expect(processMatches(firefox, undefined, '')).toBe(true);
    expect(processMatches(firefox, undefined, '   ')).toBe(true);
    expect(applicationMatches(application(), '')).toBe(true);
  });

  it('matches application names', () => {
    expect(applicationMatches(application({ displayName: 'firefox' }), 'FIRE')).toBe(true);
    expect(applicationMatches(application({ displayName: 'firefox' }), 'code')).toBe(false);
  });
});

describe('presentation helpers', () => {
  it('names the states PULSE commits to and dashes the one it does not', () => {
    expect(formatProcessState('running')).toBe('Running');
    expect(formatProcessState('sleepingOrWaiting')).toBe('Sleeping');
    expect(formatProcessState('stopped')).toBe('Stopped');
    expect(formatProcessState('zombie')).toBe('Zombie');
    expect(formatProcessState('other')).toBe('—');
  });

  it('maps application keys to display names', () => {
    const names = applicationNames([
      application({ key: 'exe:/a', displayName: 'alpha' }),
      application({ key: 'name:b', displayName: 'beta' }),
    ]);

    expect(names.get('exe:/a')).toBe('alpha');
    expect(names.get('name:b')).toBe('beta');
    expect(names.get('missing')).toBeUndefined();
  });

  it('exposes a field value without inventing one', () => {
    expect(fieldValue(value(0))).toBe(0);
    expect(fieldValue(waiting())).toBeNull();
  });

  it('shows twenty rows before Show all', () => {
    expect(TOP_PROCESS_COUNT).toBe(20);
  });
});

describe('isWaitingForBaseline', () => {
  it('is true on the first snapshot and false once rates arrive', () => {
    expect(isWaitingForBaseline([process({ cpuPercent: waiting() })])).toBe(true);
    expect(isWaitingForBaseline([process({ cpuPercent: value(0) })])).toBe(false);
  });

  it('distinguishes waiting from refused', () => {
    // Every reading refused is a different message from no baseline yet.
    expect(isWaitingForBaseline([process({ cpuPercent: denied() })])).toBe(false);
  });

  it('is false with no processes at all', () => {
    expect(isWaitingForBaseline([])).toBe(false);
  });
});
