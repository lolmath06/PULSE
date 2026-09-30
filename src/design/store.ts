import { readSectionNormalized, useUiSection, writeSection } from '@/config/uiConfig';
import type { AppearanceSection } from '@/design/appearance';
import { normalizeAppearance } from '@/design/appearance';

/** The appearance section, as every window sees it. */
export function useAppearance(): AppearanceSection {
  return useUiSection('appearance', normalizeAppearance);
}

export function readAppearance(): AppearanceSection {
  return readSectionNormalized('appearance', normalizeAppearance);
}

/** Applies a pure action to the current section and saves the result. */
export function updateAppearance(action: (section: AppearanceSection) => AppearanceSection) {
  const current = readAppearance();
  const next = action(current);
  if (next !== current) writeSection('appearance', next);
}
