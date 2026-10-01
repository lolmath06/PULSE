import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import type { ModeDefinition } from '@/modes/modes';
import { enterMode, modeStyle, updateMode } from '@/design/appearance';
import { updateAppearance, useAppearance } from '@/design/store';
import { useElementSize } from '@/visualization/useElementSize';
import { useScopedLook, withTransition } from '@/design/hooks';
import { PageStyle } from '@/design/LookContext';
import { dashboardName, setActive } from '@/dashboard/dashboards';
import { updateDashboards, useDashboards } from '@/dashboard/store';
import { createDashboardFromTemplate, findTemplate } from '@/presets/dashboardTemplates';
import { strip } from '@/presets/widgets';
import type { DesktopStatus } from '@/overlay/desktop';
import { getDesktopStatus } from '@/overlay/desktop';
import { useDesktopSettings } from '@/overlay/settings';
import { WidgetCard } from '@/components/Dashboard/WidgetCard';
import { DashboardGrid } from '@/components/Dashboard/DashboardGrid';
import { StylePicker } from '@/components/Appearance/StylePicker';
import { PackGallery } from '@/components/Presets/PackGallery';
import { TemplateCard } from '@/components/Presets/TemplateGallery';
import { Toggle } from '@/visualization/CustomizePanel';
import { Icon } from '@/components/Icon';
import { activateMode } from '@/modes/activate';

const NO_ACTIONS = {
  move: () => undefined,
  resize: () => undefined,
  remove: () => undefined,
  duplicate: () => undefined,
  customize: () => undefined,
};

/**
 * A mode's page: what the mode is, its live key metrics, its dashboard, the
 * overlay packs that suit it and how it behaves — in the mode's own style.
 */
export function ModePage({ mode }: { readonly mode: ModeDefinition }) {
  const { t } = useTranslation();
  const name = t(`modes.${mode.id}.name`);
  const appearance = useAppearance();
  const settings = appearance.modes[mode.id];
  const styleId = modeStyle(appearance, mode.id);
  const active = appearance.activeMode === mode.id;
  const look = useScopedLook(styleId);
  const [status, setStatus] = useState<DesktopStatus | null>(null);
  useEffect(() => {
    getDesktopStatus()
      .then(setStatus)
      .catch(() => undefined);
  }, []);

  return (
    <PageStyle styleId={styleId}>
      <section
        className="page page--wide mode-page"
        aria-label={t('modes.page.aria', { mode: name })}
      >
        <header className="mode-hero">
          <span className="mode-hero__icon" aria-hidden="true">
            <Icon name={mode.icon} size={30} />
          </span>
          <div className="mode-hero__text">
            <p className="page-header__eyebrow">{t('modes.page.eyebrow')}</p>
            <h1 className="page__title">{name}</h1>
            <p className="mode-hero__tagline">{t(`modes.${mode.id}.tagline`)}</p>
            <p className="page__subtitle">{t(`modes.${mode.id}.description`)}</p>
          </div>
          <div className="mode-hero__actions">
            {active ? (
              <>
                <span className="pill pill--tone-good">
                  <span className="status-dot status-dot--good" aria-hidden="true" />{' '}
                  {t('common.active')}
                </span>
                <button
                  type="button"
                  className="button button--quiet"
                  onClick={() =>
                    withTransition(
                      () => updateAppearance((section) => enterMode(section, null)),
                      look,
                    )
                  }
                >
                  {t('modes.page.leave')}
                </button>
              </>
            ) : (
              <button
                type="button"
                className="button button--primary button--lg"
                onClick={() => activateMode(mode)}
              >
                <Icon name={mode.icon} /> {t('modes.page.enter', { mode: name })}
              </button>
            )}
            <StylePicker
              label={t('modes.page.style', { mode: name })}
              value={settings.styleId}
              followLabel={t('modes.page.modeDefault', { name: t(`styles.${mode.style}.name`) })}
              onChange={(value) =>
                withTransition(
                  () =>
                    updateAppearance((section) => updateMode(section, mode.id, { styleId: value })),
                  look,
                )
              }
            />
          </div>
        </header>

        <LiveEmphasis mode={mode} />

        <ModeDashboard mode={mode} />

        <section className="mode-section" aria-label={t('modes.page.overlaysFor', { mode: name })}>
          <div className="mode-section__head">
            <h2 className="section-title">{t('overlays.page.packs')}</h2>
            <p className="card__muted">{t('modes.page.packsHint', { mode: name })}</p>
          </div>
          <PackGallery status={status} only={mode.packs} />
        </section>

        <section className="card mode-behaviour" aria-label={t('modes.page.behaviour')}>
          <h2 className="card__title">{t('modes.page.whenOn')}</h2>
          <div className="customize__toggles">
            <Toggle
              text={t('modes.page.lockOverlays')}
              checked={settings.lockOverlays}
              onChange={(lockOverlays) =>
                updateAppearance((section) => updateMode(section, mode.id, { lockOverlays }))
              }
            />
            <Toggle
              text={t('modes.page.keepRunning')}
              checked={settings.keepRunning}
              onChange={(keepRunning) =>
                updateAppearance((section) => updateMode(section, mode.id, { keepRunning }))
              }
            />
          </div>
          <ul className="mode-principles">
            {mode.principles.map((principle) => (
              <li key={principle}>
                <Icon name="check" /> {t(`modes.${mode.id}.principles.${principle}`)}
              </li>
            ))}
          </ul>
        </section>
      </section>
    </PageStyle>
  );
}

/** The mode's key metrics, live, in one strip. */
function LiveEmphasis({ mode }: { readonly mode: ModeDefinition }) {
  const { t } = useTranslation();
  const widget = useMemo(
    () => ({ ...strip(mode.emphasis, [800, 64], true), id: `w-mode-${mode.id}` }),
    [mode],
  );
  const [ref, size] = useElementSize<HTMLDivElement>();
  const width = Math.max(320, Math.floor(size.width || 800));
  return (
    <div
      ref={ref}
      className="mode-live"
      aria-label={t('modes.page.keyMetrics', { mode: t(`modes.${mode.id}.name`) })}
    >
      <WidgetCard widget={widget} width={width} height={72} editing={false} />
    </div>
  );
}

/** The mode's own dashboard, or the template to create it from. */
function ModeDashboard({ mode }: { readonly mode: ModeDefinition }) {
  const { t } = useTranslation();
  const name = t(`modes.${mode.id}.name`);
  const navigate = useNavigate();
  const appearance = useAppearance();
  const dashboards = useDashboards();
  const settings = appearance.modes[mode.id];
  const dashboard = dashboards.items.find((item) => item.id === settings.dashboardId);
  const template = findTemplate(mode.template);
  const desktop = useDesktopSettings();

  if (!dashboard) {
    return (
      <section className="mode-section" aria-label={t('modes.page.dashboardAria', { mode: name })}>
        <div className="mode-section__head">
          <h2 className="section-title">{t('nav.routes.dashboard.label')}</h2>
          <p className="card__muted">{t('modes.page.startDashboard', { mode: name })}</p>
        </div>
        {template && (
          <div className="mode-template">
            <TemplateCard
              template={template}
              actionLabel={t('modes.page.createDashboard', { mode: name })}
              onUse={() => {
                let id: string | null = null;
                updateDashboards((section) => {
                  // Named after the mode, by key: the name follows the language.
                  const result = createDashboardFromTemplate(section, template, {
                    key: `modes.${mode.id}.name`,
                  });
                  id = result.id;
                  return result.section;
                });
                if (id) {
                  const created = id;
                  updateAppearance((section) =>
                    updateMode(section, mode.id, { dashboardId: created }),
                  );
                }
              }}
            />
          </div>
        )}
      </section>
    );
  }

  return (
    <section className="mode-section" aria-label={t('modes.page.dashboardAria', { mode: name })}>
      <div className="mode-section__head mode-section__head--row">
        <h2 className="section-title">{dashboardName(dashboard)}</h2>
        <button
          type="button"
          className="button"
          onClick={() => {
            updateDashboards((section) => setActive(section, dashboard.id));
            void navigate('/dashboard');
          }}
        >
          {t('modes.page.editInDashboard')} <Icon name="arrowRight" />
        </button>
      </div>
      <DashboardGrid
        dashboard={{ ...dashboard, locked: true }}
        editing={false}
        actions={NO_ACTIONS}
      />
      {desktop.closeBehavior === 'quit' && settings.keepRunning && (
        <p className="card__note">{t('modes.page.switchesKeepRunning')}</p>
      )}
    </section>
  );
}
