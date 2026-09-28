import { registerUiConfigMigration } from '@/config/uiConfig';
import { VISUALIZATION_STORAGE_KEY, parseStore } from '@/visualization/store';

/**
 * One-time migrations into the shared UI configuration.
 *
 * Imported once by `main.tsx`, so they run in every window right after the
 * document loads and before anything renders.
 */

/**
 * Phase 10 → 11: visual preferences move from this webview's `localStorage`
 * into the shared `visualization` section.
 *
 * Runs only while that section does not exist yet, so it happens once; the
 * old key is left where it was (a Phase 10 build would still find it) but is
 * never read again.
 */
export function migrateLocalVisualizationPreferences(
  read: (section: 'visualization') => unknown,
  write: (section: 'visualization', value: unknown) => void,
  storage: Pick<Storage, 'getItem'> | undefined = globalThis.localStorage,
): boolean {
  if (read('visualization') !== undefined) return false;
  let text: string | null;
  try {
    text = storage?.getItem(VISUALIZATION_STORAGE_KEY) ?? null;
  } catch {
    return false;
  }
  const shape = parseStore(text);
  if (Object.keys(shape.charts).length === 0 && shape.customPresets.length === 0) return false;
  write('visualization', { ...shape, migratedFrom: `localStorage:${VISUALIZATION_STORAGE_KEY}` });
  return true;
}

registerUiConfigMigration((read, write) => {
  migrateLocalVisualizationPreferences(read, write);
});
