import type { CSSProperties } from 'react';
import type { Overlay } from '@/overlay/model';

/** The overlay frame's border, corners, shadow and padding. */
export function overlayChromeStyle(overlay: Overlay): CSSProperties {
  return {
    borderRadius: overlay.chrome.radius,
    border:
      overlay.chrome.border === 'thin'
        ? '1px solid var(--pulse-border-strong)'
        : '1px solid transparent',
    boxShadow: overlay.chrome.shadow ? '0 4px 18px rgba(0, 0, 0, 0.45)' : undefined,
    padding: overlay.chrome.padding,
  };
}
