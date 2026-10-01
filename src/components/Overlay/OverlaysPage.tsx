import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ConfirmDialog } from '@/components/ConfirmDialog/ConfirmDialog';
import { onUiConfigChange } from '@/config/uiConfigEvents';
import type { Overlay } from '@/overlay/model';
import {
  OVERLAY_LAYOUTS,
  addOverlayWidget,
  applyOverlayStyle,
  createOverlay,
  createOverlayFromUserPack,
  deleteOverlay,
  duplicateOverlay,
  fitToContent,
  moveOverlayWidget,
  overlayName,
  removeOverlayWidget,
  updateOverlay,
  updateOverlayWidget,
} from '@/overlay/model';
import { updateOverlays, useOverlays } from '@/overlay/store';
import type { DesktopStatus, OverlayAction } from '@/overlay/desktop';
import {
  getDesktopStatus,
  onDesktopStatusChanged,
  openMiniWindow,
  overlayAction,
  refreshGnomeBridge,
  setOverlayHotkey,
} from '@/overlay/desktop';
import { DEFAULT_HOTKEY, setCloseBehavior, useDesktopSettings } from '@/overlay/settings';
import { addWidget, dashboardName } from '@/dashboard/dashboards';
import { englishText } from '@/i18n/i18n';
import { updateDashboards, updateTemplates, useDashboards } from '@/dashboard/store';
import { deleteUserPack, saveTemplate, saveUserPack } from '@/dashboard/templates';
import { useTemplates } from '@/dashboard/store';
import { findPack, resetOverlayToPack } from '@/presets/overlayPacks';
import { PackGallery } from '@/components/Presets/PackGallery';
import { Icon } from '@/components/Icon';
import { widgetTitle } from '@/dashboard/geometry';
import { newId } from '@/dashboard/ids';
import { Choice, ColorField, Slider, Toggle } from '@/visualization/CustomizePanel';
import { WidgetCustomize } from '@/components/Dashboard/WidgetCustomize';
import { WidgetLibrary } from '@/components/Dashboard/WidgetLibrary';
import { CapabilityList } from '@/components/Overlay/CapabilityList';
import { OverlayBackendPanel } from '@/components/Overlay/OverlayBackendPanel';
import { StylePicker } from '@/components/Appearance/StylePicker';
import { overlayStyleChrome, resolveLook } from '@/design/look';
import { readAppearance } from '@/design/store';
import { OverlayPreview } from '@/components/Overlay/OverlayApp';
import { formatFixed } from '@/i18n/format';

/**
 * Desktop overlays: what this session can do, the global shortcut, what
 * closing the main window does, and the editor for every overlay.
 */
export function OverlaysPage() {
  const { t } = useTranslation();
  const overlays = useOverlays();
  const settings = useDesktopSettings();
  const [status, setStatus] = useState<DesktopStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [hotkey, setHotkey] = useState(settings.overlayHotkey ?? '');
  const [hotkeyError, setHotkeyError] = useState<string | null>(null);
  const backend = status?.hotkeyBackend ?? null;

  const refresh = useCallback(() => {
    getDesktopStatus()
      .then((next) => {
        setStatus(next);
        setStatusError(null);
      })
      .catch((error: unknown) =>
        setStatusError(error instanceof Error ? error.message : String(error)),
      );
  }, []);
  useEffect(() => {
    // Opening the page asks GNOME Shell afresh (once); configuration changes
    // only re-read the backend's cached status, so a dragged slider never
    // reaches GNOME Shell.
    refreshGnomeBridge()
      .then((next) => {
        setStatus(next);
        setStatusError(null);
      })
      .catch((error: unknown) =>
        setStatusError(error instanceof Error ? error.message : String(error)),
      );
    const offConfig = onUiConfigChange(refresh);
    const offStatus = onDesktopStatusChanged(refresh);
    return () => {
      offConfig();
      offStatus();
    };
  }, [refresh]);
  const bridgeHotkey = status?.backend?.kind === 'gnomeBridge';

  const act = (action: OverlayAction) => void overlayAction(action).catch(() => undefined);

  return (
    <section className="page page--wide overlays-page">
      <h1 className="page__title">{t('nav.routes.overlays.label')}</h1>
      <p className="page__subtitle">{t('overlays.page.subtitle')}</p>

      <div className="overlays-page__actions" role="toolbar" aria-label={t('overlays.page.all')}>
        <button type="button" className="button" onClick={() => act('editAll')}>
          {t('overlays.page.editAll')}
        </button>
        <button type="button" className="button" onClick={() => act('lockAll')}>
          {t('overlays.page.lockAll')}
        </button>
        <button type="button" className="button button--quiet" onClick={() => act('showAll')}>
          {t('overlays.page.showAll')}
        </button>
        <button type="button" className="button button--quiet" onClick={() => act('hideAll')}>
          {t('overlays.page.hideAll')}
        </button>
        <button
          type="button"
          className="button button--quiet"
          onClick={() => void openMiniWindow().catch(() => undefined)}
        >
          {t('overlays.page.openMini')}
        </button>
      </div>

      <OverlayBackendPanel status={status} error={statusError} onStatus={setStatus} />

      <section className="mode-section" aria-label={t('overlays.page.newOverlay')}>
        <div className="mode-section__head mode-section__head--row">
          <div>
            <h2 className="section-title">{t('overlays.page.packs')}</h2>
            <p className="card__muted">{t('overlays.page.packsHint')}</p>
          </div>
          <button
            type="button"
            className="button"
            onClick={() =>
              updateOverlays(
                (section) =>
                  createOverlay(section, englishText('overlays.defaultName'), [], {
                    nameKey: 'overlays.defaultName',
                  }).section,
              )
            }
          >
            <Icon name="plus" /> {t('overlays.page.emptyOverlay')}
          </button>
        </div>
        <PackGallery status={status} />
        <UserPacks />
      </section>

      {overlays.items.map((overlay) => (
        <OverlayEditor
          key={overlay.id}
          overlay={overlay}
          canPosition={status?.capabilities.positioning.status === 'supported'}
        />
      ))}

      <div className="card">
        <h2 className="card__title">{t('overlays.capabilities.globalHotkey')}</h2>
        <p className="card__muted">{t('overlays.hotkey.hint')}</p>
        <div className="customize__row">
          <input
            type="text"
            className="customize__text"
            aria-label={t('overlays.capabilities.globalHotkey')}
            placeholder={DEFAULT_HOTKEY}
            value={hotkey}
            onChange={(event) => setHotkey(event.target.value)}
          />
          <button
            type="button"
            className="button"
            onClick={() =>
              void setOverlayHotkey(hotkey.trim() || null)
                .then(() => {
                  setHotkeyError(null);
                  refresh();
                })
                .catch((error: unknown) => {
                  setHotkeyError(error instanceof Error ? error.message : String(error));
                  refresh();
                })
            }
          >
            {t('common.apply')}
          </button>
          <button
            type="button"
            className="button button--quiet"
            onClick={() => {
              setHotkey('');
              void setOverlayHotkey(null)
                .then(refresh)
                .catch(() => undefined);
            }}
          >
            {t('overlays.bridge.disable')}
          </button>
        </div>
        {bridgeHotkey && <p className="card__note">{t('overlays.hotkey.viaBridge')}</p>}
        {!bridgeHotkey && backend?.kind === 'portal' && (
          <p className="card__note">{t('overlays.hotkey.viaPortal')}</p>
        )}
        {!bridgeHotkey && backend?.kind === 'unavailable' && (
          <p className="card__note" role="status">
            {t('overlays.hotkey.unavailable', { reason: backend.reason })}
          </p>
        )}
        {hotkeyError && (
          <p className="customize__error" role="alert">
            {backend?.kind === 'plugin' || !backend
              ? t('overlays.hotkey.notRegistered', { error: hotkeyError })
              : t('overlays.hotkey.notBound', { error: hotkeyError })}
          </p>
        )}
        {status && (
          <p className="card__note">
            {status.hotkey
              ? t('overlays.hotkey.active', { hotkey: status.hotkey })
              : t('overlays.hotkey.none')}
          </p>
        )}
      </div>

      <div className="card">
        <h2 className="card__title">{t('overlays.close.title')}</h2>
        <div className="segmented" role="group" aria-label={t('overlays.close.when')}>
          {(
            [
              ['quit', t('overlays.close.quit')],
              ['keep-running', t('overlays.close.keepRunning')],
            ] as const
          ).map(([value, text]) => (
            <button
              key={value}
              type="button"
              className={`segmented__option${settings.closeBehavior === value ? ' segmented__option--active' : ''}`}
              aria-pressed={settings.closeBehavior === value}
              onClick={() => setCloseBehavior(value)}
            >
              {text}
            </button>
          ))}
        </div>
        <p className="card__note">{t('overlays.close.note')}</p>
      </div>

      <div className="card">
        <h2 className="card__title">{t('overlays.page.everyCapability')}</h2>
        {status ? (
          <CapabilityList capabilities={status.capabilities} />
        ) : (
          <p className="card__muted" title={statusError ?? undefined}>
            {statusError ? t('overlays.backend.unavailable') : t('common.checking')}
          </p>
        )}
      </div>
    </section>
  );
}

function OverlayEditor({
  overlay,
  canPosition,
}: {
  readonly overlay: Overlay;
  readonly canPosition: boolean;
}) {
  const { t } = useTranslation();
  const dashboards = useDashboards();
  const name = overlayName(overlay);
  const [library, setLibrary] = useState(false);
  const [customizing, setCustomizing] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const set = (change: (o: Overlay) => Overlay) =>
    updateOverlays((section) => updateOverlay(section, overlay.id, change));
  const customized = overlay.widgets.find((widget) => widget.id === customizing);
  const pack = findPack(overlay.origin?.pack);
  const [saving, setSaving] = useState(false);
  const [packName, setPackName] = useState('');

  return (
    <div className="card overlay-editor" aria-label={t('overlays.editor.aria', { name })}>
      <header className="overlay-editor__header">
        <input
          type="text"
          className="customize__text overlay-editor__name"
          aria-label={t('overlays.editor.name')}
          maxLength={40}
          value={name}
          onChange={(event) =>
            // Typed by the user: their words from now on, in every language.
            set(({ nameKey: _builtIn, ...o }) => ({ ...o, name: event.target.value }))
          }
        />
        <div className="customize__row">
          <Toggle
            text={t('overlays.editor.visible')}
            checked={overlay.visible}
            onChange={(visible) => set((o) => ({ ...o, visible }))}
          />
          <button
            type="button"
            className={`button${overlay.locked ? '' : ' button--active'}`}
            aria-pressed={!overlay.locked}
            onClick={() => set((o) => ({ ...o, locked: !o.locked }))}
          >
            {overlay.locked ? t('overlays.editor.edit') : t('overlays.editor.lock')}
          </button>
          <button
            type="button"
            className="button button--quiet"
            onClick={() => updateOverlays((section) => duplicateOverlay(section, overlay.id))}
          >
            {t('common.duplicate')}
          </button>
          <button
            type="button"
            className="button button--quiet"
            onClick={() => {
              setPackName(name);
              setSaving(true);
            }}
          >
            {t('overlays.editor.saveAsPack')}
          </button>
          <button type="button" className="button button--quiet" onClick={() => setDeleting(true)}>
            {t('common.delete')}
          </button>
        </div>
      </header>
      {pack && (
        <p className="overlay-editor__origin">
          <span className="chip">
            {t('overlays.editor.fromPack', { name: t(`presets.packs.${pack.id}.name`) })}
          </span>
          <button
            type="button"
            className="button button--quiet"
            onClick={() =>
              updateOverlays((section) =>
                resetOverlayToPack(section, overlay.id, readAppearance().custom),
              )
            }
          >
            {t('overlays.editor.resetToPack')}
          </button>
        </p>
      )}

      <div className="overlay-editor__body">
        <div className="overlay-editor__preview" aria-label={t('common.preview')}>
          <OverlayPreview overlay={overlay} />
        </div>
        <div className="overlay-editor__settings">
          <StylePicker
            label={t('overlays.editor.style')}
            value={overlay.styleId}
            onChange={(styleId) => {
              const appearance = readAppearance();
              const look = resolveLook(styleId ?? appearance.styleId, appearance.custom);
              updateOverlays((section) =>
                applyOverlayStyle(section, overlay.id, styleId, overlayStyleChrome(look)),
              );
            }}
          />
          <Choice
            ariaLabel={t('overlays.editor.layout')}
            options={OVERLAY_LAYOUTS}
            value={overlay.layout}
            labelFor={(value) => t(`overlays.layouts.${value}`)}
            onChange={(layout) => set((o) => ({ ...o, layout }))}
          />
          <Toggle
            text={t('customize.background')}
            checked={overlay.chrome.background !== null}
            onChange={(on) =>
              set((o) => ({ ...o, chrome: { ...o.chrome, background: on ? '#0d1013' : null } }))
            }
          />
          {overlay.chrome.background !== null && (
            <ColorField
              text={t('overlays.editor.background')}
              value={overlay.chrome.background}
              onChange={(background) => set((o) => ({ ...o, chrome: { ...o.chrome, background } }))}
            />
          )}
          <label className="customize__inline">
            {t('customize.backgroundOpacity')}
            <Slider
              ariaLabel={t('overlays.editor.backgroundOpacity')}
              value={overlay.chrome.opacity}
              min={0}
              max={1}
              step={0.05}
              format={(value) => `${formatFixed(value * 100, 0)} %`}
              onChange={(opacity) => set((o) => ({ ...o, chrome: { ...o.chrome, opacity } }))}
            />
          </label>
          <div className="customize__toggles">
            <Toggle
              text={t('customize.border')}
              checked={overlay.chrome.border === 'thin'}
              onChange={(on) =>
                set((o) => ({ ...o, chrome: { ...o.chrome, border: on ? 'thin' : 'none' } }))
              }
            />
            <Toggle
              text={t('customize.shadow')}
              checked={overlay.chrome.shadow}
              onChange={(shadow) => set((o) => ({ ...o, chrome: { ...o.chrome, shadow } }))}
            />
          </div>
          <label className="customize__inline">
            {t('customize.corners')}
            <Slider
              ariaLabel={t('overlays.editor.cornerRadius')}
              value={overlay.chrome.radius}
              min={0}
              max={32}
              step={1}
              format={(value) => `${value} px`}
              onChange={(radius) => set((o) => ({ ...o, chrome: { ...o.chrome, radius } }))}
            />
          </label>
          <label className="customize__inline">
            {t('customize.gap')}
            <Slider
              ariaLabel={t('overlays.editor.gapBetween')}
              value={overlay.gap}
              min={0}
              max={32}
              step={1}
              format={(value) => `${value} px`}
              onChange={(gap) => set((o) => ({ ...o, gap }))}
            />
          </label>
          <div className="customize__row">
            <label className="customize__inline">
              {t('customize.widthShort')}
              <input
                type="number"
                aria-label={t('overlays.editor.width')}
                value={Math.round(overlay.geometry.width)}
                onChange={(event) =>
                  set((o) => ({
                    ...o,
                    geometry: {
                      ...o.geometry,
                      width: Number(event.target.value) || o.geometry.width,
                    },
                  }))
                }
              />
            </label>
            <label className="customize__inline">
              {t('customize.heightShort')}
              <input
                type="number"
                aria-label={t('overlays.editor.height')}
                value={Math.round(overlay.geometry.height)}
                onChange={(event) =>
                  set((o) => ({
                    ...o,
                    geometry: {
                      ...o.geometry,
                      height: Number(event.target.value) || o.geometry.height,
                    },
                  }))
                }
              />
            </label>
            <button
              type="button"
              className="button button--quiet"
              onClick={() => updateOverlays((section) => fitToContent(section, overlay.id))}
            >
              {t('overlays.editor.fit')}
            </button>
          </div>
          {canPosition ? (
            <div className="customize__row">
              <label className="customize__inline">
                X
                <input
                  type="number"
                  aria-label={t('overlays.editor.x')}
                  value={Math.round(overlay.geometry.x)}
                  onChange={(event) =>
                    set((o) => ({
                      ...o,
                      geometry: { ...o.geometry, x: Number(event.target.value) },
                    }))
                  }
                />
              </label>
              <label className="customize__inline">
                Y
                <input
                  type="number"
                  aria-label={t('overlays.editor.y')}
                  value={Math.round(overlay.geometry.y)}
                  onChange={(event) =>
                    set((o) => ({
                      ...o,
                      geometry: { ...o.geometry, y: Number(event.target.value) },
                    }))
                  }
                />
              </label>
              <span className="card__note">
                {overlay.geometry.monitor ?? t('overlays.editor.primaryMonitor')}
              </span>
            </div>
          ) : (
            <p className="card__note">{t('overlays.editor.cannotPlace')}</p>
          )}
        </div>
      </div>

      <ul className="overlay-editor__widgets" aria-label={t('overlays.editor.widgets', { name })}>
        {overlay.widgets.map((widget, index) => (
          <li key={widget.id} className="overlay-editor__widget">
            <span className="overlay-editor__widget-name">{widgetTitle(widget)}</span>
            <label className="customize__inline">
              {t('customize.widthShort')}
              <input
                type="number"
                aria-label={t('overlays.editor.widgetWidth', { name: widgetTitle(widget) })}
                value={widget.size.width}
                onChange={(event) =>
                  updateOverlays((section) =>
                    updateOverlayWidget(section, overlay.id, widget.id, (w) => ({
                      ...w,
                      size: {
                        ...w.size,
                        width: Math.max(24, Number(event.target.value) || w.size.width),
                      },
                    })),
                  )
                }
              />
            </label>
            <label className="customize__inline">
              {t('customize.heightShort')}
              <input
                type="number"
                aria-label={t('overlays.editor.widgetHeight', { name: widgetTitle(widget) })}
                value={widget.size.height}
                onChange={(event) =>
                  updateOverlays((section) =>
                    updateOverlayWidget(section, overlay.id, widget.id, (w) => ({
                      ...w,
                      size: {
                        ...w.size,
                        height: Math.max(16, Number(event.target.value) || w.size.height),
                      },
                    })),
                  )
                }
              />
            </label>
            <button
              type="button"
              className="button button--quiet"
              onClick={() => setCustomizing(widget.id)}
            >
              {t('common.customize')}
            </button>
            <button
              type="button"
              className="button button--quiet"
              aria-label={t('overlays.editor.moveEarlier', { name: widgetTitle(widget) })}
              disabled={index === 0}
              onClick={() =>
                updateOverlays((section) => moveOverlayWidget(section, overlay.id, widget.id, -1))
              }
            >
              ↑
            </button>
            <button
              type="button"
              className="button button--quiet"
              aria-label={t('overlays.editor.moveLater', { name: widgetTitle(widget) })}
              disabled={index === overlay.widgets.length - 1}
              onClick={() =>
                updateOverlays((section) => moveOverlayWidget(section, overlay.id, widget.id, 1))
              }
            >
              ↓
            </button>
            <button
              type="button"
              className="button button--quiet"
              onClick={() =>
                updateDashboards((current) =>
                  addWidget(current, current.activeId, { ...widget, id: newId('w') }),
                )
              }
            >
              {t('overlays.editor.copyToDashboard')}
            </button>
            <button
              type="button"
              className="button button--quiet"
              aria-label={t('common.removeNamed', { name: widgetTitle(widget) })}
              onClick={() =>
                updateOverlays((section) => removeOverlayWidget(section, overlay.id, widget.id))
              }
            >
              ×
            </button>
          </li>
        ))}
      </ul>
      <button type="button" className="button" onClick={() => setLibrary(true)}>
        {t('dashboard.addWidget')}
      </button>
      <span className="card__note">
        {' '}
        {t('overlays.editor.copiesGoTo', {
          name: dashboardName(
            dashboards.items.find((d) => d.id === dashboards.activeId) ?? { name: '' },
          ),
        })}
      </span>

      {library && (
        <WidgetLibrary
          onClose={() => setLibrary(false)}
          onAdd={(widget) => {
            updateOverlays((section) => addOverlayWidget(section, overlay.id, widget));
            setLibrary(false);
          }}
        />
      )}
      {customized && (
        <WidgetCustomize
          widget={customized}
          onClose={() => setCustomizing(null)}
          onChange={(change) =>
            updateOverlays((section) =>
              updateOverlayWidget(section, overlay.id, customized.id, change),
            )
          }
          onSaveTemplate={(name) =>
            updateTemplates((current) => saveTemplate(current, name, customized))
          }
        />
      )}
      {saving && (
        <ConfirmDialog
          title={t('overlays.editor.saveAsPack')}
          body={[t('overlays.editor.saveAsPackBody')]}
          confirmLabel={t('overlays.editor.savePack')}
          tone="neutral"
          confirmDisabled={!packName.trim()}
          onCancel={() => setSaving(false)}
          onConfirm={() => {
            updateTemplates((current) => saveUserPack(current, packName, overlay));
            setSaving(false);
          }}
        >
          <input
            type="text"
            className="customize__text dialog__input"
            aria-label={t('overlays.editor.packName')}
            maxLength={40}
            value={packName}
            onChange={(event) => setPackName(event.target.value)}
          />
        </ConfirmDialog>
      )}
      {deleting && (
        <ConfirmDialog
          title={t('overlays.editor.deleteTitle', { name })}
          body={[t('overlays.editor.deleteBody')]}
          confirmLabel={t('overlays.editor.delete')}
          tone="danger"
          onCancel={() => setDeleting(false)}
          onConfirm={() => {
            updateOverlays((section) => deleteOverlay(section, overlay.id));
            setDeleting(false);
          }}
        />
      )}
    </div>
  );
}

/** The user's saved overlay packs. */
function UserPacks() {
  const { t } = useTranslation();
  const templates = useTemplates();
  if (templates.overlays.length === 0) return null;
  return (
    <div className="user-packs">
      <h3 className="section-subtitle">{t('overlays.page.yourPacks')}</h3>
      <ul className="user-styles">
        {templates.overlays.map((pack) => (
          <li key={pack.id} className="user-style">
            <span className="user-style__name">
              {pack.name}
              <small>{t('common.widgetCount', { count: pack.overlay.widgets.length })}</small>
            </span>
            <button
              type="button"
              className="button"
              onClick={() =>
                updateOverlays(
                  (section) => createOverlayFromUserPack(section, pack.name, pack.overlay).section,
                )
              }
            >
              <Icon name="plus" /> {t('common.add')}
            </button>
            <button
              type="button"
              className="button button--quiet"
              aria-label={t('overlays.page.deletePack', { name: pack.name })}
              onClick={() => updateTemplates((current) => deleteUserPack(current, pack.id))}
            >
              <Icon name="close" />
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
