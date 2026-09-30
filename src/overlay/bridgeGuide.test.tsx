import { describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import type { DesktopStatus, GnomeBridgeStatus } from '@/overlay/desktop';
import { bridgeSteps, bridgeTone, installCommand, placementHint } from '@/overlay/bridgeGuide';
import { OverlayBackendPanel } from '@/components/Overlay/OverlayBackendPanel';

const cap = (status: 'supported' | 'limited' | 'unsupported', reason = 'because') => ({
  status,
  reason,
});

function bridge(patch: Partial<GnomeBridgeStatus> = {}): GnomeBridgeStatus {
  return {
    state: 'active',
    summary: 'Active',
    guidance: null,
    shellVersion: '45.10',
    runningVersion: 2,
    installedVersion: 2,
    bundledVersion: 2,
    installPath: '/home/u/.local/share/gnome-shell/extensions/pulse-overlay@jamby',
    installedByPulse: true,
    connected: true,
    restartPending: false,
    updateAvailable: false,
    canEnable: false,
    canDisable: true,
    error: null,
    ...patch,
  };
}

function status(patch: Partial<DesktopStatus> = {}): DesktopStatus {
  return {
    backend: {
      kind: 'gnomeBridge',
      label: 'GNOME native bridge',
      detail: 'Kept above by Mutter.',
      verification: 'physicallyVerified',
    },
    gnomeBridge: bridge(),
    gnomeBridgeSource: '/src/PULSE/integrations/gnome-shell',
    capabilities: {
      displayServer: 'wayland',
      alwaysOnTop: cap('supported'),
      clickThrough: cap('supported'),
      positioning: cap('unsupported'),
      transparentWindow: cap('supported'),
      globalHotkey: cap('supported'),
      multiMonitorPositioning: cap('unsupported'),
      tray: cap('limited'),
    },
    hotkey: 'Ctrl+Shift+F12',
    hotkeyError: null,
    ...patch,
  };
}

describe('GNOME bridge guidance', () => {
  it('maps every state to a tone', () => {
    expect(bridgeTone('active')).toBe('good');
    expect(bridgeTone('disabled')).toBe('warn');
    expect(bridgeTone('error')).toBe('bad');
    expect(bridgeTone('notApplicable')).toBe('off');
  });

  it('walks the setup path from not installed to connected', () => {
    const fresh = bridgeSteps(
      bridge({
        state: 'notInstalled',
        runningVersion: null,
        installedVersion: null,
        connected: false,
      }),
      '/src',
    );
    expect(fresh.map((step) => step.state)).toEqual(['current', 'todo', 'todo', 'todo']);
    expect(fresh[0]!.command).toContain('./install.sh install');

    const needsLogin = bridgeSteps(
      bridge({ state: 'installedNeedsLogin', connected: false }),
      '/src',
    );
    expect(needsLogin.map((step) => step.state)).toEqual(['done', 'current', 'todo', 'todo']);

    const disabled = bridgeSteps(bridge({ state: 'disabled', connected: false }), '/src');
    expect(disabled.map((step) => step.state)).toEqual(['done', 'done', 'current', 'todo']);

    const active = bridgeSteps(bridge(), '/src');
    expect(active.every((step) => step.state === 'done')).toBe(true);
  });

  it('asks for an update, then a login, when an older copy runs', () => {
    const outdated = bridgeSteps(
      bridge({ runningVersion: 1, installedVersion: 1, updateAvailable: true, connected: false }),
      '/src',
    );
    expect(outdated[0]).toMatchObject({ title: 'Update the extension', state: 'current' });
    const pending = bridgeSteps(
      bridge({ runningVersion: 1, installedVersion: 2, restartPending: true, connected: false }),
      '/src',
    );
    expect(pending[1]!.state).toBe('current');
  });

  it('offers no steps where the bridge cannot work', () => {
    for (const state of ['notApplicable', 'incompatible', 'unavailable'] as const) {
      expect(bridgeSteps(bridge({ state }), '/src')).toEqual([]);
    }
  });

  it('quotes the source path for the shell', () => {
    expect(installCommand("/home/o'neil/PULSE")).toBe(
      `cd '/home/o'\\''neil/PULSE' && ./install.sh install`,
    );
    expect(installCommand(null)).toContain('<PULSE source>');
  });

  it('hints at placement only where the platform cannot deliver it', () => {
    expect(placementHint(null)).toBeNull();
    expect(placementHint(status())).toContain('drag it to the edge');
    const windows = status({
      backend: { kind: 'windowsNative', label: 'Windows', detail: '', verification: 'implemented' },
      capabilities: { ...status().capabilities, positioning: cap('supported') },
    });
    expect(placementHint(windows)).toBeNull();
    const plain = status({
      backend: {
        kind: 'standardWayland',
        label: 'Wayland',
        detail: '',
        verification: 'bestEffort',
      },
      capabilities: { ...status().capabilities, alwaysOnTop: cap('limited') },
    });
    expect(placementHint(plain)).toContain('GNOME bridge');
  });
});

describe('Overlay backend panel', () => {
  it('shows the active GNOME bridge, connected, with its capabilities', () => {
    render(<OverlayBackendPanel status={status()} error={null} onStatus={() => undefined} />);
    expect(screen.getByText('GNOME native bridge')).toBeInTheDocument();
    expect(screen.getByText('Verified on real hardware')).toBeInTheDocument();
    const chips = screen.getByRole('list', { name: 'What overlays can do here' });
    expect(within(chips).getByText('Above other windows')).toBeInTheDocument();
    const section = screen.getByLabelText('GNOME bridge');
    expect(within(section).getByText('Active', { selector: '.pill' })).toBeInTheDocument();
    expect(within(section).getByText('Connected')).toBeInTheDocument();
    expect(within(section).getByRole('button', { name: 'Disable' })).toBeInTheDocument();
    expect(within(section).queryByRole('button', { name: 'Enable' })).toBeNull();
  });

  it('offers Enable when installed but disabled, and the install command when missing', () => {
    const { rerender } = render(
      <OverlayBackendPanel
        status={status({
          gnomeBridge: bridge({
            state: 'disabled',
            canEnable: true,
            canDisable: false,
            connected: false,
          }),
        })}
        error={null}
        onStatus={() => undefined}
      />,
    );
    expect(screen.getByRole('button', { name: 'Enable' })).toBeInTheDocument();
    rerender(
      <OverlayBackendPanel
        status={status({
          gnomeBridge: bridge({
            state: 'notInstalled',
            summary: 'Not installed',
            runningVersion: null,
            installedVersion: null,
            connected: false,
            canDisable: false,
          }),
        })}
        error={null}
        onStatus={() => undefined}
      />,
    );
    expect(screen.getByText(/install\.sh install/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Enable' })).toBeNull();
  });

  it('hides the bridge section where it does not apply, and says so when the backend is away', () => {
    const { rerender } = render(
      <OverlayBackendPanel
        status={status({
          backend: { kind: 'x11', label: 'X11 window', detail: 'X11.', verification: 'bestEffort' },
          gnomeBridge: bridge({ state: 'notApplicable' }),
        })}
        error={null}
        onStatus={() => undefined}
      />,
    );
    expect(screen.queryByLabelText('GNOME bridge')).toBeNull();
    rerender(<OverlayBackendPanel status={null} error="offline" onStatus={() => undefined} />);
    expect(screen.getByText(/Backend unavailable/)).toBeInTheDocument();
  });
});
