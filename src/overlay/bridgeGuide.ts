import type {
  BridgeState,
  DesktopStatus,
  GnomeBridgeStatus,
  OverlayCapabilities,
} from '@/overlay/desktop';
import { t } from '@/i18n/i18n';

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

/** A bridge state's short name, for a chip. */
export function bridgeStateLabel(state: BridgeState): string {
  return t(`overlays.bridge.states.${state}`);
}

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
    : `cd ${t('overlays.bridge.sourcePlaceholder')}/integrations/gnome-shell && ./install.sh install`;
}

export function uninstallCommand(sourceDir: string | null | undefined): string {
  return sourceDir
    ? `cd ${quoted(sourceDir)} && ./install.sh uninstall`
    : `cd ${t('overlays.bridge.sourcePlaceholder')}/integrations/gnome-shell && ./install.sh uninstall`;
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
      title: needsUpdate ? t('overlays.bridge.steps.update') : t('overlays.bridge.steps.install'),
      detail: needsUpdate
        ? t('overlays.bridge.steps.updateDetail', {
            installed: bridge.installedVersion ?? '?',
            bundled: bridge.bundledVersion,
          })
        : t('overlays.bridge.steps.installDetail'),
      state: at(installed && !needsUpdate, !installed || needsUpdate),
      command: installCommand(sourceDir),
    },
    {
      id: 'login',
      title: t('overlays.bridge.steps.login'),
      detail: t('overlays.bridge.steps.loginDetail'),
      state: at(
        loaded && !bridge.restartPending,
        bridge.state === 'installedNeedsLogin' || bridge.restartPending,
      ),
    },
    {
      id: 'enable',
      title: t('overlays.bridge.steps.enable'),
      detail: t('overlays.bridge.steps.enableDetail'),
      state: at(enabled, loaded && !enabled),
    },
    {
      id: 'connected',
      title: t('overlays.bridge.steps.connected'),
      detail: bridge.connected
        ? t('overlays.bridge.steps.connectedDetail')
        : t('overlays.bridge.steps.notConnectedDetail'),
      state: at(bridge.connected, enabled && !bridge.connected),
    },
  ];
}

/** The capabilities the backend panel headlines, in order. Labels: `overlays.headline.<key>`. */
export const HEADLINE_CAPABILITIES: readonly (keyof Omit<
  OverlayCapabilities,
  'displayServer' | 'tray' | 'positioning'
>)[] = [
  'alwaysOnTop',
  'globalHotkey',
  'clickThrough',
  'transparentWindow',
  'multiMonitorPositioning',
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
    return t('overlays.placementHints.gnome');
  }
  if (caps.alwaysOnTop.status !== 'supported') {
    return t('overlays.placementHints.notOnTop');
  }
  return t('overlays.placementHints.noPositioning');
}
