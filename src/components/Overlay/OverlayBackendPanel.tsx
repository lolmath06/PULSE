import { useState } from 'react';
import { Trans, useTranslation } from 'react-i18next';
import type { DesktopStatus, GnomeBridgeStatus } from '@/overlay/desktop';
import { refreshGnomeBridge, setGnomeBridgeEnabled } from '@/overlay/desktop';
import {
  HEADLINE_CAPABILITIES,
  bridgeStateLabel,
  bridgeSteps,
  bridgeTone,
  uninstallCommand,
} from '@/overlay/bridgeGuide';
import {
  backendDetail,
  backendLabel,
  bridgeGuidance,
  bridgeSummary,
  capabilityStatusLabel,
} from '@/overlay/desktopText';
import { copyText } from '@/utils/clipboard';

/**
 * The overlay backend this session uses, what it can do, and — on GNOME
 * Wayland — the GNOME bridge's state with the way to set it up.
 *
 * Every desktop action here is the user's: nothing installs itself, and
 * Enable / Disable call GNOME Shell's own API only when pressed.
 */
export function OverlayBackendPanel({
  status,
  error,
  onStatus,
}: {
  readonly status: DesktopStatus | null;
  readonly error: string | null;
  readonly onStatus: (status: DesktopStatus) => void;
}) {
  const { t } = useTranslation();
  if (!status) {
    return (
      <section className="card backend-panel" aria-label={t('overlays.backend.title')}>
        <h2 className="card__title">{t('overlays.backend.title')}</h2>
        <p className="card__muted" title={error ?? undefined}>
          {error ? t('overlays.backend.unavailable') : t('common.checking')}
        </p>
      </section>
    );
  }
  const backend = status.backend;
  const bridge = status.gnomeBridge;
  return (
    <section className="card backend-panel" aria-label={t('overlays.backend.title')}>
      <div className="backend-panel__head">
        <div>
          <h2 className="card__title">{t('overlays.backend.title')}</h2>
          <p className="backend-panel__name">
            {backend ? backendLabel(backend) : t('overlays.backend.desktopWindow')}
          </p>
        </div>
        {backend && (
          <span className={`pill pill--${backend.verification}`}>
            {t(`overlays.verification.${backend.verification}`)}
          </span>
        )}
      </div>
      {backend && <p className="card__muted backend-panel__detail">{backendDetail(backend)}</p>}

      <ul className="capability-chips" aria-label={t('overlays.backend.whatHere')}>
        {HEADLINE_CAPABILITIES.map((key) => {
          const capability = status.capabilities[key];
          return (
            <li
              key={key}
              className={`capability-chip capability-chip--${capability.status}`}
              title={capability.reason}
            >
              <span className="capability-chip__mark" aria-hidden="true">
                {capability.status === 'supported'
                  ? '✓'
                  : capability.status === 'limited'
                    ? '~'
                    : '×'}
              </span>
              {t(`overlays.headline.${key}`)}
              <span className="sr-only">{` — ${capabilityStatusLabel(capability.status)}: ${capability.reason}`}</span>
            </li>
          );
        })}
      </ul>

      {bridge && bridge.state !== 'notApplicable' && (
        <GnomeBridgeSection
          bridge={bridge}
          sourceDir={status.gnomeBridgeSource ?? null}
          onStatus={onStatus}
        />
      )}
    </section>
  );
}

function GnomeBridgeSection({
  bridge,
  sourceDir,
  onStatus,
}: {
  readonly bridge: GnomeBridgeStatus;
  readonly sourceDir: string | null;
  readonly onStatus: (status: DesktopStatus) => void;
}) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const steps = bridgeSteps(bridge, sourceDir);
  const tone = bridgeTone(bridge.state);

  const run = (action: () => Promise<DesktopStatus | null>) => {
    setBusy(true);
    setActionError(null);
    action()
      .then((next) => {
        if (next) onStatus(next);
      })
      .catch((error: unknown) =>
        setActionError(error instanceof Error ? error.message : String(error)),
      )
      .finally(() => setBusy(false));
  };
  const copy = (id: string, text: string) =>
    void copyText(text).then(() => {
      setCopied(id);
      setTimeout(() => setCopied(null), 1600);
    });

  return (
    <div className="bridge" aria-label={t('overlays.bridge.title')}>
      <div className="bridge__head">
        <span className={`status-dot status-dot--${tone}`} aria-hidden="true" />
        <div className="bridge__title">
          <strong>{t('overlays.bridge.title')}</strong>
          <span className={`pill pill--tone-${tone}`}>{bridgeStateLabel(bridge.state)}</span>
          {bridge.connected && (
            <span className="pill pill--tone-good">{t('overlays.bridge.connected')}</span>
          )}
        </div>
        <div className="bridge__actions">
          {bridge.canEnable && (
            <button
              type="button"
              className="button button--primary"
              disabled={busy}
              onClick={() => run(() => setGnomeBridgeEnabled(true))}
            >
              {t('overlays.bridge.enable')}
            </button>
          )}
          {bridge.canDisable && (
            <button
              type="button"
              className="button button--quiet"
              disabled={busy}
              onClick={() => run(() => setGnomeBridgeEnabled(false))}
            >
              {t('overlays.bridge.disable')}
            </button>
          )}
          <button
            type="button"
            className="button button--quiet"
            disabled={busy}
            onClick={() => run(refreshGnomeBridge)}
          >
            {t('common.refresh')}
          </button>
        </div>
      </div>
      <p className="bridge__summary">{bridgeSummary(bridge)}</p>
      {bridgeGuidance(bridge) && (
        <p className="card__note bridge__guidance">{bridgeGuidance(bridge)}</p>
      )}
      {bridge.error && (
        <p className="customize__error" role="alert">
          {bridge.error}
        </p>
      )}
      {actionError && (
        <p className="customize__error" role="alert">
          {actionError}
        </p>
      )}

      <dl className="bridge__versions">
        <div>
          <dt>GNOME Shell</dt>
          <dd>{bridge.shellVersion ?? '—'}</dd>
        </div>
        <div>
          <dt>{t('overlays.bridge.running')}</dt>
          <dd>{bridge.runningVersion !== null ? `v${bridge.runningVersion}` : '—'}</dd>
        </div>
        <div>
          <dt>{t('overlays.bridge.installed')}</dt>
          <dd>{bridge.installedVersion !== null ? `v${bridge.installedVersion}` : '—'}</dd>
        </div>
        <div>
          <dt>{t('overlays.bridge.ships')}</dt>
          <dd>v{bridge.bundledVersion}</dd>
        </div>
      </dl>

      {steps.length > 0 && (
        <ol className="bridge__steps" aria-label={t('overlays.bridge.setUp')}>
          {steps.map((step, index) => (
            <li key={step.id} className={`bridge__step bridge__step--${step.state}`}>
              <span className="bridge__step-mark" aria-hidden="true">
                {step.state === 'done' ? '✓' : index + 1}
              </span>
              <div className="bridge__step-body">
                <span className="bridge__step-title">{step.title}</span>
                <span className="bridge__step-detail">{step.detail}</span>
                {step.command && step.state !== 'done' && (
                  <span className="command">
                    <code className="command__text">{step.command}</code>
                    <button
                      type="button"
                      className="button button--quiet command__copy"
                      onClick={() => copy(step.id, step.command!)}
                    >
                      {copied === step.id ? t('common.copied') : t('common.copy')}
                    </button>
                  </span>
                )}
              </div>
            </li>
          ))}
        </ol>
      )}

      <details className="bridge__details">
        <summary>{t('overlays.bridge.details.summary')}</summary>
        <div className="bridge__details-body">
          {(['unlocks', 'safety', 'troubleshooting', 'limits'] as const).map((part) => (
            <p key={part}>
              <Trans
                i18nKey={`overlays.bridge.details.${part}`}
                components={{ strong: <strong />, code: <code />, em: <em /> }}
              />
            </p>
          ))}
          <p className="command">
            <code className="command__text">{uninstallCommand(sourceDir)}</code>
            <button
              type="button"
              className="button button--quiet command__copy"
              onClick={() => copy('uninstall', uninstallCommand(sourceDir))}
            >
              {copied === 'uninstall' ? t('common.copied') : t('overlays.bridge.copyUninstall')}
            </button>
          </p>
        </div>
      </details>
    </div>
  );
}
