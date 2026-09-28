import { readSectionNormalized, useUiSection, writeSection } from '@/config/uiConfig';
import type { OverlaysSection } from '@/overlay/model';
import { normalizeOverlays } from '@/overlay/model';

export function useOverlays(): OverlaysSection {
  return useUiSection('overlays', normalizeOverlays);
}

export function updateOverlays(action: (section: OverlaysSection) => OverlaysSection) {
  const current = readSectionNormalized('overlays', normalizeOverlays);
  const next = action(current);
  if (next !== current) writeSection('overlays', next);
}
