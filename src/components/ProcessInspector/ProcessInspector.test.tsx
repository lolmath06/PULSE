import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ProcessDetailsCard } from '@/components/ProcessDetailsCard/ProcessDetailsCard';
import type { FileHash, ProcessDetails, ProcessQuery, Provenance } from '@/types/processes';
import {
  RPM,
  capabilities,
  details,
  failedQuery,
  missing,
  mockBackend,
  outcome,
  process,
  query,
  snapshot,
  value,
} from '@/components/ProcessDetailsCard/fixtures';

const HASH = 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad';

async function openInspector(inspected: ProcessDetails = details(), rows = [process()]) {
  const user = userEvent.setup();
  const backend = mockBackend([snapshot({ processes: rows })], inspected);
  render(<ProcessDetailsCard />);
  await screen.findByRole('table', { name: 'Applications' });
  await user.click(
    within(screen.getByRole('group', { name: 'Process view' })).getByRole('button', {
      name: 'Processes',
    }),
  );
  const table = screen.getByRole('table', { name: 'Processes' });
  await user.click(within(table).getAllByRole('row')[1] as HTMLElement);
  const inspector = await screen.findByRole('complementary', { name: 'Process inspector' });
  return { user, backend, inspector };
}

function viewButton() {
  return within(screen.getByRole('group', { name: 'Process view' })).getByRole('button', {
    name: 'Processes',
  });
}

beforeEach(() => {
  vi.restoreAllMocks();
});

describe('ProcessInspector', () => {
  it('opens beside the table on a normal click and can be closed', async () => {
    const { user, inspector } = await openInspector();

    expect(screen.getByRole('table', { name: 'Processes' })).toBeInTheDocument();
    expect(await within(inspector).findByText('/usr/bin/bash')).toBeInTheDocument();

    await user.click(within(inspector).getByRole('button', { name: 'Close inspector' }));
    expect(screen.queryByRole('complementary', { name: 'Process inspector' })).toBeNull();
  });

  it('shows identity, executable and scheduling for a normal executable', async () => {
    const { inspector, backend } = await openInspector();

    expect(await within(inspector).findByText('process:100-7')).toBeInTheDocument();
    expect(within(inspector).getByText(/alice \(1000\)/)).toBeInTheDocument();
    expect(within(inspector).getByText('x86_64')).toBeInTheDocument();
    expect(within(inspector).getByText('nice 0 (Normal)')).toBeInTheDocument();
    expect(within(inspector).getByText('CPU 0–3 (4 of 4)')).toBeInTheDocument();
    expect(within(inspector).getByText(/systemd/)).toBeInTheDocument();
    expect(backend.details).toHaveBeenCalledWith('process:100-7');
    // Lazy: the hash is never computed on open.
    expect(backend.sha256).not.toHaveBeenCalled();
  });

  it('names the Fedora package that owns the executable', async () => {
    const { inspector } = await openInspector();
    expect(await within(inspector).findByText('bash-5.2.26-1.fc39.x86_64')).toBeInTheDocument();
  });

  it('marks a user-local executable without calling it unsafe', async () => {
    const user = userEvent.setup();
    const backend = mockBackend([snapshot()]);
    backend.provenance.mockResolvedValue(
      query<Provenance>({ kind: 'notPackaged', location: 'userHome' }),
    );
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });
    await user.click(viewButton());
    await user.click(
      within(screen.getByRole('table', { name: 'Processes' })).getAllByRole(
        'row',
      )[1] as HTMLElement,
    );

    const inspector = await screen.findByRole('complementary', { name: 'Process inspector' });
    expect(await within(inspector).findByText(/User\/local executable/)).toBeInTheDocument();
    // No verdict badge: the only mention of "safe" is the disclaimer.
    expect(within(inspector).queryByText(/^(safe|malware|unsafe)$/i)).toBeNull();
    expect(inspector.textContent?.toLowerCase()).not.toContain('malware');
  });

  it('degrades cleanly for a kernel thread', async () => {
    const kernel = details({
      name: 'kworker/3:1',
      category: 'kernelThread',
      executable: missing({
        status: 'notDetected',
        reason: 'a kernel thread has no userspace executable',
      }),
      architecture: missing({ status: 'notDetected', reason: 'part of the kernel' }),
      capabilities: capabilities({
        terminate: {
          allowed: false,
          reason: 'Kernel thread — PULSE does not control kernel threads.',
        },
        terminateTree: {
          allowed: false,
          reason: 'Kernel thread — PULSE does not control kernel threads.',
        },
        suspend: {
          allowed: false,
          reason: 'Kernel thread — PULSE does not control kernel threads.',
        },
        openLocation: { allowed: false, reason: 'Kernel threads have no executable.' },
        computeHash: { allowed: false, reason: 'Kernel threads have no executable.' },
      }),
    });
    const { inspector } = await openInspector(kernel, [
      process({
        name: 'kworker/3:1',
        classification: 'kernelThread',
        executablePath: missing({ status: 'notDetected', reason: 'kernel' }),
      }),
    ]);

    expect(
      await within(inspector).findByText(/A kernel thread runs inside the kernel/),
    ).toBeInTheDocument();
    expect(within(inspector).getByText('Not applicable to a kernel thread')).toBeInTheDocument();
    expect(within(inspector).getByRole('button', { name: 'End process' })).toBeDisabled();
    expect(within(inspector).getByRole('button', { name: 'Compute SHA-256' })).toBeDisabled();
    expect(
      within(inspector).getAllByText(/does not control kernel threads/).length,
    ).toBeGreaterThan(0);
  });

  it('explains a permission refusal rather than inventing an owner', async () => {
    const { inspector } = await openInspector(
      details({
        owner: missing({
          status: 'permissionDenied',
          reason: 'Windows refused to open this process token',
        }),
        capabilities: capabilities({
          terminate: { allowed: false, reason: 'Permission denied: owned by UID 0.' },
        }),
      }),
    );

    expect(
      await within(inspector).findAllByText(/refused to open this process token/),
    ).not.toHaveLength(0);
    expect(within(inspector).queryByText('SYSTEM')).toBeNull();
    expect(within(inspector).getByText('Permission denied: owned by UID 0.')).toBeInTheDocument();
  });

  it('says the process exited when it vanished', async () => {
    const user = userEvent.setup();
    const backend = mockBackend([snapshot()]);
    backend.details.mockResolvedValue(
      failedQuery(outcome('processGone', 'Process 100 has already exited.')),
    );
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });
    await user.click(viewButton());
    await user.click(
      within(screen.getByRole('table', { name: 'Processes' })).getAllByRole(
        'row',
      )[1] as HTMLElement,
    );

    const inspector = await screen.findByRole('complementary', { name: 'Process inspector' });
    expect(await within(inspector).findByText(/Process exited/)).toBeInTheDocument();
    expect(within(inspector).getByRole('button', { name: 'End process' })).toBeDisabled();
  });

  it('shows a trusted Windows signature, its publisher and the version resource', async () => {
    const user = userEvent.setup();
    const windows = details({
      name: '2.1.278',
      executable: value({
        path: 'C:\\Users\\alice\\AppData\\Local\\AnthropicClaude\\claude.exe',
        fileName: 'claude.exe',
        sizeBytes: value(1),
        modifiedAt: value(1),
        replacedOnDisk: false,
      }),
      versionInfo: value({
        fileDescription: 'Claude',
        productName: 'Claude',
        companyName: 'Anthropic, PBC',
        fileVersion: '2.1.278',
        productVersion: '2.1.278',
      }),
      priority: value({ kind: 'windowsClass', class: 'normal' }),
      priorityKind: 'windowsClass',
      forceKillSupported: false,
    });
    const backend = mockBackend([snapshot({ processes: [process({ name: '2.1.278' })] })], windows);
    backend.provenance.mockResolvedValue(
      query<Provenance>({
        kind: 'signature',
        signature: {
          trust: 'trusted',
          source: 'embedded',
          publisher: 'Anthropic, PBC',
          detail: 'ok',
        },
      }),
    );
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });
    await user.click(viewButton());
    await user.click(
      within(screen.getByRole('table', { name: 'Processes' })).getAllByRole(
        'row',
      )[1] as HTMLElement,
    );
    const inspector = await screen.findByRole('complementary', { name: 'Process inspector' });

    expect(await within(inspector).findByText('Trusted signature')).toBeInTheDocument();
    expect(within(inspector).getAllByText('Anthropic, PBC').length).toBeGreaterThan(0);
    // Product name and process name are both shown when they differ.
    expect(within(inspector).getByRole('heading', { name: 'Claude' })).toBeInTheDocument();
    const processName = within(inspector).getByText('Process name').nextElementSibling;
    expect(processName).toHaveTextContent('2.1.278');
    expect(within(inspector).getByText(/declared by the file itself/)).toBeInTheDocument();
    expect(within(inspector).queryByRole('button', { name: 'Force kill' })).toBeNull();
  });

  it('shows an unsigned file as unsigned, not as malicious', async () => {
    const user = userEvent.setup();
    const backend = mockBackend([snapshot()], details({ priorityKind: 'windowsClass' }));
    backend.provenance.mockResolvedValue(
      query<Provenance>({
        kind: 'signature',
        signature: { trust: 'unsigned', source: null, publisher: null, detail: 'no signature' },
      }),
    );
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });
    await user.click(viewButton());
    await user.click(
      within(screen.getByRole('table', { name: 'Processes' })).getAllByRole(
        'row',
      )[1] as HTMLElement,
    );
    const inspector = await screen.findByRole('complementary', { name: 'Process inspector' });

    expect(await within(inspector).findByText('Unsigned')).toBeInTheDocument();
    expect(inspector.textContent?.toLowerCase()).not.toContain('malware');
  });

  it('computes the hash only on click, shows progress and then the digest', async () => {
    const { user, inspector, backend } = await openInspector();
    let finish: (value: ProcessQuery<FileHash>) => void = () => undefined;
    backend.sha256.mockReturnValue(
      new Promise<ProcessQuery<FileHash>>((resolve) => {
        finish = resolve;
      }),
    );

    expect(backend.sha256).not.toHaveBeenCalled();
    await user.click(await within(inspector).findByRole('button', { name: 'Compute SHA-256' }));
    expect(within(inspector).getAllByText('Computing…').length).toBeGreaterThan(0);

    finish(query({ status: 'computed', sha256: HASH, sizeBytes: 3, reason: null }));
    expect(await within(inspector).findByText(HASH)).toBeInTheDocument();
    expect(
      within(inspector).getByRole('button', { name: 'Check hash on VirusTotal' }),
    ).toBeInTheDocument();
    expect(backend.sha256).toHaveBeenCalledTimes(1);
  });

  it('never presents a hash of a file that changed while being read', async () => {
    const { user, inspector, backend } = await openInspector();
    backend.sha256.mockResolvedValue(
      query({
        status: 'changedWhileHashing',
        sha256: null,
        sizeBytes: null,
        reason: 'The executable changed while it was being read.',
      }),
    );

    await user.click(await within(inspector).findByRole('button', { name: 'Compute SHA-256' }));
    expect(
      await within(inspector).findByText(/changed while it was being read/),
    ).toBeInTheDocument();
    expect(
      within(inspector).queryByRole('button', { name: 'Check hash on VirusTotal' }),
    ).toBeNull();
  });

  it('keeps a very long path and a Unicode publisher readable', async () => {
    const long = `/opt/${'très-long-répertoire/'.repeat(20)}outil`;
    const user = userEvent.setup();
    const backend = mockBackend(
      [snapshot()],
      details({
        executable: value({
          path: long,
          fileName: 'outil',
          sizeBytes: value(1),
          modifiedAt: value(1),
          replacedOnDisk: false,
        }),
      }),
    );
    backend.provenance.mockResolvedValue(
      query<Provenance>({
        kind: 'signature',
        signature: {
          trust: 'signedButUntrusted',
          source: 'embedded',
          publisher: 'Société Générale 株式会社',
          detail: 'untrusted root',
        },
      }),
    );
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });
    await user.click(viewButton());
    await user.click(
      within(screen.getByRole('table', { name: 'Processes' })).getAllByRole(
        'row',
      )[1] as HTMLElement,
    );
    const inspector = await screen.findByRole('complementary', { name: 'Process inspector' });

    const path = await within(inspector).findByText(long);
    expect(path).toHaveClass('inspector__path');
    expect(await within(inspector).findByText(/Société Générale 株式会社/)).toBeInTheDocument();
    expect(within(inspector).getByText(/claimed, not trusted/)).toBeInTheDocument();
  });

  it('is a drawer with its own close control, usable at any width', async () => {
    const { inspector } = await openInspector();
    expect(inspector).toHaveClass('inspector');
    expect(within(inspector).getByRole('button', { name: 'Close inspector' })).toBeInTheDocument();
  });

  it('shows Process exited after a refresh drops the process', async () => {
    const user = userEvent.setup();
    const backend = mockBackend([
      snapshot(),
      snapshot({ processes: [process({ pid: 200, instanceId: 'process:200-1', name: 'other' })] }),
    ]);
    render(<ProcessDetailsCard />);
    await screen.findByRole('table', { name: 'Applications' });
    await user.click(viewButton());
    await user.click(
      within(screen.getByRole('table', { name: 'Processes' })).getAllByRole(
        'row',
      )[1] as HTMLElement,
    );
    const inspector = await screen.findByRole('complementary', { name: 'Process inspector' });
    await within(inspector).findByText('/usr/bin/bash');

    await user.click(screen.getByRole('button', { name: 'Refresh process details' }));
    await vi.waitFor(() => expect(backend.snapshot).toHaveBeenCalledTimes(2));

    expect(await within(inspector).findByText(/Process exited/)).toBeInTheDocument();
    expect(within(inspector).getByRole('button', { name: 'Suspend' })).toBeDisabled();
  });

  it('marks PULSE itself', async () => {
    const { inspector } = await openInspector(
      details({
        isSelf: true,
        capabilities: capabilities({
          suspend: {
            allowed: false,
            reason: 'Suspending PULSE would freeze the window needed to resume it.',
          },
        }),
      }),
    );
    expect(await within(inspector).findByText(/This is PULSE itself/)).toBeInTheDocument();
    expect(within(inspector).getByRole('button', { name: 'Suspend' })).toBeDisabled();
  });

  it('resolves RPM provenance as evidence, with no verdict', async () => {
    const { inspector } = await openInspector();
    await within(inspector).findByText('bash-5.2.26-1.fc39.x86_64');
    expect(
      within(inspector).getByText(/Provenance is evidence, not a verdict/),
    ).toBeInTheDocument();
    expect(RPM.kind).toBe('rpmPackage');
  });
});
