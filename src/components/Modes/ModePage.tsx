import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import type { ModeDefinition } from '@/modes/modes';
import { enterMode, modeStyle, updateMode } from '@/design/appearance';
import { updateAppearance, useAppearance } from '@/design/store';
import { useElementSize } from '@/visualization/useElementSize';
import { useScopedLook, withTransition } from '@/design/hooks';
import { PageStyle } from '@/design/LookContext';
import { styleById } from '@/design/styles';
import { setActive } from '@/dashboard/dashboards';
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
      <section className="page page--wide mode-page" aria-label={`${mode.name} mode`}>
        <header className="mode-hero">
          <span className="mode-hero__icon" aria-hidden="true">
            <Icon name={mode.icon} size={30} />
          </span>
          <div className="mode-hero__text">
            <p className="page-header__eyebrow">Mode</p>
            <h1 className="page__title">{mode.name}</h1>
            <p className="mode-hero__tagline">{mode.tagline}</p>
            <p className="page__subtitle">{mode.description}</p>
          </div>
          <div className="mode-hero__actions">
            {active ? (
              <>
                <span className="pill pill--tone-good">
                  <span className="status-dot status-dot--good" aria-hidden="true" /> Active
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
                  Leave mode
                </button>
              </>
            ) : (
              <button
                type="button"
                className="button button--primary button--lg"
                onClick={() => activateMode(mode)}
              >
                <Icon name={mode.icon} /> Enter {mode.name} mode
              </button>
            )}
            <StylePicker
              label={`${mode.name} style`}
              value={settings.styleId}
              followLabel={`${styleById(mode.style).name} (mode default)`}
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

        <section className="mode-section" aria-label={`Overlays for ${mode.name}`}>
          <div className="mode-section__head">
            <h2 className="section-title">Overlay packs</h2>
            <p className="card__muted">
              Composed for {mode.name.toLowerCase()} — each becomes an editable overlay in its own
              window.
            </p>
          </div>
          <PackGallery status={status} only={mode.packs} />
        </section>

        <section className="card mode-behaviour" aria-label="Behaviour">
          <h2 className="card__title">When this mode is on</h2>
          <div className="customize__toggles">
            <Toggle
              text="Lock every overlay on entering (click-through, never focused)"
              checked={settings.lockOverlays}
              onChange={(lockOverlays) =>
                updateAppearance((section) => updateMode(section, mode.id, { lockOverlays }))
              }
            />
            <Toggle
              text="Keep PULSE and its overlays running when the main window closes"
              checked={settings.keepRunning}
              onChange={(keepRunning) =>
                updateAppearance((section) => updateMode(section, mode.id, { keepRunning }))
              }
            />
          </div>
          <ul className="mode-principles">
            {mode.principles.map((principle) => (
              <li key={principle}>
                <Icon name="check" /> {principle}
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
  const widget = useMemo(
    () => ({ ...strip(mode.emphasis, [800, 64], true), id: `w-mode-${mode.id}` }),
    [mode],
  );
  const [ref, size] = useElementSize<HTMLDivElement>();
  const width = Math.max(320, Math.floor(size.width || 800));
  return (
    <div ref={ref} className="mode-live" aria-label={`${mode.name} key metrics`}>
      <WidgetCard widget={widget} width={width} height={72} editing={false} />
    </div>
  );
}

/** The mode's own dashboard, or the template to create it from. */
function ModeDashboard({ mode }: { readonly mode: ModeDefinition }) {
  const navigate = useNavigate();
  const appearance = useAppearance();
  const dashboards = useDashboards();
  const settings = appearance.modes[mode.id];
  const dashboard = dashboards.items.find((item) => item.id === settings.dashboardId);
  const template = findTemplate(mode.template);
  const desktop = useDesktopSettings();

  if (!dashboard) {
    return (
      <section className="mode-section" aria-label={`${mode.name} dashboard`}>
        <div className="mode-section__head">
          <h2 className="section-title">Dashboard</h2>
          <p className="card__muted">
            Start the {mode.name.toLowerCase()} dashboard from its template — then change anything.
          </p>
        </div>
        {template && (
          <div className="mode-template">
            <TemplateCard
              template={template}
              actionLabel={`Create ${mode.name} dashboard`}
              onUse={() => {
                let id: string | null = null;
                updateDashboards((section) => {
                  const result = createDashboardFromTemplate(section, template, `${mode.name}`);
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
    <section className="mode-section" aria-label={`${mode.name} dashboard`}>
      <div className="mode-section__head mode-section__head--row">
        <h2 className="section-title">{dashboard.name}</h2>
        <button
          type="button"
          className="button"
          onClick={() => {
            updateDashboards((section) => setActive(section, dashboard.id));
            void navigate('/dashboard');
          }}
        >
          Edit in Dashboard <Icon name="arrowRight" />
        </button>
      </div>
      <DashboardGrid
        dashboard={{ ...dashboard, locked: true }}
        editing={false}
        actions={NO_ACTIONS}
      />
      {desktop.closeBehavior === 'quit' && settings.keepRunning && (
        <p className="card__note">
          Entering the mode switches “closing the main window” to keep running.
        </p>
      )}
    </section>
  );
}
