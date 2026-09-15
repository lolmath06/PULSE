import { usePlatformInfo } from '@/hooks/usePlatformInfo';
import { formatDisplayServer, formatPlatformLabel } from '@/utils/format';
import { APP_VERSION } from '@/app/constants';

/**
 * Discreet footer proving the React -> Tauri -> Rust round trip.
 *
 * Phase 0 only: this is not the metrics surface, it is the integration check.
 */
export function StatusBar() {
  const state = usePlatformInfo();

  return (
    <footer className="status-bar" aria-label="Backend status">
      {state.status === 'loading' && <span className="status-bar__muted">Querying backend…</span>}

      {state.status === 'error' && (
        <span className="status-bar__muted" title={state.message}>
          Backend unavailable — UI-only mode
        </span>
      )}

      {state.status === 'ready' && (
        <>
          <span className="status-bar__dot" aria-hidden="true" />
          <span>{formatPlatformLabel(state.info.os, state.info.osVersion)}</span>
          <span className="status-bar__sep">·</span>
          <span>{state.info.arch}</span>
          {formatDisplayServer(state.info.displayServer) && (
            <>
              <span className="status-bar__sep">·</span>
              <span>{formatDisplayServer(state.info.displayServer)}</span>
            </>
          )}
        </>
      )}

      <span className="status-bar__spacer" />
      <span className="status-bar__muted">
        PULSE {state.status === 'ready' ? state.info.appVersion : APP_VERSION}
      </span>
    </footer>
  );
}
