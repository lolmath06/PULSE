import type { CSSProperties } from 'react';
import type { Overlay } from '@/overlay/model';

/** The overlay frame's border, corners and shadow (padding is in the layout boxes). */
export function overlayChromeStyle(overlay: Overlay): CSSProperties {
  // The border is drawn inside the box (an inset shadow), so it never adds to
  // the size computed by overlayLayout() — Fit to widgets stays exact.
  const shadows = [
    overlay.chrome.border === 'thin' ? 'inset 0 0 0 1px var(--pulse-border-strong)' : null,
    overlay.chrome.shadow ? '0 4px 18px rgba(0, 0, 0, 0.45)' : null,
  ].filter(Boolean);
  return {
    borderRadius: overlay.chrome.radius,
    boxShadow: shadows.length > 0 ? shadows.join(', ') : undefined,
  };
}
