import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { findMode } from '@/modes/modes';
import { MINI_LAYOUTS } from '@/modes/miniLayouts';
import { enterMode, modeStyle, setMini, updateMode } from '@/design/appearance';
import { resolveLook } from '@/design/look';
import { StyleScope, PageStyle } from '@/design/LookContext';
import { withTransition } from '@/design/hooks';
import { updateAppearance, useAppearance } from '@/design/store';
import { useDashboards } from '@/dashboard/store';
import { dashboardName } from '@/dashboard/dashboards';
import { openMiniWindow } from '@/overlay/desktop';
import { StylePicker } from '@/components/Appearance/StylePicker';
import { MiniView } from '@/components/Modes/MiniView';
import { PackGallery } from '@/components/Presets/PackGallery';
import { Icon } from '@/components/Icon';

/**
 * The Mini mode: choose what the small Mini window shows and how it looks,
 * see it live, open it.
 */
export function MiniModePage() {
  const { t } = useTranslation();
  const mode = findMode('mini')!;
  const styleName = (id: string) => t(`styles.${id}.name`);
  const appearance = useAppearance();
  const dashboards = useDashboards();
  const styleId = appearance.mini.styleId ?? modeStyle(appearance, 'mini');
  const look = useMemo(() => resolveLook(styleId, appearance.custom), [styleId, appearance.custom]);
  const active = appearance.activeMode === 'mini';
  const source = appearance.mini.source;

  return (
    <PageStyle styleId={modeStyle(appearance, 'mini')}>
      <section
        className="page page--wide mode-page"
        aria-label={t('modes.page.aria', { mode: t('modes.mini.name') })}
      >
        <header className="mode-hero">
          <span className="mode-hero__icon" aria-hidden="true">
            <Icon name="mini" size={30} />
          </span>
          <div className="mode-hero__text">
            <p className="page-header__eyebrow">{t('modes.page.eyebrow')}</p>
            <h1 className="page__title">{t('modes.mini.name')}</h1>
            <p className="mode-hero__tagline">{t('modes.mini.tagline')}</p>
            <p className="page__subtitle">{t('modes.miniPage.subtitle')}</p>
          </div>
          <div className="mode-hero__actions">
            <button
              type="button"
              className="button button--primary button--lg"
              onClick={() => void openMiniWindow().catch(() => undefined)}
            >
              <Icon name="mini" /> {t('overlays.page.openMini')}
            </button>
            {active ? (
              <button
                type="button"
                className="button button--quiet"
                onClick={() => withTransition(() => updateAppearance((s) => enterMode(s, null)))}
              >
                {t('appearance.leaveMode', { mode: t('modes.mini.name') })}
              </button>
            ) : (
              <button
                type="button"
                className="button button--quiet"
                onClick={() => withTransition(() => updateAppearance((s) => enterMode(s, 'mini')))}
              >
                {t('modes.miniPage.useHere')}
              </button>
            )}
          </div>
        </header>

        <div className="mini-studio">
          <div className="mini-studio__choices">
            <section className="card" aria-label={t('modes.miniPage.layouts')}>
              <h2 className="card__title">{t('modes.miniPage.whatShows')}</h2>
              <div className="mini-layouts" role="group" aria-label={t('modes.miniPage.layout')}>
                {MINI_LAYOUTS.map((layout) => {
                  const chosen = source.kind === 'layout' && source.id === layout.id;
                  return (
                    <button
                      key={layout.id}
                      type="button"
                      className={`mini-layout${chosen ? ' mini-layout--active' : ''}`}
                      aria-pressed={chosen}
                      onClick={() =>
                        updateAppearance((section) =>
                          setMini(section, { source: { kind: 'layout', id: layout.id } }),
                        )
                      }
                    >
                      <span className="mini-layout__name">
                        {t(`modes.miniLayouts.${layout.id}.name`)}
                      </span>
                      <span className="mini-layout__description">
                        {t(`modes.miniLayouts.${layout.id}.description`)}
                      </span>
                    </button>
                  );
                })}
              </div>
              <label className="style-picker mini-studio__dashboard">
                {t('modes.miniPage.orDashboard')}
                <select
                  className="history-panel__select"
                  aria-label={t('modes.miniPage.dashboardShown')}
                  value={source.kind === 'dashboard' ? source.id : ''}
                  onChange={(event) =>
                    event.target.value &&
                    updateAppearance((section) =>
                      setMini(section, { source: { kind: 'dashboard', id: event.target.value } }),
                    )
                  }
                >
                  <option value="">—</option>
                  {dashboards.items.map((item) => (
                    <option key={item.id} value={item.id}>
                      {dashboardName(item)}
                    </option>
                  ))}
                </select>
              </label>
            </section>
            <section className="card" aria-label={t('modes.miniPage.style')}>
              <h2 className="card__title">{t('modes.miniPage.howLooks')}</h2>
              <div className="customize__row">
                <StylePicker
                  label={t('modes.miniPage.style')}
                  value={appearance.mini.styleId}
                  followLabel={t('modes.miniPage.followMode', {
                    name: styleName(modeStyle(appearance, 'mini')),
                  })}
                  onChange={(value) =>
                    updateAppearance((section) => setMini(section, { styleId: value }))
                  }
                />
                <StylePicker
                  label={t('modes.miniPage.modeStyle')}
                  value={appearance.modes.mini.styleId}
                  followLabel={t('modes.page.modeDefault', { name: styleName(mode.style) })}
                  onChange={(value) =>
                    updateAppearance((section) => updateMode(section, 'mini', { styleId: value }))
                  }
                />
              </div>
              <p className="card__note">{t('modes.miniPage.followsAtOnce')}</p>
            </section>
          </div>
          <aside className="mini-studio__preview" aria-label={t('modes.miniPage.preview')}>
            <p className="appearance__preview-label">
              <Icon name="sparkles" /> {t('modes.miniPage.live', { name: styleName(styleId) })}
            </p>
            <StyleScope look={look} className="mini-frame">
              <span className="mini-frame__bar" aria-hidden="true">
                <span />
                PULSE Mini
              </span>
              <div className="mini-frame__body">
                <MiniView settings={appearance.mini} />
              </div>
            </StyleScope>
          </aside>
        </div>

        <section
          className="mode-section"
          aria-label={t('modes.page.overlaysFor', { mode: t('modes.mini.name') })}
        >
          <div className="mode-section__head">
            <h2 className="section-title">{t('modes.miniPage.tinyOverlays')}</h2>
            <p className="card__muted">{t('modes.miniPage.tinyOverlaysHint')}</p>
          </div>
          <PackGallery status={null} only={mode.packs} />
        </section>
      </section>
    </PageStyle>
  );
}
