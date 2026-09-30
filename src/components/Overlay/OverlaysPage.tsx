import { useCallback, useEffect, useState } from 'react';
import { ConfirmDialog } from '@/components/ConfirmDialog/ConfirmDialog';
import { onUiConfigChange } from '@/config/uiConfigEvents';
import type { Overlay } from '@/overlay/model';
import {
  OVERLAY_LAYOUTS,
  OVERLAY_PRESETS,
  addOverlayWidget,
  createOverlay,
  createOverlayFromPreset,
  deleteOverlay,
  fitToContent,
  moveOverlayWidget,
  removeOverlayWidget,
  updateOverlay,
  updateOverlayWidget,
} from '@/overlay/model';
import { updateOverlays, useOverlays } from '@/overlay/store';
import type { DesktopStatus, OverlayAction } from '@/overlay/desktop';
import {
  getDesktopStatus,
  openMiniWindow,
  overlayAction,
  setOverlayHotkey,
} from '@/overlay/desktop';
import { DEFAULT_HOTKEY, setCloseBehavior, useDesktopSettings } from '@/overlay/settings';
import { addWidget } from '@/dashboard/dashboards';
import { updateDashboards, updateTemplates, useDashboards } from '@/dashboard/store';
import { saveTemplate } from '@/dashboard/templates';
import { widgetTitle } from '@/dashboard/geometry';
import { newId } from '@/dashboard/ids';
import { Choice, ColorField, Slider, Toggle } from '@/visualization/CustomizePanel';
import { WidgetCustomize } from '@/components/Dashboard/WidgetCustomize';
import { WidgetLibrary } from '@/components/Dashboard/WidgetLibrary';
import { CapabilityList } from '@/components/Overlay/CapabilityList';
import { OverlayPreview } from '@/components/Overlay/OverlayApp';

/**
 * Desktop overlays: what this session can do, the global shortcut, what
 * closing the main window does, and the editor for every overlay.
 */
export function OverlaysPage() {
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
    refresh();
    return onUiConfigChange(refresh);
  }, [refresh]);

  const act = (action: OverlayAction) => void overlayAction(action).catch(() => undefined);

  return (
    <section className="page page--wide overlays-page">
      <h1 className="page__title">Overlays</h1>
      <p className="page__subtitle">
        Widgets in their own small windows on the desktop — above other apps where the system
        allows, click-through when locked. A safe desktop window: PULSE never injects into games.
      </p>

      <div className="overlays-page__actions" role="toolbar" aria-label="All overlays">
        <button type="button" className="button" onClick={() => act('editAll')}>
          Edit all
        </button>
        <button type="button" className="button" onClick={() => act('lockAll')}>
          Lock all
        </button>
        <button type="button" className="button button--quiet" onClick={() => act('showAll')}>
          Show all
        </button>
        <button type="button" className="button button--quiet" onClick={() => act('hideAll')}>
          Hide all
        </button>
        <button
          type="button"
          className="button button--quiet"
          onClick={() => void openMiniWindow().catch(() => undefined)}
        >
          Open Mini window
        </button>
      </div>

      <div className="card">
        <h2 className="card__title">New overlay</h2>
        <div className="overlays-page__presets">
          <button
            type="button"
            className="library__entry overlays-page__preset"
            onClick={() => updateOverlays((section) => createOverlay(section, 'Overlay').section)}
          >
            <span className="library__label">Empty</span>
            <span className="library__description">Add widgets yourself.</span>
          </button>
          {OVERLAY_PRESETS.map((preset) => (
            <button
              key={preset.id}
              type="button"
              className="library__entry overlays-page__preset"
              onClick={() =>
                updateOverlays((section) => createOverlayFromPreset(section, preset).section)
              }
            >
              <span className="library__label">{preset.name}</span>
              <span className="library__description">{preset.description}</span>
            </button>
          ))}
        </div>
      </div>

      {overlays.items.map((overlay) => (
        <OverlayEditor
          key={overlay.id}
          overlay={overlay}
          canPosition={status?.capabilities.positioning.status === 'supported'}
        />
      ))}

      <div className="card">
        <h2 className="card__title">Global shortcut</h2>
        <p className="card__muted">
          Toggles the visible overlays between Edit and Locked. Hidden overlays keep their state.
        </p>
        <div className="customize__row">
          <input
            type="text"
            className="customize__text"
            aria-label="Global shortcut"
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
            Apply
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
            Disable
          </button>
        </div>
        {backend?.kind === 'portal' && (
          <p className="card__note">
            Delivered by your desktop through the XDG Desktop Portal, whichever application has
            focus. The desktop may ask you to approve it, and may let you change the keys in its own
            keyboard settings.
          </p>
        )}
        {backend?.kind === 'unavailable' && (
          <p className="card__note" role="status">
            {`Global shortcuts are not available on this session: ${backend.reason}`}
          </p>
        )}
        {hotkeyError && (
          <p className="customize__error" role="alert">
            {backend?.kind === 'plugin' || !backend
              ? `Not registered — ${hotkeyError}. The previous shortcut is still active.`
              : `Not bound — ${hotkeyError}.`}
          </p>
        )}
        {status && (
          <p className="card__note">
            {status.hotkey ? `Active: ${status.hotkey}.` : 'No global shortcut is active.'}
          </p>
        )}
      </div>

      <div className="card">
        <h2 className="card__title">Closing the main window</h2>
        <div className="segmented" role="group" aria-label="When the main window is closed">
          {(
            [
              ['quit', 'Quit PULSE'],
              ['keep-running', 'Keep running while overlays are visible'],
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
        <p className="card__note">
          Quit is the default. When PULSE keeps running, reopen it from the tray or an
          overlay&apos;s “Open PULSE”; Quit from the tray stops everything.
        </p>
      </div>

      <div className="card">
        <h2 className="card__title">What overlays can do here</h2>
        {status ? (
          <CapabilityList capabilities={status.capabilities} />
        ) : (
          <p className="card__muted" title={statusError ?? undefined}>
            {statusError ? 'Backend unavailable. Run PULSE with pnpm app:dev.' : 'Checking…'}
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
  const dashboards = useDashboards();
  const [library, setLibrary] = useState(false);
  const [customizing, setCustomizing] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const set = (change: (o: Overlay) => Overlay) =>
    updateOverlays((section) => updateOverlay(section, overlay.id, change));
  const customized = overlay.widgets.find((widget) => widget.id === customizing);

  return (
    <div className="card overlay-editor" aria-label={`Overlay ${overlay.name}`}>
      <header className="overlay-editor__header">
        <input
          type="text"
          className="customize__text overlay-editor__name"
          aria-label="Overlay name"
          maxLength={40}
          value={overlay.name}
          onChange={(event) => set((o) => ({ ...o, name: event.target.value }))}
        />
        <div className="customize__row">
          <Toggle
            text="Visible"
            checked={overlay.visible}
            onChange={(visible) => set((o) => ({ ...o, visible }))}
          />
          <button
            type="button"
            className={`button${overlay.locked ? '' : ' button--active'}`}
            aria-pressed={!overlay.locked}
            onClick={() => set((o) => ({ ...o, locked: !o.locked }))}
          >
            {overlay.locked ? 'Edit overlay' : 'Lock overlay'}
          </button>
          <button type="button" className="button button--quiet" onClick={() => setDeleting(true)}>
            Delete
          </button>
        </div>
      </header>

      <div className="overlay-editor__body">
        <div className="overlay-editor__preview" aria-label="Preview">
          <OverlayPreview overlay={overlay} />
        </div>
        <div className="overlay-editor__settings">
          <Choice
            ariaLabel="Overlay layout"
            options={OVERLAY_LAYOUTS}
            value={overlay.layout}
            labelFor={(value) => ({ horizontal: 'Row', vertical: 'Column', grid: 'Grid' })[value]}
            onChange={(layout) => set((o) => ({ ...o, layout }))}
          />
          <Toggle
            text="Background"
            checked={overlay.chrome.background !== null}
            onChange={(on) =>
              set((o) => ({ ...o, chrome: { ...o.chrome, background: on ? '#0d1013' : null } }))
            }
          />
          {overlay.chrome.background !== null && (
            <ColorField
              text="Overlay background"
              value={overlay.chrome.background}
              onChange={(background) => set((o) => ({ ...o, chrome: { ...o.chrome, background } }))}
            />
          )}
          <label className="customize__inline">
            Background opacity
            <Slider
              ariaLabel="Overlay background opacity"
              value={overlay.chrome.opacity}
              min={0}
              max={1}
              step={0.05}
              format={(value) => `${Math.round(value * 100)} %`}
              onChange={(opacity) => set((o) => ({ ...o, chrome: { ...o.chrome, opacity } }))}
            />
          </label>
          <div className="customize__toggles">
            <Toggle
              text="Border"
              checked={overlay.chrome.border === 'thin'}
              onChange={(on) =>
                set((o) => ({ ...o, chrome: { ...o.chrome, border: on ? 'thin' : 'none' } }))
              }
            />
            <Toggle
              text="Shadow"
              checked={overlay.chrome.shadow}
              onChange={(shadow) => set((o) => ({ ...o, chrome: { ...o.chrome, shadow } }))}
            />
          </div>
          <label className="customize__inline">
            Corners
            <Slider
              ariaLabel="Overlay corner radius"
              value={overlay.chrome.radius}
              min={0}
              max={32}
              step={1}
              format={(value) => `${value} px`}
              onChange={(radius) => set((o) => ({ ...o, chrome: { ...o.chrome, radius } }))}
            />
          </label>
          <label className="customize__inline">
            Gap
            <Slider
              ariaLabel="Gap between widgets"
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
              W
              <input
                type="number"
                aria-label="Overlay width"
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
              H
              <input
                type="number"
                aria-label="Overlay height"
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
              Fit to widgets
            </button>
          </div>
          {canPosition ? (
            <div className="customize__row">
              <label className="customize__inline">
                X
                <input
                  type="number"
                  aria-label="Overlay x"
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
                  aria-label="Overlay y"
                  value={Math.round(overlay.geometry.y)}
                  onChange={(event) =>
                    set((o) => ({
                      ...o,
                      geometry: { ...o.geometry, y: Number(event.target.value) },
                    }))
                  }
                />
              </label>
              <span className="card__note">{overlay.geometry.monitor ?? 'primary monitor'}</span>
            </div>
          ) : (
            <p className="card__note">
              This session cannot place windows: drag the overlay by its bar in Edit mode.
            </p>
          )}
        </div>
      </div>

      <ul className="overlay-editor__widgets" aria-label={`${overlay.name} widgets`}>
        {overlay.widgets.map((widget, index) => (
          <li key={widget.id} className="overlay-editor__widget">
            <span className="overlay-editor__widget-name">{widgetTitle(widget)}</span>
            <label className="customize__inline">
              W
              <input
                type="number"
                aria-label={`${widgetTitle(widget)} width`}
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
              H
              <input
                type="number"
                aria-label={`${widgetTitle(widget)} height`}
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
              Customize
            </button>
            <button
              type="button"
              className="button button--quiet"
              aria-label={`Move ${widgetTitle(widget)} earlier`}
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
              aria-label={`Move ${widgetTitle(widget)} later`}
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
              Copy to dashboard
            </button>
            <button
              type="button"
              className="button button--quiet"
              aria-label={`Remove ${widgetTitle(widget)}`}
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
        Add widget
      </button>
      <span className="card__note">{` Copies go to the dashboard “${dashboards.items.find((d) => d.id === dashboards.activeId)?.name ?? ''}”.`}</span>

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
      {deleting && (
        <ConfirmDialog
          title={`Delete overlay “${overlay.name}”?`}
          body={['Its window closes and its widgets are removed.']}
          confirmLabel="Delete overlay"
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
