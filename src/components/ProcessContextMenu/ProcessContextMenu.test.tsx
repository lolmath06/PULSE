import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { UserEvent } from '@testing-library/user-event';
import { ProcessDetailsCard } from '@/components/ProcessDetailsCard/ProcessDetailsCard';
import type { ProcessDetails, ProcessEntry } from '@/types/processes';
import {
  capabilities,
  destructiveCalls,
  details,
  mockBackend,
  process,
  snapshot,
  value,
} from '@/components/ProcessDetailsCard/fixtures';

const writeText = vi.fn<(text: string) => Promise<void>>();

function viewButton() {
  return within(screen.getByRole('group', { name: 'Process view' })).getByRole('button', {
    name: 'Processes',
  });
}

function row(name: string) {
  const table = screen.getByRole('table', { name: 'Processes' });
  return within(table).getByText(name).closest('tr') as HTMLElement;
}

async function setup(
  inspected: ProcessDetails = details(),
  rows: ProcessEntry[] = [process()],
  later: ProcessEntry[] = rows,
) {
  const user = userEvent.setup();
  // user-event installs its own clipboard stub; replace it with the spy.
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText },
  });
  const backend = mockBackend(
    [snapshot({ processes: rows }), snapshot({ processes: later })],
    inspected,
  );
  render(<ProcessDetailsCard />);
  await screen.findByRole('table', { name: 'Applications' });
  await user.click(viewButton());
  return { user, backend };
}

async function openMenu(user: UserEvent, name = 'bash') {
  fireEvent.contextMenu(row(name), { clientX: 40, clientY: 60 });
  const menu = await screen.findByRole('menu', { name: `Actions for ${name}` });
  // Wait until capabilities have arrived.
  await within(menu).findByRole('menuitem', { name: /^End process$/ });
  await vi.waitFor(() =>
    expect(within(menu).getByRole('menuitem', { name: /^Suspend/ })).not.toHaveTextContent(
      'Checking',
    ),
  );
  void user;
  return menu;
}

function item(menu: HTMLElement, name: RegExp | string) {
  return within(menu).getByRole('menuitem', { name });
}

beforeEach(() => {
  vi.restoreAllMocks();
  writeText.mockReset().mockResolvedValue(undefined);
});

afterEach(() => {
  vi.useRealTimers();
});

describe('process context menu', () => {
  it('opens on right click with the full menu', async () => {
    const { user } = await setup();
    const menu = await openMenu(user);

    for (const label of [
      'Inspect details',
      'Search online',
      'Open file location',
      'Copy',
      /^Suspend/,
      /^Resume/,
      /^End process$/,
      'End process tree',
      'Force kill',
      'Set priority',
      'Set affinity…',
    ]) {
      expect(item(menu, label)).toBeInTheDocument();
    }
  });

  it('closes on Escape and returns focus to the row', async () => {
    const { user } = await setup();
    const menu = await openMenu(user);

    await user.keyboard('{Escape}');
    expect(menu).not.toBeInTheDocument();
    expect(row('bash')).toHaveFocus();
  });

  it('closes on an outside click', async () => {
    const { user } = await setup();
    const menu = await openMenu(user);

    await user.click(screen.getByRole('heading', { name: 'Process details' }));
    expect(menu).not.toBeInTheDocument();
  });

  it('opens from the keyboard with Shift+F10', async () => {
    const { user } = await setup();
    row('bash').focus();
    await user.keyboard('{Shift>}{F10}{/Shift}');
    expect(await screen.findByRole('menu', { name: 'Actions for bash' })).toBeInTheDocument();
  });

  it('Inspect details opens the inspector', async () => {
    const { user } = await setup();
    const menu = await openMenu(user);
    await user.click(item(menu, 'Inspect details'));
    expect(
      await screen.findByRole('complementary', { name: 'Process inspector' }),
    ).toBeInTheDocument();
  });

  it('Search online sends names only — never a path, PID or user', async () => {
    const path = '/home/alice/private/project/token-app';
    const { user, backend } = await setup(
      details({
        name: 'token-app',
        executable: value({
          path,
          fileName: 'token-app',
          sizeBytes: value(1),
          modifiedAt: value(1),
          replacedOnDisk: false,
        }),
      }),
      [process({ name: 'token-app', executablePath: value(path) })],
    );
    const menu = await openMenu(user, 'token-app');
    await user.click(item(menu, 'Search online'));

    expect(backend.webSearch).toHaveBeenCalledTimes(1);
    const terms = backend.webSearch.mock.calls[0]?.[0] ?? [];
    expect(terms).toEqual(['token-app']);
    const joined = terms.join(' ');
    for (const secret of ['/home', 'alice', 'private', '100']) {
      expect(joined).not.toContain(secret);
    }
  });

  it('Open file location asks the backend for exactly this instance', async () => {
    const { user, backend } = await setup();
    const menu = await openMenu(user);
    await user.click(item(menu, 'Open file location'));
    expect(backend.openLocation).toHaveBeenCalledWith('process:100-7');
  });

  it('disables Open file location with the reason when the executable is unavailable', async () => {
    const { user } = await setup(
      details({
        capabilities: capabilities({
          openLocation: { allowed: false, reason: 'Executable unavailable: permission denied.' },
        }),
      }),
    );
    const menu = await openMenu(user);
    const location = item(menu, /Open file location/);
    expect(location).toHaveAttribute('aria-disabled', 'true');
    expect(location).toHaveTextContent('Executable unavailable: permission denied.');
  });

  it('copies from the Copy submenu, opened by keyboard', async () => {
    const { user } = await setup();
    const menu = await openMenu(user);

    item(menu, 'Copy').focus();
    await user.keyboard('{ArrowRight}');
    const submenu = await screen.findByRole('menu', { name: 'Copy' });
    await user.click(within(submenu).getByRole('menuitem', { name: 'PID' }));
    await vi.waitFor(() => expect(writeText).toHaveBeenCalledWith('100'));
  });

  it('copies the process instance ID and computes SHA-256 only when asked', async () => {
    const { user, backend } = await setup();
    let menu = await openMenu(user);
    await user.click(item(menu, 'Copy'));
    await user.click(
      within(await screen.findByRole('menu', { name: 'Copy' })).getByRole('menuitem', {
        name: 'Process instance ID',
      }),
    );
    await vi.waitFor(() => expect(writeText).toHaveBeenCalledWith('process:100-7'));
    expect(backend.sha256).not.toHaveBeenCalled();

    menu = await openMenu(user);
    await user.click(item(menu, 'Copy'));
    await user.click(
      within(await screen.findByRole('menu', { name: 'Copy' })).getByRole('menuitem', {
        name: /SHA-256/,
      }),
    );
    await vi.waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(
        'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
      ),
    );
    expect(backend.sha256).toHaveBeenCalledTimes(1);
  });

  it('enables Suspend and disables Resume for a running process', async () => {
    const { user, backend } = await setup();
    const menu = await openMenu(user);

    expect(item(menu, /^Suspend/)).not.toHaveAttribute('aria-disabled');
    const resume = item(menu, /^Resume/);
    expect(resume).toHaveAttribute('aria-disabled', 'true');
    expect(resume).toHaveTextContent('PULSE did not suspend this process.');

    await user.click(resume);
    expect(backend.resume).not.toHaveBeenCalled();

    await user.click(item(await openMenu(user), /^Suspend/));
    expect(backend.suspend).toHaveBeenCalledWith('process:100-7');
  });

  it('enables Resume only for what PULSE suspended', async () => {
    const { user, backend } = await setup(
      details({
        state: 'stopped',
        suspendedByPulse: true,
        capabilities: capabilities({
          suspend: { allowed: false, reason: 'Already suspended by PULSE.' },
          resume: { allowed: true, reason: null },
        }),
      }),
    );
    const menu = await openMenu(user);
    expect(item(menu, /^Suspend/)).toHaveAttribute('aria-disabled', 'true');
    await user.click(item(menu, /^Resume/));
    expect(backend.resume).toHaveBeenCalledWith('process:100-7');
  });

  it('asks before ending a process, and Cancel sends nothing', async () => {
    const { user, backend } = await setup();
    await user.click(item(await openMenu(user), /^End process$/));

    const dialog = await screen.findByRole('alertdialog');
    expect(dialog).toHaveTextContent('bash');
    expect(dialog).toHaveTextContent('PID 100');
    expect(destructiveCalls(backend)).toBe(0);

    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(destructiveCalls(backend)).toBe(0);
  });

  it('ends the process after confirmation, then refreshes exactly once', async () => {
    const { user, backend } = await setup();
    expect(backend.snapshot).toHaveBeenCalledTimes(1);
    await user.click(item(await openMenu(user), /^End process$/));
    await user.click(
      within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'End process' }),
    );

    await vi.waitFor(() => expect(backend.terminate).toHaveBeenCalledWith('process:100-7', false));
    await vi.waitFor(() => expect(backend.snapshot).toHaveBeenCalledTimes(2));
    expect(backend.terminate).toHaveBeenCalledTimes(1);
  });

  it('asks more firmly before ending a tree, with the descendant count', async () => {
    const { user, backend } = await setup(details(), [
      process(),
      process({ pid: 101, instanceId: 'process:101-7', name: 'child', parentPid: 100 }),
      process({ pid: 102, instanceId: 'process:102-7', name: 'grandchild', parentPid: 101 }),
      process({ pid: 200, instanceId: 'process:200-7', name: 'sibling', parentPid: 1 }),
    ]);
    await user.click(item(await openMenu(user), 'End process tree'));

    const dialog = await screen.findByRole('alertdialog');
    expect(dialog).toHaveTextContent('Root process: bash (PID 100)');
    expect(dialog).toHaveTextContent('About 2 descendant processes');
    expect(destructiveCalls(backend)).toBe(0);

    await user.click(within(dialog).getByRole('button', { name: 'End process tree' }));
    await vi.waitFor(() => expect(backend.terminateTree).toHaveBeenCalledWith('process:100-7'));
  });

  it('warns strongly before Force kill', async () => {
    const { user, backend } = await setup();
    await user.click(item(await openMenu(user), 'Force kill'));
    const dialog = await screen.findByRole('alertdialog');
    expect(dialog).toHaveTextContent('SIGKILL');
    expect(dialog).toHaveTextContent('cannot be caught');
    await user.click(within(dialog).getByRole('button', { name: 'Force kill' }));
    await vi.waitFor(() => expect(backend.terminate).toHaveBeenCalledWith('process:100-7', true));
  });

  it('sets a nice preset from the priority submenu without a confirmation', async () => {
    const { user, backend } = await setup();
    await user.click(item(await openMenu(user), 'Set priority'));
    const submenu = await screen.findByRole('menu', { name: 'Set priority' });
    expect(
      within(submenu).getByRole('menuitemradio', { name: /Normal \(nice 0\)/ }),
    ).toHaveAttribute('aria-checked', 'true');
    await user.click(within(submenu).getByRole('menuitemradio', { name: /Low \(nice 19\)/ }));

    await vi.waitFor(() =>
      expect(backend.setPriority).toHaveBeenCalledWith(
        'process:100-7',
        { kind: 'nice', value: 19 },
        false,
      ),
    );
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('asks a specific confirmation before Windows Realtime', async () => {
    const { user, backend } = await setup(
      details({
        priorityKind: 'windowsClass',
        priority: value({ kind: 'windowsClass', class: 'normal' }),
        forceKillSupported: false,
      }),
    );
    await user.click(item(await openMenu(user), 'Set priority'));
    const submenu = await screen.findByRole('menu', { name: 'Set priority' });
    await user.click(within(submenu).getByRole('menuitemradio', { name: 'Realtime' }));

    const dialog = await screen.findByRole('alertdialog');
    expect(dialog).toHaveTextContent('Realtime priority can make the system unresponsive.');
    expect(backend.setPriority).not.toHaveBeenCalled();

    await user.click(within(dialog).getByRole('button', { name: 'Set Realtime' }));
    await vi.waitFor(() =>
      expect(backend.setPriority).toHaveBeenCalledWith(
        'process:100-7',
        { kind: 'windowsClass', class: 'realtime' },
        true,
      ),
    );
  });

  it('hides Force kill on Windows, where ending is already immediate', async () => {
    const { user } = await setup(
      details({ forceKillSupported: false, priorityKind: 'windowsClass' }),
    );
    const menu = await openMenu(user);
    expect(within(menu).queryByRole('menuitem', { name: 'Force kill' })).toBeNull();
  });

  it('changes affinity through a dialog that never allows an empty set', async () => {
    const { user, backend } = await setup();
    await user.click(item(await openMenu(user), 'Set affinity…'));

    const dialog = await screen.findByRole('alertdialog');
    const apply = within(dialog).getByRole('button', { name: 'Apply' });
    for (const cpu of ['CPU 1', 'CPU 2', 'CPU 3']) {
      await user.click(within(dialog).getByLabelText(cpu));
    }
    expect(apply).toBeEnabled();
    await user.click(within(dialog).getByLabelText('CPU 0'));
    expect(apply).toBeDisabled();
    expect(within(dialog).getByRole('alert')).toHaveTextContent('Keep at least one CPU');
    expect(within(dialog).queryByRole('button', { name: 'Clear all' })).toBeNull();

    await user.click(within(dialog).getByLabelText('CPU 2'));
    await user.click(apply);
    await vi.waitFor(() => expect(backend.setAffinity).toHaveBeenCalledWith('process:100-7', [2]));
  });

  it('closes when its process disappears in a refresh', async () => {
    const { user } = await setup(
      details(),
      [process()],
      [process({ pid: 300, instanceId: 'process:300-1', name: 'other' })],
    );
    const menu = await openMenu(user);
    expect(menu).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Refresh process details' }));
    await vi.waitFor(() =>
      expect(screen.queryByRole('menu', { name: 'Actions for bash' })).toBeNull(),
    );
  });

  it('shows a stale refusal honestly instead of acting', async () => {
    const { user, backend } = await setup();
    backend.suspend.mockResolvedValue({
      status: 'staleProcess',
      reason: 'PID 100 now belongs to a different process than the one selected.',
      affectedCount: null,
      failedCount: null,
      tree: null,
    });
    await user.click(item(await openMenu(user), /^Suspend/));
    expect(await screen.findByRole('status')).toHaveTextContent('different process');
  });
});

describe('application context menu', () => {
  it('offers inspect, search and show processes — never an ambiguous kill', async () => {
    const user = userEvent.setup();
    const backend = mockBackend([snapshot()]);
    render(<ProcessDetailsCard />);
    const table = await screen.findByRole('table', { name: 'Applications' });

    fireEvent.contextMenu(within(table).getByText('bash').closest('tr') as HTMLElement);
    const menu = await screen.findByRole('menu', { name: 'Actions for bash' });
    const labels = within(menu)
      .getAllByRole('menuitem')
      .map((entry) => entry.textContent);
    expect(labels).toEqual(['Inspect application', 'Search online', 'Show processes']);

    await user.click(within(menu).getByRole('menuitem', { name: 'Show processes' }));
    expect(screen.getByRole('table', { name: 'Processes' })).toBeInTheDocument();
    expect(destructiveCalls(backend)).toBe(0);
  });
});

describe('no silent network', () => {
  it('inspecting, hashing and reading provenance make no web request', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const xhrOpen = vi.spyOn(XMLHttpRequest.prototype, 'open');
    const { user, backend } = await setup();

    await user.click(row('bash'));
    const inspector = await screen.findByRole('complementary', { name: 'Process inspector' });
    await within(inspector).findByText('bash-5.2.26-1.fc39.x86_64');
    await user.click(within(inspector).getByRole('button', { name: 'Compute SHA-256' }));
    await within(inspector).findByRole('button', { name: 'Check hash on VirusTotal' });

    expect(fetchSpy).not.toHaveBeenCalled();
    expect(xhrOpen).not.toHaveBeenCalled();
    expect(backend.webSearch).not.toHaveBeenCalled();
    expect(backend.hashLookup).not.toHaveBeenCalled();

    // Only the explicit buttons open the browser — and only with the hash.
    await user.click(within(inspector).getByRole('button', { name: 'Check hash on VirusTotal' }));
    expect(backend.hashLookup).toHaveBeenCalledWith(
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
      'virusTotal',
    );
    await user.click(within(inspector).getByRole('button', { name: 'Search hash online' }));
    expect(backend.hashLookup).toHaveBeenLastCalledWith(
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
      'web',
    );
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
