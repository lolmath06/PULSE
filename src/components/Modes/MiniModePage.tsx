import { useMemo } from 'react';
import { findMode } from '@/modes/modes';
import { MINI_LAYOUTS } from '@/modes/miniLayouts';
import { enterMode, modeStyle, setMini, updateMode } from '@/design/appearance';
import { resolveLook } from '@/design/look';
import { StyleScope, PageStyle } from '@/design/LookContext';
import { withTransition } from '@/design/hooks';
import { styleById } from '@/design/styles';
import { updateAppearance, useAppearance } from '@/design/store';
import { useDashboards } from '@/dashboard/store';
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
  const mode = findMode('mini')!;
  const appearance = useAppearance();
  const dashboards = useDashboards();
  const styleId = appearance.mini.styleId ?? modeStyle(appearance, 'mini');
  const look = useMemo(() => resolveLook(styleId, appearance.custom), [styleId, appearance.custom]);
  const active = appearance.activeMode === 'mini';
  const source = appearance.mini.source;

  return (
    <PageStyle styleId={modeStyle(appearance, 'mini')}>
      <section className="page page--wide mode-page" aria-label="Mini mode">
        <header className="mode-hero">
          <span className="mode-hero__icon" aria-hidden="true">
            <Icon name="mini" size={30} />
          </span>
          <div className="mode-hero__text">
            <p className="page-header__eyebrow">Mode</p>
            <h1 className="page__title">Mini</h1>
            <p className="mode-hero__tagline">{mode.tagline}</p>
            <p className="page__subtitle">
              A small, normal PULSE window. Mini is interactive and behaves like any other window;
              for something that stays on top and lets clicks through, add an overlay.
            </p>
          </div>
          <div className="mode-hero__actions">
            <button
              type="button"
              className="button button--primary button--lg"
              onClick={() => void openMiniWindow().catch(() => undefined)}
            >
              <Icon name="mini" /> Open Mini window
            </button>
            {active ? (
              <button
                type="button"
                className="button button--quiet"
                onClick={() => withTransition(() => updateAppearance((s) => enterMode(s, null)))}
              >
                Leave Mini mode
              </button>
            ) : (
              <button
                type="button"
                className="button button--quiet"
                onClick={() => withTransition(() => updateAppearance((s) => enterMode(s, 'mini')))}
              >
                Use Mini mode here too
              </button>
            )}
          </div>
        </header>

        <div className="mini-studio">
          <div className="mini-studio__choices">
            <section className="card" aria-label="Mini layouts">
              <h2 className="card__title">What Mini shows</h2>
              <div className="mini-layouts" role="group" aria-label="Mini layout">
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
                      <span className="mini-layout__name">{layout.name}</span>
                      <span className="mini-layout__description">{layout.description}</span>
                    </button>
                  );
                })}
              </div>
              <label className="style-picker mini-studio__dashboard">
                Or a dashboard
                <select
                  className="history-panel__select"
                  aria-label="Dashboard shown in Mini"
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
                      {item.name}
                    </option>
                  ))}
                </select>
              </label>
            </section>
            <section className="card" aria-label="Mini style">
              <h2 className="card__title">How Mini looks</h2>
              <div className="customize__row">
                <StylePicker
                  label="Mini style"
                  value={appearance.mini.styleId}
                  followLabel={`Mini mode (${styleById(modeStyle(appearance, 'mini')).name})`}
                  onChange={(value) =>
                    updateAppearance((section) => setMini(section, { styleId: value }))
                  }
                />
                <StylePicker
                  label="Mini mode style"
                  value={appearance.modes.mini.styleId}
                  followLabel={`${styleById(mode.style).name} (mode default)`}
                  onChange={(value) =>
                    updateAppearance((section) => updateMode(section, 'mini', { styleId: value }))
                  }
                />
              </div>
              <p className="card__note">
                The Mini window follows these at once — no need to reopen it.
              </p>
            </section>
          </div>
          <aside className="mini-studio__preview" aria-label="Mini preview">
            <p className="appearance__preview-label">
              <Icon name="sparkles" /> Live · {styleById(styleId).name}
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

        <section className="mode-section" aria-label="Overlays for Mini">
          <div className="mode-section__head">
            <h2 className="section-title">Tiny overlays</h2>
            <p className="card__muted">
              When the reading must stay above everything and let clicks through, use one of these.
            </p>
          </div>
          <PackGallery status={null} only={mode.packs} />
        </section>
      </section>
    </PageStyle>
  );
}
