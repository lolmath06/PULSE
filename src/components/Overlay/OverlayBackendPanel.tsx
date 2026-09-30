import { useState } from 'react';
import type { DesktopStatus, GnomeBridgeStatus } from '@/overlay/desktop';
import { refreshGnomeBridge, setGnomeBridgeEnabled } from '@/overlay/desktop';
import {
  BRIDGE_STATE_LABELS,
  HEADLINE_CAPABILITIES,
  bridgeSteps,
  bridgeTone,
  uninstallCommand,
} from '@/overlay/bridgeGuide';
import { copyText } from '@/utils/clipboard';

const VERIFICATION_LABELS = {
  physicallyVerified: 'Verified on real hardware',
  implemented: 'Implemented · not yet verified on hardware',
  bestEffort: 'Platform-dependent',
  unsupported: 'Unavailable',
} as const;

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
  if (!status) {
    return (
      <section className="card backend-panel" aria-label="Overlay backend">
        <h2 className="card__title">Overlay backend</h2>
        <p className="card__muted" title={error ?? undefined}>
          {error ? 'Backend unavailable. Run PULSE with pnpm app:dev.' : 'Checking…'}
        </p>
      </section>
    );
  }
  const backend = status.backend;
  const bridge = status.gnomeBridge;
  return (
    <section className="card backend-panel" aria-label="Overlay backend">
      <div className="backend-panel__head">
        <div>
          <h2 className="card__title">Overlay backend</h2>
          <p className="backend-panel__name">{backend?.label ?? 'Desktop window'}</p>
        </div>
        {backend && (
          <span className={`pill pill--${backend.verification}`}>
            {VERIFICATION_LABELS[backend.verification]}
          </span>
        )}
      </div>
      {backend && <p className="card__muted backend-panel__detail">{backend.detail}</p>}

      <ul className="capability-chips" aria-label="What overlays can do here">
        {HEADLINE_CAPABILITIES.map(({ key, label }) => {
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
              {label}
              <span className="sr-only">{` — ${capability.status}: ${capability.reason}`}</span>
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
    <div className="bridge" aria-label="GNOME bridge">
      <div className="bridge__head">
        <span className={`status-dot status-dot--${tone}`} aria-hidden="true" />
        <div className="bridge__title">
          <strong>GNOME bridge</strong>
          <span className={`pill pill--tone-${tone}`}>{BRIDGE_STATE_LABELS[bridge.state]}</span>
          {bridge.connected && <span className="pill pill--tone-good">Connected</span>}
        </div>
        <div className="bridge__actions">
          {bridge.canEnable && (
            <button
              type="button"
              className="button button--primary"
              disabled={busy}
              onClick={() => run(() => setGnomeBridgeEnabled(true))}
            >
              Enable
            </button>
          )}
          {bridge.canDisable && (
            <button
              type="button"
              className="button button--quiet"
              disabled={busy}
              onClick={() => run(() => setGnomeBridgeEnabled(false))}
            >
              Disable
            </button>
          )}
          <button
            type="button"
            className="button button--quiet"
            disabled={busy}
            onClick={() => run(refreshGnomeBridge)}
          >
            Refresh
          </button>
        </div>
      </div>
      <p className="bridge__summary">{bridge.summary}</p>
      {bridge.guidance && <p className="card__note bridge__guidance">{bridge.guidance}</p>}
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
          <dt>Running</dt>
          <dd>{bridge.runningVersion !== null ? `v${bridge.runningVersion}` : '—'}</dd>
        </div>
        <div>
          <dt>Installed</dt>
          <dd>{bridge.installedVersion !== null ? `v${bridge.installedVersion}` : '—'}</dd>
        </div>
        <div>
          <dt>Ships with PULSE</dt>
          <dd>v{bridge.bundledVersion}</dd>
        </div>
      </dl>

      {steps.length > 0 && (
        <ol className="bridge__steps" aria-label="Set up the GNOME bridge">
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
                      {copied === step.id ? 'Copied' : 'Copy'}
                    </button>
                  </span>
                )}
              </div>
            </li>
          ))}
        </ol>
      )}

      <details className="bridge__details">
        <summary>What it does · troubleshooting · limits</summary>
        <div className="bridge__details-body">
          <p>
            <strong>Unlocks:</strong> overlays stay above the focused application (Mutter{' '}
            <code>make_above</code>), and the overlay shortcut works whichever application has focus
            — GNOME 45 has no global-shortcut portal. Click-through when locked is PULSE&apos;s own
            and works with or without the bridge.
          </p>
          <p>
            <strong>Safety:</strong> it only touches windows owned by the process holding
            PULSE&apos;s bus name <em>and</em> titled exactly{' '}
            <code>PULSE Overlay :: &lt;id&gt;</code>. No polling, no input interception, no files,
            no network.
          </p>
          <p>
            <strong>Troubleshooting:</strong> after installing or updating, log out and back in —
            GNOME loads extension code only at login. Its log:{' '}
            <code>
              journalctl --user -b /usr/bin/gnome-shell | grep &quot;PULSE overlay bridge&quot;
            </code>
            . If GNOME Shell ever misbehaves at login, turn user extensions off from another session
            with <code>gsettings set org.gnome.shell disable-user-extensions true</code>.
          </p>
          <p>
            <strong>Limits:</strong> GNOME still places Wayland windows itself, so positions and
            multi-monitor placement stay the compositor&apos;s; drag an overlay once in Edit mode.
            Verified on GNOME 45 only.
          </p>
          <p className="command">
            <code className="command__text">{uninstallCommand(sourceDir)}</code>
            <button
              type="button"
              className="button button--quiet command__copy"
              onClick={() => copy('uninstall', uninstallCommand(sourceDir))}
            >
              {copied === 'uninstall' ? 'Copied' : 'Copy uninstall'}
            </button>
          </p>
        </div>
      </details>
    </div>
  );
}
