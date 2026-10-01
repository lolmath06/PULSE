import { Trans, useTranslation } from 'react-i18next';
import { usePlatformInfo } from '@/hooks/usePlatformInfo';
import { formatDisplayServer, formatPlatformLabel } from '@/utils/format';
import { APP_VERSION } from '@/app/constants';
import { MetricsEngineCard } from '@/components/MetricsEngineCard/MetricsEngineCard';
import { LiveSampleCard } from '@/components/LiveSampleCard/LiveSampleCard';
import { CpuDetailsCard } from '@/components/CpuDetailsCard/CpuDetailsCard';
import { GpuDetailsCard } from '@/components/GpuDetailsCard/GpuDetailsCard';
import { StorageDetailsCard } from '@/components/StorageDetailsCard/StorageDetailsCard';
import { NetworkDetailsCard } from '@/components/NetworkDetailsCard/NetworkDetailsCard';
import { ProcessDetailsCard } from '@/components/ProcessDetailsCard/ProcessDetailsCard';
import { HistoryStatusCard } from '@/components/History/HistoryStatusCard';
import { CpuHistory } from '@/components/History/CpuHistory';
import { MemoryHistory } from '@/components/History/MemoryHistory';
import { ThermalHistory } from '@/components/History/ThermalHistory';
import { GpuHistory } from '@/components/History/GpuHistory';
import { StorageHistory } from '@/components/History/StorageHistory';
import { NetworkHistory } from '@/components/History/NetworkHistory';
import { ProcessHistory } from '@/components/History/ProcessHistory';
import { HomeHero } from '@/components/Home/HomeHero';

export function OverviewPage() {
  const { t } = useTranslation();
  const state = usePlatformInfo();

  return (
    <section className="page page--home">
      <HomeHero>
        <h1 className="page__hero">PULSE</h1>
        <p className="page__subtitle home-hero__tagline">{t('app.tagline')}</p>
        <p className="page__note">{t('overview.version', { version: APP_VERSION })}</p>
      </HomeHero>

      <h2 className="section-title home__details">{t('overview.systemDetails')}</h2>

      <div className="card" aria-label={t('overview.detectedPlatform')}>
        <h2 className="card__title">{t('overview.detectedPlatform')}</h2>

        {state.status === 'loading' && <p className="card__muted">{t('status.querying')}</p>}

        {state.status === 'error' && (
          <p className="card__muted">
            <Trans i18nKey="overview.backendUnavailable" components={{ code: <code /> }} />
          </p>
        )}

        {state.status === 'ready' && (
          <dl className="kv">
            <div className="kv__row">
              <dt>{t('overview.os')}</dt>
              <dd>{formatPlatformLabel(state.info.os, state.info.osVersion)}</dd>
            </div>
            <div className="kv__row">
              <dt>{t('overview.platformLayer')}</dt>
              <dd>{state.info.platform}</dd>
            </div>
            <div className="kv__row">
              <dt>{t('overview.architecture')}</dt>
              <dd>{state.info.arch}</dd>
            </div>
            {formatDisplayServer(state.info.displayServer) && (
              <div className="kv__row">
                <dt>{t('overview.displayServer')}</dt>
                <dd>{formatDisplayServer(state.info.displayServer)}</dd>
              </div>
            )}
            <div className="kv__row">
              <dt>{t('overview.backendVersion')}</dt>
              <dd>{state.info.appVersion}</dd>
            </div>
          </dl>
        )}
      </div>

      <MetricsEngineCard />
      <HistoryStatusCard />
      {/* Each system section: its live summary, then its history. */}
      <LiveSampleCard />
      <MemoryHistory />
      <CpuDetailsCard />
      <CpuHistory />
      <ThermalHistory />
      <GpuDetailsCard />
      <GpuHistory />
      <StorageDetailsCard />
      <StorageHistory />
      <NetworkDetailsCard />
      <NetworkHistory />
      <ProcessDetailsCard />
      <ProcessHistory />
    </section>
  );
}
