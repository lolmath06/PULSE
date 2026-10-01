import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
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
import { backendLabel } from '@/overlay/desktopText';
import { LanguageSelect } from '@/components/Language/LanguageSelect';
import { Sheet } from '@/components/Sheet/Sheet';
import { StyleCard } from '@/components/Appearance/StyleCard';
import { Icon } from '@/components/Icon';

/**
 * The first-run welcome: a style, a mode, a starter dashboard and overlay —
 * one sheet, every choice optional, all of it changeable later. It also says
 * plainly which overlay backend this desktop gives, GNOME bridge included.
 */
export function Welcome({ onDone }: { readonly onDone: () => void }) {
  const { t } = useTranslation();
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
      title={t('welcome.title')}
      subtitle={t('welcome.subtitle')}
      onClose={skip}
      label={t('welcome.label')}
      footer={
        <>
          <button type="button" className="button button--quiet" onClick={skip}>
            {t('welcome.skip')}
          </button>
          <button type="button" className="button button--primary button--lg" onClick={start}>
            {t('welcome.start')} <Icon name="arrowRight" />
          </button>
        </>
      }
    >
      <section className="welcome__step welcome__language" aria-label={t('language.label')}>
        <LanguageSelect />
      </section>

      <section className="welcome__step" aria-label={t('welcome.modeLabel')}>
        <h3 className="welcome__heading">
          <span>1</span> {t('welcome.howUse')}
        </h3>
        <div className="welcome__modes" role="group" aria-label={t('welcome.modeLabel')}>
          {MODES.filter((m) => m.id !== 'mini').map((m) => (
            <button
              key={m.id}
              type="button"
              className={`welcome__mode${mode === m.id ? ' welcome__mode--active' : ''}`}
              aria-pressed={mode === m.id}
              onClick={() => pickMode(m.id)}
            >
              <Icon name={m.icon} size={22} />
              <strong>{t(`modes.${m.id}.name`)}</strong>
              <span>{t(`modes.${m.id}.tagline`)}</span>
            </button>
          ))}
          <button
            type="button"
            className={`welcome__mode${mode === null ? ' welcome__mode--active' : ''}`}
            aria-pressed={mode === null}
            onClick={() => pickMode(null)}
          >
            <Icon name="overview" size={22} />
            <strong>{t('welcome.justMonitor')}</strong>
            <span>{t('welcome.justMonitorHint')}</span>
          </button>
        </div>
      </section>

      <section className="welcome__step" aria-label={t('nav.style')}>
        <h3 className="welcome__heading">
          <span>2</span> {t('welcome.pickStyle')}
        </h3>
        <div
          className="style-gallery style-gallery--compact"
          role="group"
          aria-label={t('welcome.styles')}
        >
          {looks.map(({ style, look }) => (
            <StyleCard
              key={style.id}
              look={look}
              name={t(`styles.${style.id}.name`)}
              tagline={t(`styles.${style.id}.tagline`)}
              active={styleId === style.id}
              onSelect={() => setStyleId(style.id)}
            />
          ))}
        </div>
      </section>

      <section className="welcome__step welcome__starters" aria-label={t('welcome.starters')}>
        <h3 className="welcome__heading">
          <span>3</span> {t('welcome.startWith')}
        </h3>
        <label className="welcome__field">
          {t('welcome.dashboard')}
          <select
            className="history-panel__select"
            aria-label={t('welcome.starterDashboard')}
            value={template}
            onChange={(event) => setTemplate(event.target.value)}
          >
            <option value="keep">{t('welcome.keepDashboards')}</option>
            {DASHBOARD_TEMPLATES.map((template) => (
              <option key={template.id} value={template.id}>
                {t(`presets.templates.${template.id}.name`)}
              </option>
            ))}
          </select>
        </label>
        <label className="welcome__field">
          {t('welcome.overlay')}
          <select
            className="history-panel__select"
            aria-label={t('welcome.starterOverlay')}
            value={pack}
            onChange={(event) => setPack(event.target.value)}
          >
            <option value="none">{t('welcome.noOverlay')}</option>
            {OVERLAY_PACKS.map((p) => (
              <option key={p.id} value={p.id}>
                {t(`presets.packs.${p.id}.name`)}
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
            {t('welcome.overlaysHere')} <strong>{backendLabel(backend)}</strong>
            {bridge && bridge.state !== 'notApplicable' && bridge.state !== 'active' && (
              <>
                {' '}
                {t('welcome.bridgeNotReady', {
                  state: t(`overlays.bridge.states.${bridge.state}`),
                })}
              </>
            )}
            {bridge?.state === 'active' && <> {t('welcome.bridgeActive')}</>}
          </>
        ) : (
          t('welcome.noBackend')
        )}
      </p>
    </Sheet>
  );
}
