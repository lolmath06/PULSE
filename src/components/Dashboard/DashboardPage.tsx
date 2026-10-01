import { useState } from 'react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { ConfirmDialog } from '@/components/ConfirmDialog/ConfirmDialog';
import { copyText } from '@/utils/clipboard';
import type { WidgetInstance } from '@/dashboard/model';
import {
  activeDashboard,
  addWidget,
  createDashboard,
  dashboardName,
  deleteDashboard,
  duplicateDashboard,
  duplicateWidget,
  exportDashboard,
  importDashboard,
  moveWidgetTo,
  removeWidget,
  renameDashboard,
  resizeWidgetTo,
  setActive,
  setDashboardStyle,
  setLocked,
  updateWidget,
} from '@/dashboard/dashboards';
import { saveTemplate } from '@/dashboard/templates';
import { updateDashboards, updateTemplates, useDashboards } from '@/dashboard/store';
import { DashboardGrid } from '@/components/Dashboard/DashboardGrid';
import { WidgetCustomize } from '@/components/Dashboard/WidgetCustomize';
import { WidgetLibrary } from '@/components/Dashboard/WidgetLibrary';
import { StylePicker } from '@/components/Appearance/StylePicker';
import { Sheet } from '@/components/Sheet/Sheet';
import { TemplateGallery } from '@/components/Presets/TemplateGallery';
import {
  createDashboardFromTemplate,
  findTemplate,
  resetDashboardToOrigin,
} from '@/presets/dashboardTemplates';
import { PageStyle } from '@/design/LookContext';

type Dialog =
  | { kind: 'new' }
  | { kind: 'rename' }
  | { kind: 'delete' }
  | { kind: 'reset' }
  | { kind: 'export'; text: string }
  | { kind: 'import' };

/**
 * The configurable dashboard: several named dashboards, one shown, each a
 * grid of widgets the user adds, moves, resizes, customizes and removes.
 *
 * Locked by default — nothing moves by accident — and *Edit* reveals the
 * handles. Every change is saved to the shared configuration immediately.
 *
 * `onSendToOverlay` is provided by the overlay layer when available.
 */
export function DashboardPage({
  onSendToOverlay,
  headerExtra,
}: {
  readonly onSendToOverlay?: (widget: WidgetInstance) => void;
  readonly headerExtra?: ReactNode;
}) {
  const { t } = useTranslation();
  const section = useDashboards();
  const dashboard = activeDashboard(section);
  const currentName = dashboardName(dashboard);
  const [library, setLibrary] = useState(false);
  const [customizing, setCustomizing] = useState<string | null>(null);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [name, setName] = useState('');
  const [importText, setImportText] = useState('');
  const [importError, setImportError] = useState<string | null>(null);
  const editing = !dashboard.locked;
  const id = dashboard.id;
  const customized = dashboard.widgets.find((widget) => widget.id === customizing);

  const open = (next: Dialog, initial = '') => {
    setName(initial);
    setDialog(next);
  };

  return (
    <PageStyle styleId={dashboard.styleId}>
      <section className="page page--wide dashboard-page">
        <header className="dashboard-page__header">
          <div className="dashboard-page__title">
            <h1 className="page__title">{t('nav.routes.dashboard.label')}</h1>
            <select
              className="history-panel__select"
              aria-label={t('nav.routes.dashboard.label')}
              value={id}
              onChange={(event) =>
                updateDashboards((current) => setActive(current, event.target.value))
              }
            >
              {section.items.map((item) => (
                <option key={item.id} value={item.id}>
                  {dashboardName(item)}
                </option>
              ))}
            </select>
            <StylePicker
              label={t('dashboard.style')}
              value={dashboard.styleId}
              onChange={(styleId) =>
                updateDashboards((current) => setDashboardStyle(current, id, styleId))
              }
            />
          </div>
          <div className="dashboard-page__tools" role="toolbar" aria-label={t('dashboard.actions')}>
            <button
              type="button"
              className={`button${editing ? ' button--active' : ''}`}
              aria-pressed={editing}
              onClick={() => updateDashboards((current) => setLocked(current, id, editing))}
            >
              {editing ? t('dashboard.lockLayout') : t('dashboard.editLayout')}
            </button>
            <button type="button" className="button" onClick={() => setLibrary(true)}>
              {t('dashboard.addWidget')}
            </button>
            <button
              type="button"
              className="button button--quiet"
              onClick={() => open({ kind: 'new' }, '')}
            >
              {t('dashboard.new')}
            </button>
            <button
              type="button"
              className="button button--quiet"
              onClick={() => open({ kind: 'rename' }, currentName)}
            >
              {t('common.rename')}
            </button>
            <button
              type="button"
              className="button button--quiet"
              onClick={() => updateDashboards((current) => duplicateDashboard(current, id))}
            >
              {t('common.duplicate')}
            </button>
            <button
              type="button"
              className="button button--quiet"
              disabled={section.items.length <= 1}
              onClick={() => open({ kind: 'delete' })}
            >
              {t('common.delete')}
            </button>
            <button
              type="button"
              className="button button--quiet"
              onClick={() => open({ kind: 'reset' })}
            >
              {t('common.reset')}
            </button>
            <button
              type="button"
              className="button button--quiet"
              onClick={() =>
                setDialog({
                  kind: 'export',
                  text: JSON.stringify(exportDashboard(dashboard), null, 2),
                })
              }
            >
              {t('common.export')}
            </button>
            <button
              type="button"
              className="button button--quiet"
              onClick={() => {
                setImportText('');
                setImportError(null);
                setDialog({ kind: 'import' });
              }}
            >
              {t('common.import')}
            </button>
            {headerExtra}
          </div>
        </header>

        {dashboard.widgets.length === 0 ? (
          <div className="card dashboard-page__empty">
            <p className="card__muted">{t('dashboard.empty')}</p>
            <button type="button" className="button" onClick={() => setLibrary(true)}>
              {t('dashboard.addWidget')}
            </button>
          </div>
        ) : (
          <DashboardGrid
            dashboard={dashboard}
            editing={editing}
            actions={{
              move: (widgetId, x, y) =>
                updateDashboards((current) => moveWidgetTo(current, id, widgetId, x, y)),
              resize: (widgetId, w, h) =>
                updateDashboards((current) => resizeWidgetTo(current, id, widgetId, w, h)),
              remove: (widgetId) =>
                updateDashboards((current) => removeWidget(current, id, widgetId)),
              duplicate: (widgetId) =>
                updateDashboards((current) => duplicateWidget(current, id, widgetId)),
              customize: (widgetId) => setCustomizing(widgetId),
              sendToOverlay: onSendToOverlay
                ? (widgetId) => {
                    const widget = dashboard.widgets.find((entry) => entry.id === widgetId);
                    if (widget) onSendToOverlay(widget);
                  }
                : undefined,
            }}
          />
        )}

        {library && (
          <WidgetLibrary
            onClose={() => setLibrary(false)}
            onAdd={(widget) => {
              updateDashboards((current) => addWidget(current, id, widget));
              setLibrary(false);
            }}
          />
        )}

        {customized && (
          <WidgetCustomize
            widget={customized}
            onClose={() => setCustomizing(null)}
            onChange={(change) =>
              updateDashboards((current) => updateWidget(current, id, customized.id, change))
            }
            onSaveTemplate={(templateName) =>
              updateTemplates((current) => saveTemplate(current, templateName, customized))
            }
          />
        )}

        {dialog?.kind === 'new' && (
          <Sheet
            title={t('dashboard.newTitle')}
            subtitle={t('dashboard.newSubtitle')}
            onClose={() => setDialog(null)}
          >
            <div className="new-dashboard__name">
              <input
                type="text"
                className="customize__text"
                aria-label={t('dashboard.name')}
                placeholder={t('dashboard.nameOptional')}
                maxLength={40}
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
              <button
                type="button"
                className="button"
                onClick={() => {
                  updateDashboards((current) => createDashboard(current, name));
                  setDialog(null);
                }}
              >
                {t('dashboard.blank')}
              </button>
            </div>
            <TemplateGallery
              onUse={(template) => {
                updateDashboards(
                  (current) =>
                    createDashboardFromTemplate(current, template, name.trim() || undefined)
                      .section,
                );
                setDialog(null);
              }}
            />
          </Sheet>
        )}

        {dialog?.kind === 'rename' && (
          <ConfirmDialog
            title={t('dashboard.renameTitle', { name: currentName })}
            body={[]}
            confirmLabel={t('common.rename')}
            tone="neutral"
            confirmDisabled={!name.trim()}
            onCancel={() => setDialog(null)}
            onConfirm={() => {
              updateDashboards((current) => renameDashboard(current, id, name));
              setDialog(null);
            }}
          >
            <input
              type="text"
              className="customize__text dialog__input"
              aria-label={t('dashboard.name')}
              maxLength={40}
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </ConfirmDialog>
        )}

        {dialog?.kind === 'delete' && (
          <ConfirmDialog
            title={t('dashboard.deleteTitle', { name: currentName })}
            body={[t('dashboard.deleteBody')]}
            confirmLabel={t('dashboard.deleteConfirm')}
            tone="danger"
            onCancel={() => setDialog(null)}
            onConfirm={() => {
              updateDashboards((current) => deleteDashboard(current, id));
              setDialog(null);
            }}
          />
        )}

        {dialog?.kind === 'reset' && (
          <ConfirmDialog
            title={t('dashboard.resetTitle', { name: currentName })}
            body={[
              findTemplate(dashboard.origin?.template)
                ? t('dashboard.resetToTemplate', {
                    name: t(
                      `presets.templates.${findTemplate(dashboard.origin?.template)!.id}.name`,
                    ),
                  })
                : t('dashboard.resetToDefault'),
            ]}
            confirmLabel={t('dashboard.resetConfirm')}
            tone="warning"
            onCancel={() => setDialog(null)}
            onConfirm={() => {
              updateDashboards((current) => resetDashboardToOrigin(current, id));
              setDialog(null);
            }}
          />
        )}

        {dialog?.kind === 'export' && (
          <ConfirmDialog
            title={t('dashboard.exportTitle')}
            body={[t('dashboard.exportBody')]}
            confirmLabel={t('common.copyToClipboard')}
            tone="neutral"
            onCancel={() => setDialog(null)}
            onConfirm={() => {
              void copyText(dialog.text);
              setDialog(null);
            }}
          >
            <textarea
              className="dialog__textarea mono"
              readOnly
              value={dialog.text}
              aria-label={t('dashboard.exportedJson')}
            />
          </ConfirmDialog>
        )}

        {dialog?.kind === 'import' && (
          <ConfirmDialog
            title={t('dashboard.importTitle')}
            body={[t('dashboard.importBody')]}
            confirmLabel={t('common.import')}
            tone="neutral"
            confirmDisabled={!importText.trim()}
            onCancel={() => setDialog(null)}
            onConfirm={() => {
              let parsed: unknown;
              try {
                parsed = JSON.parse(importText);
              } catch {
                setImportError(t('common.invalidJson'));
                return;
              }
              let error: string | undefined;
              updateDashboards((current) => {
                const result = importDashboard(current, parsed);
                error = result.error;
                return result.section;
              });
              if (error) setImportError(error);
              else setDialog(null);
            }}
          >
            <textarea
              className="dialog__textarea mono"
              aria-label={t('dashboard.json')}
              value={importText}
              onChange={(event) => setImportText(event.target.value)}
            />
            {importError && (
              <p className="customize__error" role="alert">
                {importError}
              </p>
            )}
          </ConfirmDialog>
        )}
      </section>
    </PageStyle>
  );
}
