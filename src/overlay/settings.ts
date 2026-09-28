import { readSectionNormalized, useUiSection, writeSection } from '@/config/uiConfig';

/**
 * Desktop settings. The backend acts on them (`src-tauri/src/overlay/settings.rs`):
 * what closing the main window does, and the global shortcut. The shortcut is
 * changed through a command — never by writing this section directly — so a
 * conflict is detected and the previous shortcut kept.
 */
export type CloseBehavior = 'quit' | 'keep-running';

export const DEFAULT_HOTKEY = 'Ctrl+Shift+F12';

export interface DesktopSettings {
  readonly version: 1;
  readonly closeBehavior: CloseBehavior;
  /** `null`: no global shortcut. */
  readonly overlayHotkey: string | null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function normalizeSettings(raw: unknown): DesktopSettings {
  const source = isRecord(raw) ? raw : {};
  return {
    version: 1,
    closeBehavior: source.closeBehavior === 'keep-running' ? 'keep-running' : 'quit',
    overlayHotkey:
      source.overlayHotkey === null
        ? null
        : typeof source.overlayHotkey === 'string' && source.overlayHotkey.trim()
          ? source.overlayHotkey.trim().slice(0, 64)
          : DEFAULT_HOTKEY,
  };
}

export function useDesktopSettings(): DesktopSettings {
  return useUiSection('settings', normalizeSettings);
}

export function setCloseBehavior(closeBehavior: CloseBehavior) {
  const current = readSectionNormalized('settings', normalizeSettings);
  writeSection('settings', { ...current, closeBehavior });
}
