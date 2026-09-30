import { useState } from 'react';
import type { ReactNode } from 'react';
import { ConfirmDialog } from '@/components/ConfirmDialog/ConfirmDialog';
import { copyText } from '@/utils/clipboard';
import type { WidgetInstance } from '@/dashboard/model';
import {
  activeDashboard,
  addWidget,
  createDashboard,
  deleteDashboard,
  duplicateDashboard,
  duplicateWidget,
  exportDashboard,
  importDashboard,
  moveWidgetTo,
  removeWidget,
  renameDashboard,
  resetDashboard,
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
  const section = useDashboards();
  const dashboard = activeDashboard(section);
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
            <h1 className="page__title">Dashboard</h1>
            <select
              className="history-panel__select"
              aria-label="Dashboard"
              value={id}
              onChange={(event) =>
                updateDashboards((current) => setActive(current, event.target.value))
              }
            >
              {section.items.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </select>
            <StylePicker
              label="Dashboard style"
              value={dashboard.styleId}
              onChange={(styleId) =>
                updateDashboards((current) => setDashboardStyle(current, id, styleId))
              }
            />
          </div>
          <div className="dashboard-page__tools" role="toolbar" aria-label="Dashboard actions">
            <button
              type="button"
              className={`button${editing ? ' button--active' : ''}`}
              aria-pressed={editing}
              onClick={() => updateDashboards((current) => setLocked(current, id, editing))}
            >
              {editing ? 'Lock layout' : 'Edit layout'}
            </button>
            <button type="button" className="button" onClick={() => setLibrary(true)}>
              Add widget
            </button>
            <button
              type="button"
              className="button button--quiet"
              onClick={() => open({ kind: 'new' }, 'Gaming')}
            >
              New
            </button>
            <button
              type="button"
              className="button button--quiet"
              onClick={() => open({ kind: 'rename' }, dashboard.name)}
            >
              Rename
            </button>
            <button
              type="button"
              className="button button--quiet"
              onClick={() => updateDashboards((current) => duplicateDashboard(current, id))}
            >
              Duplicate
            </button>
            <button
              type="button"
              className="button button--quiet"
              disabled={section.items.length <= 1}
              onClick={() => open({ kind: 'delete' })}
            >
              Delete
            </button>
            <button
              type="button"
              className="button button--quiet"
              onClick={() => open({ kind: 'reset' })}
            >
              Reset
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
              Export
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
              Import
            </button>
            {headerExtra}
          </div>
        </header>

        {dashboard.widgets.length === 0 ? (
          <div className="card dashboard-page__empty">
            <p className="card__muted">This dashboard is empty.</p>
            <button type="button" className="button" onClick={() => setLibrary(true)}>
              Add widget
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

        {(dialog?.kind === 'new' || dialog?.kind === 'rename') && (
          <ConfirmDialog
            title={dialog.kind === 'new' ? 'New dashboard' : `Rename “${dashboard.name}”`}
            body={[]}
            confirmLabel={dialog.kind === 'new' ? 'Create' : 'Rename'}
            tone="neutral"
            confirmDisabled={!name.trim()}
            onCancel={() => setDialog(null)}
            onConfirm={() => {
              updateDashboards((current) =>
                dialog.kind === 'new'
                  ? createDashboard(current, name)
                  : renameDashboard(current, id, name),
              );
              setDialog(null);
            }}
          >
            <input
              type="text"
              className="customize__text dialog__input"
              aria-label="Dashboard name"
              maxLength={40}
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </ConfirmDialog>
        )}

        {dialog?.kind === 'delete' && (
          <ConfirmDialog
            title={`Delete “${dashboard.name}”?`}
            body={['Its widgets and layout are removed. Templates and overlays are kept.']}
            confirmLabel="Delete dashboard"
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
            title={`Reset “${dashboard.name}”?`}
            body={['The default widgets replace the current ones. Your templates are kept.']}
            confirmLabel="Reset dashboard"
            tone="warning"
            onCancel={() => setDialog(null)}
            onConfirm={() => {
              updateDashboards((current) => resetDashboard(current, id));
              setDialog(null);
            }}
          />
        )}

        {dialog?.kind === 'export' && (
          <ConfirmDialog
            title="Export dashboard"
            body={[
              'A portable JSON description. It contains no hardware identifier, path or user name.',
            ]}
            confirmLabel="Copy to clipboard"
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
              aria-label="Exported JSON"
            />
          </ConfirmDialog>
        )}

        {dialog?.kind === 'import' && (
          <ConfirmDialog
            title="Import dashboard"
            body={['Paste a PULSE dashboard export. It is added as a new dashboard.']}
            confirmLabel="Import"
            tone="neutral"
            confirmDisabled={!importText.trim()}
            onCancel={() => setDialog(null)}
            onConfirm={() => {
              let parsed: unknown;
              try {
                parsed = JSON.parse(importText);
              } catch {
                setImportError('This is not valid JSON.');
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
              aria-label="Dashboard JSON"
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
