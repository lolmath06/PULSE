import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ProcessSnapshot } from '@/types/processes';
import { getProcessSnapshot } from '@/services/processes';

export interface ProcessDetailsState {
  /** `loading` only before the first response; a refresh keeps the old data. */
  readonly status: 'loading' | 'ready' | 'error';
  /** The most recent snapshot, or `null` before the first one arrives. */
  readonly snapshot: ProcessSnapshot | null;
  /** Set when the whole call failed, e.g. outside the Tauri runtime. */
  readonly message?: string;
  /** True while a refresh is in flight over already-displayed data. */
  readonly refreshing: boolean;
  /** Walks the process table again. */
  readonly refresh: () => void;
}

/**
 * Loads a process snapshot and re-takes it on demand.
 *
 * # One call, not one per process
 *
 * The backend walks the whole table once and returns everything derived from
 * it: rows, applications, counts and the duration the walk took. There is no
 * per-process request, no per-process command and no second call to fill in
 * the application view — which is grouped from the same rows.
 *
 * # The first snapshot has no baseline, and says so
 *
 * CPU shares and I/O rates exist only *between* two snapshots. On mount there
 * is one, so those three columns report `waiting for another sample` rather
 * than `0`. This hook deliberately does **not** fire a hidden second request
 * to paper over it: that would be a polling loop with one iteration, and it
 * would hide the very thing that makes the numbers trustworthy — that a rate
 * is measured, not read.
 *
 * Memory and thread counts are snapshot values and are shown immediately.
 *
 * # Deliberately not polling
 *
 * Exactly one snapshot on mount and one per click of *Refresh*. A test
 * advances timers and asserts no further request is made.
 */
export function useProcessDetails(): ProcessDetailsState {
  const [snapshot, setSnapshot] = useState<ProcessSnapshot | null>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [message, setMessage] = useState<string | undefined>(undefined);
  const [refreshing, setRefreshing] = useState(false);

  // Guards against a response arriving after the component unmounted.
  const mounted = useRef(true);

  const apply = useCallback((next: ProcessSnapshot) => {
    setSnapshot(next);
    setStatus('ready');
    setMessage(undefined);
  }, []);

  const fail = useCallback((error: unknown) => {
    setStatus('error');
    setMessage(error instanceof Error ? error.message : String(error));
  }, []);

  useEffect(() => {
    mounted.current = true;
    let cancelled = false;

    void getProcessSnapshot()
      .then((next) => {
        if (!cancelled) apply(next);
      })
      .catch((error: unknown) => {
        if (!cancelled) fail(error);
      });

    return () => {
      cancelled = true;
      mounted.current = false;
    };
  }, [apply, fail]);

  const refresh = useCallback(() => {
    setRefreshing(true);

    void getProcessSnapshot()
      .then((next) => {
        if (mounted.current) apply(next);
      })
      .catch((error: unknown) => {
        if (mounted.current) fail(error);
      })
      .finally(() => {
        if (mounted.current) setRefreshing(false);
      });
  }, [apply, fail]);

  return useMemo(
    () => ({ status, snapshot, message, refreshing, refresh }),
    [status, snapshot, message, refreshing, refresh],
  );
}
