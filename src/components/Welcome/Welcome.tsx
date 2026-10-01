import { useEffect, useMemo, useState } from 'react';
import type { StyleId } from '@/design/styles';
import { STYLES } from '@/design/styles';
import { completeSetup, enterMode, setStyle, updateMode } from '@/design/appearance';
import { resolveLook } from '@/design/look';
import { withTransition } from '@/design/hooks';
import { readAppearance, updateAppearance } from '@/design/store';
import type { ModeId } from '@/modes/modes';
import { MODES, findMode } from '@/modes/modes';
import {
  DASHBOARD_TEMPLATES,
  createDashboardFromTemplate,
  findTemplate,
} from '@/presets/dashboardTemplates';
import { OVERLAY_PACKS, createOverlayFromPack, findPack } from '@/presets/overlayPacks';
import { updateDashboards } from '@/dashboard/store';
import { updateOverlays } from '@/overlay/store';
import { primaryScreen } from '@/overlay/screen';
import type { DesktopStatus } from '@/overlay/desktop';
import { getDesktopStatus } from '@/overlay/desktop';
import { bridgeTone } from '@/overlay/bridgeGuide';
import { Sheet } from '@/components/Sheet/Sheet';
import { StyleCard } from '@/components/Appearance/StyleCard';
import { Icon } from '@/components/Icon';

/**
 * The first-run welcome: a style, a mode, a starter dashboard and overlay —
 * one sheet, every choice optional, all of it changeable later. It also says
 * plainly which overlay backend this desktop gives, GNOME bridge included.
 */
export function Welcome({ onDone }: { readonly onDone: () => void }) {
  const [styleId, setStyleId] = useState<StyleId>('clean');
  const [mode, setMode] = useState<ModeId | null>(null);
  const [template, setTemplate] = useState<string>('keep');
  const [pack, setPack] = useState<string>('none');
  const [status, setStatus] = useState<DesktopStatus | null>(null);
  const looks = useMemo(() => STYLES.map((style) => ({ style, look: resolveLook(style.id) })), []);

  useEffect(() => {
    getDesktopStatus()
      .then(setStatus)
      .catch(() => undefined);
  }, []);

  const pickMode = (next: ModeId | null) => {
    setMode(next);
    const definition = findMode(next);
    if (definition) {
      setStyleId(definition.style);
      setTemplate(definition.template);
      setPack(definition.packs[0] ?? 'none');
    }
  };

  const skip = () => {
    updateAppearance(completeSetup);
    onDone();
  };

  const start = () => {
    let dashboardId: string | null = null;
    const chosen = findTemplate(template);
    if (chosen) {
      updateDashboards((section) => {
        const result = createDashboardFromTemplate(section, chosen);
        dashboardId = result.id;
        return result.section;
      });
    }
    withTransition(() =>
      updateAppearance((section) => {
        let next = completeSetup(setStyle(section, styleId));
        if (mode) {
          next = enterMode(next, mode);
          next = updateMode(next, mode, { styleId, ...(dashboardId ? { dashboardId } : {}) });
        }
        return next;
      }),
    );
    const starter = findPack(pack);
    if (starter) {
      void primaryScreen().then((screen) =>
        updateOverlays(
          (section) =>
            createOverlayFromPack(section, starter, screen, readAppearance().custom).section,
        ),
      );
    }
    onDone();
  };

  const backend = status?.backend;
  const bridge = status?.gnomeBridge;

  return (
    <Sheet
      title="Welcome to PULSE"
      subtitle="Choose a starting point. Every choice can be changed later — in Appearance, the modes, Dashboard and Overlays."
      onClose={skip}
      label="Welcome"
      footer={
        <>
          <button type="button" className="button button--quiet" onClick={skip}>
            Skip
          </button>
          <button type="button" className="button button--primary button--lg" onClick={start}>
            Start PULSE <Icon name="arrowRight" />
          </button>
        </>
      }
    >
      <section className="welcome__step" aria-label="Mode">
        <h3 className="welcome__heading">
          <span>1</span> How will you use it?
        </h3>
        <div className="welcome__modes" role="group" aria-label="Mode">
          {MODES.filter((m) => m.id !== 'mini').map((m) => (
            <button
              key={m.id}
              type="button"
              className={`welcome__mode${mode === m.id ? ' welcome__mode--active' : ''}`}
              aria-pressed={mode === m.id}
              onClick={() => pickMode(m.id)}
            >
              <Icon name={m.icon} size={22} />
              <strong>{m.name}</strong>
              <span>{m.tagline}</span>
            </button>
          ))}
          <button
            type="button"
            className={`welcome__mode${mode === null ? ' welcome__mode--active' : ''}`}
            aria-pressed={mode === null}
            onClick={() => pickMode(null)}
          >
            <Icon name="overview" size={22} />
            <strong>Just monitor</strong>
            <span>No mode — a style and the dashboards.</span>
          </button>
        </div>
      </section>

      <section className="welcome__step" aria-label="Style">
        <h3 className="welcome__heading">
          <span>2</span> Pick a style
        </h3>
        <div
          className="style-gallery style-gallery--compact"
          role="group"
          aria-label="Welcome styles"
        >
          {looks.map(({ style, look }) => (
            <StyleCard
              key={style.id}
              look={look}
              name={style.name}
              tagline={style.tagline}
              active={styleId === style.id}
              onSelect={() => setStyleId(style.id)}
            />
          ))}
        </div>
      </section>

      <section className="welcome__step welcome__starters" aria-label="Starters">
        <h3 className="welcome__heading">
          <span>3</span> Start with
        </h3>
        <label className="welcome__field">
          Dashboard
          <select
            className="history-panel__select"
            aria-label="Starter dashboard"
            value={template}
            onChange={(event) => setTemplate(event.target.value)}
          >
            <option value="keep">Keep my current dashboards</option>
            {DASHBOARD_TEMPLATES.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
          </select>
        </label>
        <label className="welcome__field">
          Overlay
          <select
            className="history-panel__select"
            aria-label="Starter overlay"
            value={pack}
            onChange={(event) => setPack(event.target.value)}
          >
            <option value="none">No overlay for now</option>
            {OVERLAY_PACKS.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
      </section>

      <p className="welcome__desktop" role="status">
        <span
          className={`status-dot status-dot--${bridge && bridge.state !== 'notApplicable' ? bridgeTone(bridge.state) : backend ? 'good' : 'off'}`}
          aria-hidden="true"
        />
        {backend ? (
          <>
            Overlays here: <strong>{backend.label}</strong>
            {bridge && bridge.state !== 'notApplicable' && bridge.state !== 'active' && (
              <> — the GNOME bridge is {bridge.summary.toLowerCase()}; see Overlays to set it up.</>
            )}
            {bridge?.state === 'active' && <> — overlays stay above other windows.</>}
          </>
        ) : (
          'Overlay capabilities are shown in Overlays once the backend answers.'
        )}
      </p>
    </Sheet>
  );
}
