import { usePlatformInfo } from '@/hooks/usePlatformInfo';
import { formatDisplayServer, formatPlatformLabel } from '@/utils/format';
import { APP_TAGLINE, APP_VERSION } from '@/app/constants';
import { MetricsEngineCard } from '@/components/MetricsEngineCard/MetricsEngineCard';
import { LiveSampleCard } from '@/components/LiveSampleCard/LiveSampleCard';
import { CpuDetailsCard } from '@/components/CpuDetailsCard/CpuDetailsCard';
import { GpuDetailsCard } from '@/components/GpuDetailsCard/GpuDetailsCard';
import { StorageDetailsCard } from '@/components/StorageDetailsCard/StorageDetailsCard';
import { NetworkDetailsCard } from '@/components/NetworkDetailsCard/NetworkDetailsCard';
import { ProcessDetailsCard } from '@/components/ProcessDetailsCard/ProcessDetailsCard';

export function OverviewPage() {
  const state = usePlatformInfo();

  return (
    <section className="page">
      <h1 className="page__hero">PULSE</h1>
      <p className="page__subtitle">{APP_TAGLINE}</p>
      <p className="page__note">Version {APP_VERSION}</p>

      <div className="card" aria-label="Detected platform">
        <h2 className="card__title">Detected platform</h2>

        {state.status === 'loading' && <p className="card__muted">Querying backend…</p>}

        {state.status === 'error' && (
          <p className="card__muted">
            Backend unavailable. Run PULSE with <code>pnpm app:dev</code> to reach the Rust layer.
          </p>
        )}

        {state.status === 'ready' && (
          <dl className="kv">
            <div className="kv__row">
              <dt>Operating system</dt>
              <dd>{formatPlatformLabel(state.info.os, state.info.osVersion)}</dd>
            </div>
            <div className="kv__row">
              <dt>Platform layer</dt>
              <dd>{state.info.platform}</dd>
            </div>
            <div className="kv__row">
              <dt>Architecture</dt>
              <dd>{state.info.arch}</dd>
            </div>
            {formatDisplayServer(state.info.displayServer) && (
              <div className="kv__row">
                <dt>Display server</dt>
                <dd>{formatDisplayServer(state.info.displayServer)}</dd>
              </div>
            )}
            <div className="kv__row">
              <dt>Backend version</dt>
              <dd>{state.info.appVersion}</dd>
            </div>
          </dl>
        )}
      </div>

      <MetricsEngineCard />
      <LiveSampleCard />
      <CpuDetailsCard />
      <GpuDetailsCard />
      <StorageDetailsCard />
      <NetworkDetailsCard />
      <ProcessDetailsCard />
    </section>
  );
}
