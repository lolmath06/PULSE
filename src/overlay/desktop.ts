import { invokeCommand } from '@/services/tauri';

/** The desktop commands (`src-tauri/src/commands/desktop.rs`). */

export type CapabilityStatus = 'supported' | 'limited' | 'unsupported';

export interface Capability {
  readonly status: CapabilityStatus;
  readonly reason: string;
}

export interface OverlayCapabilities {
  readonly displayServer: 'windows' | 'x11' | 'wayland' | 'xWayland' | 'other';
  readonly alwaysOnTop: Capability;
  readonly clickThrough: Capability;
  readonly positioning: Capability;
  readonly transparentWindow: Capability;
  readonly globalHotkey: Capability;
  readonly multiMonitorPositioning: Capability;
  readonly tray: Capability;
}

export interface DesktopStatus {
  readonly capabilities: OverlayCapabilities;
  readonly hotkey: string | null;
  readonly hotkeyError: string | null;
}

export type OverlayAction =
  'lockAll' | 'editAll' | 'toggleLockAll' | 'showAll' | 'hideAll' | 'toggleVisibleAll';

export const getDesktopStatus = () => invokeCommand<DesktopStatus | null>('get_desktop_status');
export const setOverlayHotkey = (shortcut: string | null) =>
  invokeCommand<string | null>('set_overlay_hotkey', { shortcut });
export const overlayAction = (action: OverlayAction) =>
  invokeCommand<void>('overlay_action', { action });
export const openMainWindow = () => invokeCommand<void>('open_main_window');
export const openMiniWindow = () => invokeCommand<void>('open_mini_window');
export const quitApp = () => invokeCommand<void>('quit_app');

export const CAPABILITY_LABELS: Readonly<
  Record<keyof Omit<OverlayCapabilities, 'displayServer'>, string>
> = {
  alwaysOnTop: 'Always on top',
  clickThrough: 'Click-through when locked',
  positioning: 'Absolute positioning',
  transparentWindow: 'Transparent window',
  globalHotkey: 'Global shortcut',
  multiMonitorPositioning: 'Multi-monitor placement',
  tray: 'System tray',
};

export const DISPLAY_SERVER_LABELS: Readonly<Record<OverlayCapabilities['displayServer'], string>> =
  {
    windows: 'Windows',
    x11: 'X11',
    wayland: 'Wayland (native)',
    xWayland: 'XWayland (X11 inside a Wayland session)',
    other: 'Unsupported platform',
  };
