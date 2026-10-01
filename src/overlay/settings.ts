import { readSectionNormalized, useUiSection, writeSection } from '@/config/uiConfig';
import type { LanguagePreference } from '@/i18n/locales';
import { DEFAULT_LANGUAGE_PREFERENCE, isLanguagePreference } from '@/i18n/locales';

/**
 * Desktop settings. The backend acts on them (`src-tauri/src/overlay/settings.rs`):
 * what closing the main window does, and the global shortcut. The shortcut is
 * changed through a command — never by writing this section directly — so a
 * conflict is detected and the previous shortcut kept.
 *
 * The interface language lives here too (`src/i18n/language.ts`): one value
 * every window reads, so the main window, Mini and overlays always agree.
 */
export type CloseBehavior = 'quit' | 'keep-running';

export const DEFAULT_HOTKEY = 'Ctrl+Shift+F12';

export interface DesktopSettings {
  readonly version: 1;
  readonly closeBehavior: CloseBehavior;
  /** `null`: no global shortcut. */
  readonly overlayHotkey: string | null;
  /** The interface language, or `system` to follow the environment. */
  readonly language: LanguagePreference;
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
    // Older files have no language: they follow the system, the default.
    language: isLanguagePreference(source.language) ? source.language : DEFAULT_LANGUAGE_PREFERENCE,
  };
}

export function useDesktopSettings(): DesktopSettings {
  return useUiSection('settings', normalizeSettings);
}

export function setCloseBehavior(closeBehavior: CloseBehavior) {
  const current = readSectionNormalized('settings', normalizeSettings);
  writeSection('settings', { ...current, closeBehavior });
}

export function setLanguagePreference(language: LanguagePreference) {
  const current = readSectionNormalized('settings', normalizeSettings);
  if (current.language === language) return;
  writeSection('settings', { ...current, language });
}
