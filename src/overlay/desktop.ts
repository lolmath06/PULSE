import { listen } from '@tauri-apps/api/event';
import { invokeCommand, isTauriRuntime } from '@/services/tauri';

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

/**
 * Which backend delivers the global shortcut on this session: the plugin
 * (Windows, X11, XWayland), the XDG Desktop Portal (native Wayland), or none.
 */
export type ShortcutBackend =
  | { readonly kind: 'plugin' }
  | { readonly kind: 'portal'; readonly version: number }
  | { readonly kind: 'unavailable'; readonly reason: string };

/** Which native path puts overlays on screen (`src-tauri/src/overlay/backend.rs`). */
export type OverlayBackendKind =
  'windowsNative' | 'gnomeBridge' | 'standardWayland' | 'x11' | 'unsupported';

export type Verification = 'physicallyVerified' | 'implemented' | 'bestEffort' | 'unsupported';

export interface OverlayBackendInfo {
  readonly kind: OverlayBackendKind;
  readonly label: string;
  readonly detail: string;
  readonly verification: Verification;
}

/** `src-tauri/src/overlay/gnome_bridge.rs` → `BridgeState`. */
export type BridgeState =
  | 'notApplicable'
  | 'unavailable'
  | 'incompatible'
  | 'notInstalled'
  | 'installedNeedsLogin'
  | 'extensionsOff'
  | 'disabled'
  | 'error'
  | 'active'
  | 'changing';

export interface GnomeBridgeStatus {
  readonly state: BridgeState;
  /** English, for logs and older frontends; the UI words come from the code. */
  readonly summary: string;
  /** Stable code of `summary`. Absent from backends before i18n. */
  readonly summaryCode?: string;
  readonly guidance: string | null;
  /** Stable code of `guidance`. */
  readonly guidanceCode?: string | null;
  readonly shellVersion: string | null;
  readonly runningVersion: number | null;
  readonly installedVersion: number | null;
  readonly bundledVersion: number;
  readonly installPath: string | null;
  readonly installedByPulse: boolean;
  readonly connected: boolean;
  readonly restartPending: boolean;
  readonly updateAvailable: boolean;
  readonly canEnable: boolean;
  readonly canDisable: boolean;
  readonly error: string | null;
}

export interface DesktopStatus {
  /** Absent from backends older than Phase 12. */
  readonly backend?: OverlayBackendInfo;
  readonly gnomeBridge?: GnomeBridgeStatus | null;
  /** The repository copy of the extension, when PULSE runs from source. */
  readonly gnomeBridgeSource?: string | null;
  readonly capabilities: OverlayCapabilities;
  /** The shortcut in force — with the portal, as the desktop describes it. */
  readonly hotkey: string | null;
  readonly hotkeyError: string | null;
  /** Absent from older backends and until the backend is chosen. */
  readonly hotkeyBackend?: ShortcutBackend | null;
}

export type OverlayAction =
  'lockAll' | 'editAll' | 'toggleLockAll' | 'showAll' | 'hideAll' | 'toggleVisibleAll';

export const getDesktopStatus = () => invokeCommand<DesktopStatus | null>('get_desktop_status');
/** Re-reads GNOME Shell's view of the bridge. Never on a timer: on open and on request. */
export const refreshGnomeBridge = () => invokeCommand<DesktopStatus | null>('refresh_gnome_bridge');
/** GNOME Shell's own Enable/Disable, from an explicit button only. */
export const setGnomeBridgeEnabled = (enabled: boolean) =>
  invokeCommand<DesktopStatus | null>('set_gnome_bridge_enabled', { enabled });

/** Calls `listener` when the backend's desktop status changed on its own (the extension said hello). */
export function onDesktopStatusChanged(listener: () => void): () => void {
  if (!isTauriRuntime()) return () => undefined;
  const pending = listen('desktop-status-changed', () => listener());
  return () => void pending.then((unlisten) => unlisten()).catch(() => undefined);
}
export const setOverlayHotkey = (shortcut: string | null) =>
  invokeCommand<string | null>('set_overlay_hotkey', { shortcut });
export const overlayAction = (action: OverlayAction) =>
  invokeCommand<void>('overlay_action', { action });
export const openMainWindow = () => invokeCommand<void>('open_main_window');
export const openMiniWindow = () => invokeCommand<void>('open_mini_window');
export const quitApp = () => invokeCommand<void>('quit_app');

/** Every capability, in display order. Names: `overlays.capabilities.<key>`. */
export const CAPABILITY_KEYS: readonly (keyof Omit<OverlayCapabilities, 'displayServer'>)[] = [
  'alwaysOnTop',
  'clickThrough',
  'positioning',
  'transparentWindow',
  'globalHotkey',
  'multiMonitorPositioning',
  'tray',
];

/** The tray menu's words, in the interface language (the native menu cannot translate). */
export interface TrayLabels {
  readonly open: string;
  readonly edit: string;
  readonly lock: string;
  readonly toggle: string;
  readonly quit: string;
}

export const setTrayLabels = (labels: TrayLabels) =>
  invokeCommand<void>('set_tray_labels', { labels });
