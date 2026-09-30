import type {
  BridgeState,
  DesktopStatus,
  GnomeBridgeStatus,
  OverlayCapabilities,
} from '@/overlay/desktop';

/**
 * What the overlay backend panel says and offers, decided from the backend's
 * status alone — no guessing in the component.
 */

export type Tone = 'good' | 'warn' | 'bad' | 'off';

export function bridgeTone(state: BridgeState): Tone {
  switch (state) {
    case 'active':
      return 'good';
    case 'disabled':
    case 'installedNeedsLogin':
    case 'changing':
    case 'notInstalled':
      return 'warn';
    case 'error':
    case 'incompatible':
    case 'unavailable':
    case 'extensionsOff':
      return 'bad';
    case 'notApplicable':
      return 'off';
  }
}

export const BRIDGE_STATE_LABELS: Readonly<Record<BridgeState, string>> = {
  notApplicable: 'Not needed here',
  unavailable: 'Unavailable',
  incompatible: 'Incompatible',
  notInstalled: 'Not installed',
  installedNeedsLogin: 'Installed — log in again',
  extensionsOff: 'Extensions off',
  disabled: 'Disabled',
  error: 'Error',
  active: 'Active',
  changing: 'Changing…',
};

export type StepState = 'done' | 'current' | 'todo';

export interface BridgeStep {
  readonly id: 'install' | 'login' | 'enable' | 'connected';
  readonly title: string;
  readonly detail: string;
  readonly state: StepState;
  /** A shell command the user can copy, when the step is theirs to run. */
  readonly command?: string;
}

/** The directory `install.sh` lives in, quoted for a shell. */
function quoted(path: string): string {
  return `'${path.replace(/'/g, `'\\''`)}'`;
}

export function installCommand(sourceDir: string | null | undefined): string {
  return sourceDir
    ? `cd ${quoted(sourceDir)} && ./install.sh install`
    : 'cd <PULSE source>/integrations/gnome-shell && ./install.sh install';
}

export function uninstallCommand(sourceDir: string | null | undefined): string {
  return sourceDir
    ? `cd ${quoted(sourceDir)} && ./install.sh uninstall`
    : 'cd <PULSE source>/integrations/gnome-shell && ./install.sh uninstall';
}

/**
 * The setup path — install, log in again, enable, connected — with where the
 * user stands. Empty where the bridge does not apply or cannot work.
 */
export function bridgeSteps(
  bridge: GnomeBridgeStatus,
  sourceDir: string | null | undefined,
): BridgeStep[] {
  if (['notApplicable', 'incompatible', 'unavailable'].includes(bridge.state)) return [];
  const installed = bridge.state !== 'notInstalled';
  const loaded = installed && bridge.state !== 'installedNeedsLogin';
  const enabled = bridge.state === 'active';
  const needsUpdate = bridge.updateAvailable && !bridge.restartPending;
  const at = (done: boolean, current: boolean): StepState =>
    done ? 'done' : current ? 'current' : 'todo';

  return [
    {
      id: 'install',
      title: needsUpdate ? 'Update the extension' : 'Install the extension',
      detail: needsUpdate
        ? `v${bridge.installedVersion ?? '?'} is installed; this PULSE ships v${bridge.bundledVersion}.`
        : 'Copies it to ~/.local/share/gnome-shell/extensions/ — your account only, no sudo.',
      state: at(installed && !needsUpdate, !installed || needsUpdate),
      command: installCommand(sourceDir),
    },
    {
      id: 'login',
      title: 'Log out and back in once',
      detail: 'GNOME Shell 45 loads extension code only at login.',
      state: at(
        loaded && !bridge.restartPending,
        bridge.state === 'installedNeedsLogin' || bridge.restartPending,
      ),
    },
    {
      id: 'enable',
      title: 'Enable it',
      detail: 'With the button here, or in GNOME’s Extensions app.',
      state: at(enabled, loaded && !enabled),
    },
    {
      id: 'connected',
      title: 'Connected to PULSE',
      detail: bridge.connected
        ? 'The running extension greeted this PULSE.'
        : 'Versions from 2 greet PULSE as soon as it starts.',
      state: at(bridge.connected, enabled && !bridge.connected),
    },
  ];
}

/** The capabilities the backend panel headlines, in order. */
export const HEADLINE_CAPABILITIES: readonly {
  readonly key: keyof Omit<OverlayCapabilities, 'displayServer' | 'tray' | 'positioning'>;
  readonly label: string;
}[] = [
  { key: 'alwaysOnTop', label: 'Above other windows' },
  { key: 'globalHotkey', label: 'Global shortcut' },
  { key: 'clickThrough', label: 'Click-through' },
  { key: 'transparentWindow', label: 'Transparent' },
  { key: 'multiMonitorPositioning', label: 'Multi-monitor' },
];

/**
 * Whether a preset designed for a stable, full-width or edge placement will
 * behave as designed on this session. Customization is never blocked — this
 * only decides whether the preset carries a hint.
 */
export function placementHint(status: DesktopStatus | null): string | null {
  if (!status) return null;
  const caps = status.capabilities;
  if (caps.positioning.status === 'supported' && caps.alwaysOnTop.status === 'supported') {
    return null;
  }
  if (status.backend?.kind === 'gnomeBridge') {
    return 'Stays above other windows here; drag it to the edge once in Edit mode — GNOME places Wayland windows itself.';
  }
  if (caps.alwaysOnTop.status !== 'supported') {
    return 'Best with the GNOME bridge or Windows native overlays: here the compositor may put it behind a focused app.';
  }
  return 'Drag it into place once in Edit mode: this session cannot position windows.';
}
