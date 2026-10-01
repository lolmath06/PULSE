import { useContext, useMemo } from 'react';
import type { StyleId } from '@/design/styles';
import type { Look } from '@/design/look';
import { appLook, resolveLook } from '@/design/look';
import { useAppearance } from '@/design/store';
import { LookContext } from '@/design/lookContextValue';

/** The look of the surface a component is drawn on. */
export function useLook(): Look {
  return useContext(LookContext);
}

/** The look the whole app wears (the appearance section's style). */
export function useAppLook(): Look {
  const appearance = useAppearance();
  return useMemo(() => appLook(appearance), [appearance]);
}

/**
 * The look for a part that may wear its own style (`styleId`), or the app's
 * (`null`), with the user's customization either way.
 */
export function useScopedLook(styleId: StyleId | null | undefined): Look {
  const appearance = useAppearance();
  return useMemo(
    () => (styleId ? resolveLook(styleId, appearance.custom) : appLook(appearance)),
    [styleId, appearance],
  );
}

/**
 * Runs `change` inside a View Transition where the webview has one and motion
 * is on — a soft cross-fade between two styles or modes. Otherwise at once.
 */
export function withTransition(change: () => void, look?: Look) {
  const doc = document as Document & { startViewTransition?: (cb: () => void) => unknown };
  const reduce =
    look?.motion === 'none' ||
    (typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches);
  if (!reduce && typeof doc.startViewTransition === 'function') {
    doc.startViewTransition(change);
  } else {
    change();
  }
}
