import { useCallback, useRef, useState } from 'react';

/**
 * Whether an element is on screen, via `IntersectionObserver`.
 *
 * History panels below the fold stop re-querying on every batch until they
 * scroll into view — the backend event still arrives, it is just ignored.
 * Where the observer does not exist (tests), everything counts as visible.
 */
export function useInViewport<T extends Element>(): [(element: T | null) => void, boolean] {
  const [visible, setVisible] = useState(typeof IntersectionObserver === 'undefined');
  const observer = useRef<IntersectionObserver | null>(null);

  const ref = useCallback((element: T | null) => {
    observer.current?.disconnect();
    observer.current = null;
    if (!element || typeof IntersectionObserver === 'undefined') return;
    const next = new IntersectionObserver(
      (entries) => setVisible(entries.some((entry) => entry.isIntersecting)),
      { rootMargin: '200px 0px' },
    );
    next.observe(element);
    observer.current = next;
  }, []);

  return [ref, visible];
}
