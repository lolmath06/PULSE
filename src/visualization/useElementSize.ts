import { useCallback, useRef, useState } from 'react';

export interface ElementSize {
  readonly width: number;
  readonly height: number;
}

/**
 * Tracks an element's content size with a `ResizeObserver`.
 *
 * Returns a callback ref rather than a ref object, so the observer follows the
 * element even when it is swapped out. Reports `0 × 0` until the first
 * measurement — and forever where `ResizeObserver` does not exist (tests), in
 * which case callers use their explicit or configured size.
 */
export function useElementSize<T extends HTMLElement>(): [
  (element: T | null) => void,
  ElementSize,
] {
  const [size, setSize] = useState<ElementSize>({ width: 0, height: 0 });
  const observer = useRef<ResizeObserver | null>(null);

  const ref = useCallback((element: T | null) => {
    observer.current?.disconnect();
    observer.current = null;
    if (!element || typeof ResizeObserver === 'undefined') return;

    const next = new ResizeObserver((entries) => {
      const box = entries[0]?.contentRect;
      if (!box) return;
      const width = Math.round(box.width);
      const height = Math.round(box.height);
      setSize((previous) =>
        previous.width === width && previous.height === height ? previous : { width, height },
      );
    });
    next.observe(element);
    observer.current = next;
  }, []);

  return [ref, size];
}
