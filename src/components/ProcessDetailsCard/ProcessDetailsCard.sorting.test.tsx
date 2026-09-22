import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { UserEvent } from '@testing-library/user-event';
import { ProcessDetailsCard } from '@/components/ProcessDetailsCard/ProcessDetailsCard';
import type { ApplicationEntry, ProcessEntry } from '@/types/processes';
import { application, mockBackend, process, snapshot, value } from './fixtures';

/**
 * The Phase 8 bug, made impossible to reintroduce.
 *
 * Phase 8's headers looked sortable but clicking CPU twice never reversed the
 * order. These tests click real headers in a rendered table and read the rows
 * back, three clicks per column, in both tables — they do not test a
 * comparator in isolation.
 */

type Field = 'cpuPercent' | 'residentMemoryBytes' | 'readBytesPerSecond' | 'writeBytesPerSecond';

function processRows(field: Field): ProcessEntry[] {
  return [
    process({ pid: 1, instanceId: 'process:1-1', name: 'two', [field]: value(2) }),
    process({ pid: 2, instanceId: 'process:2-1', name: 'ten', [field]: value(10) }),
    process({ pid: 3, instanceId: 'process:3-1', name: 'zero', [field]: value(0) }),
  ];
}

function applicationRows(field: Exclude<Field, never>): ApplicationEntry[] {
  return [
    application({ key: 'exe:/two', displayName: 'two', [field]: value(2) }),
    application({ key: 'exe:/ten', displayName: 'ten', [field]: value(10) }),
    application({ key: 'exe:/zero', displayName: 'zero', [field]: value(0) }),
  ];
}

function order(table: 'Processes' | 'Applications'): string[] {
  const rows = within(screen.getByRole('table', { name: table }))
    .getAllByRole('row')
    .slice(1);
  return rows.map((row) => within(row).getAllByRole('rowheader')[0]?.textContent ?? '');
}

function header(table: 'Processes' | 'Applications', label: string) {
  return within(screen.getByRole('table', { name: table })).getByRole('button', {
    name: new RegExp(`^${label}`),
  });
}

function columnHeader(table: 'Processes' | 'Applications', label: string) {
  return header(table, label).closest('th') as HTMLElement;
}

async function showProcesses(user: UserEvent) {
  await user.click(
    within(screen.getByRole('group', { name: 'Process view' })).getByRole('button', {
      name: 'Processes',
    }),
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
});

describe.each([
  ['CPU', 'cpuPercent'],
  ['Memory', 'residentMemoryBytes'],
  ['Read', 'readBytesPerSecond'],
  ['Write', 'writeBytesPerSecond'],
] as const)('%s column', (label, field) => {
  it('toggles 10/2/0 → 0/2/10 → 10/2/0 in the Processes table', async () => {
    const user = userEvent.setup();
    mockBackend([snapshot({ processes: processRows(field) })]);
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });
    await showProcesses(user);

    // Start from another column, so the first click selects this one.
    await user.click(header('Processes', 'Process'));

    await user.click(header('Processes', label));
    expect(order('Processes')).toEqual(['ten', 'two', 'zero']);
    expect(columnHeader('Processes', label)).toHaveAttribute('aria-sort', 'descending');
    expect(header('Processes', label)).toHaveTextContent('↓');

    await user.click(header('Processes', label));
    expect(order('Processes')).toEqual(['zero', 'two', 'ten']);
    expect(columnHeader('Processes', label)).toHaveAttribute('aria-sort', 'ascending');
    expect(header('Processes', label)).toHaveTextContent('↑');

    await user.click(header('Processes', label));
    expect(order('Processes')).toEqual(['ten', 'two', 'zero']);
    expect(columnHeader('Processes', label)).toHaveAttribute('aria-sort', 'descending');
    expect(header('Processes', label)).toHaveTextContent('↓');
  });

  it('toggles 10/2/0 → 0/2/10 → 10/2/0 in the Applications table', async () => {
    const user = userEvent.setup();
    mockBackend([snapshot({ applications: applicationRows(field) })]);
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });

    await user.click(header('Applications', 'Application'));

    await user.click(header('Applications', label));
    expect(order('Applications')).toEqual(['ten', 'two', 'zero']);
    expect(columnHeader('Applications', label)).toHaveAttribute('aria-sort', 'descending');
    expect(header('Applications', label)).toHaveTextContent('↓');

    await user.click(header('Applications', label));
    expect(order('Applications')).toEqual(['zero', 'two', 'ten']);
    expect(columnHeader('Applications', label)).toHaveAttribute('aria-sort', 'ascending');
    expect(header('Applications', label)).toHaveTextContent('↑');

    await user.click(header('Applications', label));
    expect(order('Applications')).toEqual(['ten', 'two', 'zero']);
    expect(columnHeader('Applications', label)).toHaveAttribute('aria-sort', 'descending');
    expect(header('Applications', label)).toHaveTextContent('↓');
  });
});

describe('initial state', () => {
  it('opens on CPU ↓ and the very first CPU click reverses it', async () => {
    const user = userEvent.setup();
    mockBackend([snapshot({ processes: processRows('cpuPercent') })]);
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });
    await showProcesses(user);

    expect(order('Processes')).toEqual(['ten', 'two', 'zero']);
    expect(header('Processes', 'CPU')).toHaveTextContent('↓');

    await user.click(header('Processes', 'CPU'));
    expect(order('Processes')).toEqual(['zero', 'two', 'ten']);
    expect(header('Processes', 'CPU')).toHaveTextContent('↑');

    await user.click(header('Processes', 'CPU'));
    expect(order('Processes')).toEqual(['ten', 'two', 'zero']);
  });
});

/**
 * Text columns read ↓ for A → Z and ↑ for Z → A — the arrow points the way
 * the column opens, as it does for numbers — while `aria-sort` keeps the
 * literal data order.
 */
describe('name column', () => {
  const names = ['b', 'c', 'a'];

  it('toggles A/B/C ↓ → C/B/A ↑ → A/B/C ↓ in the Applications table', async () => {
    const user = userEvent.setup();
    mockBackend([
      snapshot({
        applications: names.map((name) => application({ key: `exe:/${name}`, displayName: name })),
      }),
    ]);
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });

    await user.click(header('Applications', 'Application'));
    expect(order('Applications')).toEqual(['a', 'b', 'c']);
    expect(header('Applications', 'Application')).toHaveTextContent('↓');
    expect(columnHeader('Applications', 'Application')).toHaveAttribute('aria-sort', 'ascending');

    await user.click(header('Applications', 'Application'));
    expect(order('Applications')).toEqual(['c', 'b', 'a']);
    expect(header('Applications', 'Application')).toHaveTextContent('↑');
    expect(columnHeader('Applications', 'Application')).toHaveAttribute('aria-sort', 'descending');

    await user.click(header('Applications', 'Application'));
    expect(order('Applications')).toEqual(['a', 'b', 'c']);
    expect(header('Applications', 'Application')).toHaveTextContent('↓');
    expect(columnHeader('Applications', 'Application')).toHaveAttribute('aria-sort', 'ascending');
  });

  it('toggles A/B/C ↓ → C/B/A ↑ → A/B/C ↓ in the Processes table', async () => {
    const user = userEvent.setup();
    mockBackend([
      snapshot({
        processes: names.map((name, index) =>
          process({ pid: index + 1, instanceId: `process:${index + 1}-1`, name }),
        ),
      }),
    ]);
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });
    await showProcesses(user);

    await user.click(header('Processes', 'Process'));
    expect(order('Processes')).toEqual(['a', 'b', 'c']);
    expect(header('Processes', 'Process')).toHaveTextContent('↓');
    expect(columnHeader('Processes', 'Process')).toHaveAttribute('aria-sort', 'ascending');

    await user.click(header('Processes', 'Process'));
    expect(order('Processes')).toEqual(['c', 'b', 'a']);
    expect(header('Processes', 'Process')).toHaveTextContent('↑');
    expect(columnHeader('Processes', 'Process')).toHaveAttribute('aria-sort', 'descending');

    await user.click(header('Processes', 'Process'));
    expect(order('Processes')).toEqual(['a', 'b', 'c']);
    expect(header('Processes', 'Process')).toHaveTextContent('↓');
    expect(columnHeader('Processes', 'Process')).toHaveAttribute('aria-sort', 'ascending');
  });
});

describe('other numeric columns', () => {
  it('toggles PID and Threads', async () => {
    const user = userEvent.setup();
    mockBackend([
      snapshot({
        processes: [
          process({ pid: 20, instanceId: 'process:20-1', name: 'b', threadCount: value(2) }),
          process({ pid: 30, instanceId: 'process:30-1', name: 'c', threadCount: value(10) }),
          process({ pid: 10, instanceId: 'process:10-1', name: 'a', threadCount: value(0) }),
        ],
      }),
    ]);
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });
    await showProcesses(user);

    await user.click(header('Processes', 'PID'));
    expect(order('Processes')).toEqual(['c', 'b', 'a']);
    await user.click(header('Processes', 'PID'));
    expect(order('Processes')).toEqual(['a', 'b', 'c']);

    await user.click(header('Processes', 'Threads'));
    expect(order('Processes')).toEqual(['c', 'b', 'a']);
    await user.click(header('Processes', 'Threads'));
    expect(order('Processes')).toEqual(['a', 'b', 'c']);
  });

  it('toggles the application process count', async () => {
    const user = userEvent.setup();
    mockBackend([
      snapshot({
        applications: [
          application({ key: 'exe:/a', displayName: 'a', processCount: 1 }),
          application({ key: 'exe:/b', displayName: 'b', processCount: 13 }),
        ],
      }),
    ]);
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });

    await user.click(header('Applications', 'Processes'));
    expect(order('Applications')).toEqual(['b', 'a']);
    await user.click(header('Applications', 'Processes'));
    expect(order('Applications')).toEqual(['a', 'b']);
  });
});

describe('unavailable values', () => {
  it('stay last in both directions and are never treated as zero', async () => {
    const user = userEvent.setup();
    mockBackend([
      snapshot({
        processes: [
          process({
            pid: 1,
            instanceId: 'process:1-1',
            name: 'unknown',
            cpuPercent: {
              value: null,
              availability: { status: 'permissionDenied', reason: 'denied' },
            },
          }),
          ...processRows('cpuPercent').slice(1),
        ],
      }),
    ]);
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });
    await showProcesses(user);

    expect(order('Processes')).toEqual(['ten', 'zero', 'unknown']);
    await user.click(header('Processes', 'CPU'));
    expect(order('Processes')).toEqual(['zero', 'ten', 'unknown']);
  });
});

describe('refresh', () => {
  it('keeps the chosen column and direction', async () => {
    const user = userEvent.setup();
    const backend = mockBackend([
      snapshot({ processes: processRows('cpuPercent') }),
      snapshot({ processes: processRows('cpuPercent') }),
    ]);
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });
    await showProcesses(user);

    await user.click(header('Processes', 'CPU'));
    expect(order('Processes')).toEqual(['zero', 'two', 'ten']);

    await user.click(screen.getByRole('button', { name: 'Refresh process details' }));
    await vi.waitFor(() => expect(backend.snapshot).toHaveBeenCalledTimes(2));

    expect(order('Processes')).toEqual(['zero', 'two', 'ten']);
    expect(header('Processes', 'CPU')).toHaveTextContent('↑');
  });
});
