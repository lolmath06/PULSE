import { useEffect, useState } from 'react';
import type { PlatformInfo } from '@/types/platform';
import { getPlatformInfo } from '@/services/platform';

export type PlatformInfoState =
  | { status: 'loading' }
  | { status: 'ready'; info: PlatformInfo }
  | { status: 'error'; message: string };

/** Loads platform information once, on mount. */
export function usePlatformInfo(): PlatformInfoState {
  const [state, setState] = useState<PlatformInfoState>({ status: 'loading' });

  useEffect(() => {
    let cancelled = false;

    getPlatformInfo()
      .then((info) => {
        if (!cancelled) setState({ status: 'ready', info });
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setState({
          status: 'error',
          message: error instanceof Error ? error.message : String(error),
        });
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return state;
}
