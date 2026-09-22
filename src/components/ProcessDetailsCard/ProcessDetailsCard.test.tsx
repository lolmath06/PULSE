import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Availability } from '@/types/metrics';
import type {
  ApplicationEntry,
  ProcessEntry,
  ProcessField,
  ProcessSnapshot,
} from '@/types/processes';
import { ProcessDetailsCard } from '@/components/ProcessDetailsCard/ProcessDetailsCard';
import * as processService from '@/services/processes';

const AVAILABLE: Availability = { status: 'available' };
const WAITING: Availability = {
  status: 'temporarilyUnavailable',
  reason: 'waiting for another sample',
};
const DENIED: Availability = {
  status: 'permissionDenied',
  reason: '/proc/812/io belongs to another user',
};

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
  const pid = overrides.pid ?? 1;

  return {
    instanceId: `process:${pid}-1`,
    pid,
    parentPid: 1,
    name: 'systemd',
    executablePath: { value: '/usr/lib/systemd/systemd', availability: AVAILABLE },
    state: 'sleepingOrWaiting',
    stateAvailability: AVAILABLE,
    classification: 'systemProcess',
    cpuPercent: value(0),
    residentMemoryBytes: value(1024 * 1024),
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
    key: 'exe:/usr/lib/systemd/systemd',
    displayName: 'systemd',
    identity: 'executable',
    classification: 'systemProcess',
    processCount: 1,
    threadCount: value(1),
    cpuPercent: value(0),
    residentMemoryBytes: value(1024 * 1024),
    readBytesPerSecond: value(0),
    writeBytesPerSecond: value(0),
    ...overrides,
  };
}

function snapshot(overrides: Partial<ProcessSnapshot> = {}): ProcessSnapshot {
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

function mockSnapshots(...snapshots: readonly ProcessSnapshot[]) {
  const spy = vi.spyOn(processService, 'getProcessSnapshot');
  snapshots.forEach((next) => spy.mockResolvedValueOnce(next));
  const last = snapshots.at(-1);
  if (last !== undefined) spy.mockResolvedValue(last);
  return spy;
}

async function renderCard(...snapshots: readonly ProcessSnapshot[]) {
  const spy = mockSnapshots(...snapshots);
  render(<ProcessDetailsCard />);
  await screen.findByLabelText('Applications');
  return spy;
}

beforeEach(() => {
  vi.restoreAllMocks();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('ProcessDetailsCard', () => {
  it('shows the machine-wide counts in its header', async () => {
    await renderCard(
      snapshot({
        counts: { total: 342, running: 2, threads: 1204 },
      }),
    );

    const card = screen.getByLabelText('Process details');
    expect(within(card).getByText('342')).toBeInTheDocument();
    expect(within(card).getByText('2')).toBeInTheDocument();
    // Grouped with the locale's own thousands separator, whatever it is.
    expect(within(card).getByText(/1.?204/)).toBeInTheDocument();
  });

  it('opens on the Applications view', async () => {
    await renderCard(snapshot());

    expect(screen.getByRole('button', { name: 'Applications' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByLabelText('Applications')).toBeInTheDocument();
    expect(screen.queryByLabelText('Processes')).not.toBeInTheDocument();
  });

  it('switches to the Processes view and back', async () => {
    const user = userEvent.setup();
    await renderCard(snapshot());

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    expect(screen.getByLabelText('Processes')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Applications' }));
    expect(screen.getByLabelText('Applications')).toBeInTheDocument();
  });

  it('handles a machine reporting no processes at all', async () => {
    mockSnapshots(
      snapshot({
        processes: [],
        applications: [],
        counts: { total: 0, running: 0, threads: 0 },
        unsupportedReason: 'PULSE has no process collector for this platform',
      }),
    );
    render(<ProcessDetailsCard />);

    expect(
      await screen.findByText('PULSE has no process collector for this platform'),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText('Applications')).not.toBeInTheDocument();
  });

  it('renders a single process', async () => {
    const user = userEvent.setup();
    await renderCard(snapshot({ processes: [process({ pid: 7, name: 'alone' })] }));

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    const table = screen.getByLabelText('Processes');
    expect(within(table).getByText('alone')).toBeInTheDocument();
    expect(within(table).getByText('7')).toBeInTheDocument();
  });

  it('aggregates several processes into one application row', async () => {
    await renderCard(
      snapshot({
        processes: [
          process({ pid: 10, name: 'firefox', applicationKey: 'exe:/x/firefox' }),
          process({ pid: 11, name: 'Web Content', applicationKey: 'exe:/x/firefox' }),
          process({ pid: 12, name: 'WebExtensions', applicationKey: 'exe:/x/firefox' }),
        ],
        applications: [
          application({
            key: 'exe:/x/firefox',
            displayName: 'firefox',
            processCount: 3,
            cpuPercent: value(4),
            residentMemoryBytes: value(3 * 1024 * 1024),
          }),
        ],
      }),
    );

    const table = screen.getByLabelText('Applications');
    const row = within(table).getByRole('row', { name: /firefox/ });
    expect(within(row).getByText('3')).toBeInTheDocument();
    expect(within(row).getByText('4.0 %')).toBeInTheDocument();
  });

  // --- the first sample --------------------------------------------------

  it('shows a dash and an explanation for rates before a baseline exists', async () => {
    const user = userEvent.setup();
    await renderCard(
      snapshot({
        processes: [
          process({
            pid: 5,
            name: 'new',
            cpuPercent: waiting(),
            readBytesPerSecond: waiting(),
            writeBytesPerSecond: waiting(),
          }),
        ],
      }),
    );

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    const row = screen.getByRole('row', { name: /new/ });
    const dashes = within(row).getAllByText('—');

    expect(dashes).toHaveLength(3);
    for (const dash of dashes) {
      expect(dash).toHaveAttribute('title', 'Temporarily unavailable: waiting for another sample');
    }
  });

  it('shows memory and thread counts on the very first snapshot', async () => {
    const user = userEvent.setup();
    await renderCard(
      snapshot({
        processes: [
          process({
            pid: 5,
            name: 'new',
            cpuPercent: waiting(),
            residentMemoryBytes: value(512 * 1024 * 1024),
            threadCount: value(112),
          }),
        ],
      }),
    );

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    const row = screen.getByRole('row', { name: /new/ });
    expect(within(row).getByText('512.0 MiB')).toBeInTheDocument();
    expect(within(row).getByText('112')).toBeInTheDocument();
  });

  it('shows real rates on the second snapshot', async () => {
    const user = userEvent.setup();
    const first = snapshot({
      processes: [process({ pid: 5, name: 'busy', cpuPercent: waiting() })],
      applications: [application({ key: 'exe:/x', displayName: 'busy', cpuPercent: waiting() })],
    });
    const second = snapshot({
      processes: [process({ pid: 5, name: 'busy', cpuPercent: value(3.125) })],
      applications: [application({ key: 'exe:/x', displayName: 'busy', cpuPercent: value(3.125) })],
    });

    await renderCard(first, second);
    await user.click(screen.getByRole('button', { name: 'Refresh process details' }));

    await waitFor(() => {
      expect(screen.getByText('3.1 %')).toBeInTheDocument();
    });
  });

  it('shows a measured zero as zero, never as a dash', async () => {
    const user = userEvent.setup();
    await renderCard(
      snapshot({
        processes: [
          process({
            pid: 5,
            name: 'idle',
            cpuPercent: value(0),
            readBytesPerSecond: value(0),
            writeBytesPerSecond: value(0),
          }),
        ],
      }),
    );

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    const row = screen.getByRole('row', { name: /idle/ });

    expect(within(row).queryByText('—')).not.toBeInTheDocument();
    expect(within(row).getByText('0.0 %')).toBeInTheDocument();
    expect(within(row).getAllByText('0 B/s')).toHaveLength(2);
  });

  it('keeps a partially refused process visible with a reason on the refused cells', async () => {
    const user = userEvent.setup();
    await renderCard(
      snapshot({
        processes: [
          process({
            pid: 812,
            name: 'guarded',
            readBytesPerSecond: denied(),
            writeBytesPerSecond: denied(),
            executablePath: { value: null, availability: DENIED },
          }),
        ],
      }),
    );

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    const row = screen.getByRole('row', { name: /guarded/ });

    expect(within(row).getByText('812')).toBeInTheDocument();
    expect(within(row).getByText('1.0 MiB')).toBeInTheDocument();
    for (const dash of within(row).getAllByText('—')) {
      expect(dash).toHaveAttribute(
        'title',
        'Permission required: /proc/812/io belongs to another user',
      );
    }
  });

  // --- sorting -----------------------------------------------------------

  it.each([
    ['CPU', 'cpuPercent', ['heavy', 'light']],
    ['Memory', 'residentMemoryBytes', ['heavy', 'light']],
    ['Read', 'readBytesPerSecond', ['heavy', 'light']],
    ['Write', 'writeBytesPerSecond', ['heavy', 'light']],
  ] as const)('sorts the process table by %s', async (label, field, expected) => {
    const user = userEvent.setup();
    await renderCard(
      snapshot({
        processes: [
          process({ pid: 1, name: 'light', [field]: value(1) }),
          process({ pid: 2, name: 'heavy', [field]: value(900) }),
        ],
      }),
    );

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    await user.click(
      within(screen.getByLabelText('Processes')).getByRole('button', { name: label }),
    );

    const rows = within(screen.getByLabelText('Processes')).getAllByRole('row').slice(1);
    expect(rows.map((row) => within(row).getAllByRole('rowheader')[0]?.textContent)).toEqual(
      expected,
    );
  });

  it('sorts by name when the Process column is chosen', async () => {
    const user = userEvent.setup();
    await renderCard(
      snapshot({
        processes: [
          process({ pid: 1, name: 'zsh', cpuPercent: value(90) }),
          process({ pid: 2, name: 'awk', cpuPercent: value(0) }),
        ],
      }),
    );

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    await user.click(
      within(screen.getByLabelText('Processes')).getByRole('button', { name: 'Process' }),
    );

    const rows = within(screen.getByLabelText('Processes')).getAllByRole('row').slice(1);
    expect(rows.map((row) => within(row).getAllByRole('rowheader')[0]?.textContent)).toEqual([
      'awk',
      'zsh',
    ]);
  });

  it('sorts the application table too', async () => {
    const user = userEvent.setup();
    await renderCard(
      snapshot({
        applications: [
          application({ key: 'exe:/a', displayName: 'alpha', residentMemoryBytes: value(10) }),
          application({ key: 'exe:/b', displayName: 'beta', residentMemoryBytes: value(9000) }),
        ],
      }),
    );

    await user.click(
      within(screen.getByLabelText('Applications')).getByRole('button', { name: 'Memory' }),
    );

    const rows = within(screen.getByLabelText('Applications')).getAllByRole('row').slice(1);
    expect(rows.map((row) => within(row).getAllByRole('rowheader')[0]?.textContent)).toEqual([
      'beta',
      'alpha',
    ]);
  });

  // --- search ------------------------------------------------------------

  it('filters by process name', async () => {
    const user = userEvent.setup();
    await renderCard(
      snapshot({
        processes: [process({ pid: 1, name: 'firefox' }), process({ pid: 2, name: 'sshd' })],
      }),
    );

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    await user.type(screen.getByLabelText('Search processes'), 'fire');

    const table = screen.getByLabelText('Processes');
    expect(within(table).getByText('firefox')).toBeInTheDocument();
    expect(within(table).queryByText('sshd')).not.toBeInTheDocument();
  });

  it('filters by PID', async () => {
    const user = userEvent.setup();
    await renderCard(
      snapshot({
        processes: [process({ pid: 4242, name: 'firefox' }), process({ pid: 7, name: 'sshd' })],
      }),
    );

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    await user.type(screen.getByLabelText('Search processes'), '4242');

    const table = screen.getByLabelText('Processes');
    expect(within(table).getByText('firefox')).toBeInTheDocument();
    expect(within(table).queryByText('sshd')).not.toBeInTheDocument();
  });

  it('says so when a search matches nothing', async () => {
    const user = userEvent.setup();
    await renderCard(snapshot());

    await user.type(screen.getByLabelText('Search processes'), 'nothing-matches-this');

    expect(screen.getByText('No application matches this search.')).toBeInTheDocument();
  });

  // --- Show all ----------------------------------------------------------

  it('shows twenty processes and then all of them on request', async () => {
    const user = userEvent.setup();
    const many = Array.from({ length: 57 }, (_, index) =>
      process({ pid: index + 1, name: `worker${String(index).padStart(2, '0')}` }),
    );
    await renderCard(snapshot({ processes: many }));

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    expect(within(screen.getByLabelText('Processes')).getAllByRole('row')).toHaveLength(21);

    await user.click(screen.getByRole('button', { name: 'Show all 57 processes' }));
    expect(within(screen.getByLabelText('Processes')).getAllByRole('row')).toHaveLength(58);
    expect(screen.queryByRole('button', { name: /Show all/ })).not.toBeInTheDocument();
  });

  it('offers no Show all when everything already fits', async () => {
    const user = userEvent.setup();
    await renderCard(snapshot());

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    expect(screen.queryByRole('button', { name: /Show all/ })).not.toBeInTheDocument();
  });

  // --- long names and the timestamp --------------------------------------

  it('keeps a very long process name on one row rather than breaking the table', async () => {
    const user = userEvent.setup();
    const long = 'a-process-with-an-extremely-long-name-'.repeat(6);
    await renderCard(snapshot({ processes: [process({ pid: 9, name: long })] }));

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    const label = screen.getByText(long);

    // The truncation is CSS, and the class carrying it is what this asserts:
    // the text is never cut in the DOM, so search and accessibility still see
    // the whole name.
    expect(label).toHaveClass('process-table__label');
    expect(label.textContent).toBe(long);
  });

  it('shows when the snapshot was taken and how long it took', async () => {
    await renderCard(snapshot({ durationMs: 31 }));

    expect(screen.getByText(/collected in 31 ms/)).toBeInTheDocument();
  });

  it('shows the executable path only as a tooltip, never as a column', async () => {
    const user = userEvent.setup();
    await renderCard(
      snapshot({
        processes: [
          process({
            pid: 9,
            name: 'firefox',
            executablePath: { value: '/usr/lib64/firefox/firefox', availability: AVAILABLE },
          }),
        ],
      }),
    );

    await user.click(screen.getByRole('button', { name: 'Processes' }));
    const table = screen.getByLabelText('Processes');

    expect(within(table).getByText('firefox')).toHaveAttribute(
      'title',
      '/usr/lib64/firefox/firefox',
    );
    expect(within(table).queryByText('/usr/lib64/firefox/firefox')).not.toBeInTheDocument();
  });

  // --- refresh -----------------------------------------------------------

  it('re-walks the process table on Refresh', async () => {
    const user = userEvent.setup();
    const spy = await renderCard(
      snapshot({ counts: { total: 300, running: 1, threads: 900 } }),
      snapshot({ counts: { total: 301, running: 2, threads: 905 } }),
    );

    expect(spy).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole('button', { name: 'Refresh process details' }));

    await waitFor(() => {
      expect(screen.getByText('301')).toBeInTheDocument();
    });
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it('never polls', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const spy = mockSnapshots(snapshot());
    render(<ProcessDetailsCard />);
    await screen.findByLabelText('Applications');

    expect(spy).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(120_000);
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('explains an unreachable backend instead of rendering an empty table', async () => {
    vi.spyOn(processService, 'getProcessSnapshot').mockRejectedValue(
      new Error('PULSE backend is not available'),
    );
    render(<ProcessDetailsCard />);

    expect(await screen.findByText(/Backend unavailable/)).toBeInTheDocument();
    expect(screen.queryByLabelText('Applications')).not.toBeInTheDocument();
  });
});
