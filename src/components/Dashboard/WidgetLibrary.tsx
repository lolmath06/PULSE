import { useMemo, useState } from 'react';
import type { MetricDefinition } from '@/types/metrics';
import { useMetricCatalog } from '@/hooks/useMetricCatalog';
import type { Category } from '@/dashboard/library';
import {
  BLUEPRINTS,
  CATEGORIES,
  blueprintAvailability,
  createCustomWidget,
  createWidget,
} from '@/dashboard/library';
import type { WidgetInstance } from '@/dashboard/model';
import { persistableRef, useSourceRefs } from '@/dashboard/bindings';
import { deleteTemplate, instantiateTemplate, renameTemplate } from '@/dashboard/templates';
import { updateTemplates, useTemplates } from '@/dashboard/store';
import { liveRefusalReason } from '@/dashboard/liveRules';

/**
 * *Add widget*: categories of starting points built from the live catalog,
 * the user's own templates, and — under Custom — any numeric metric the
 * machine reports.
 *
 * An entry the machine cannot show is listed, disabled, with the backend's
 * reason; it is never offered as if it would work.
 */
export function WidgetLibrary({
  onAdd,
  onClose,
}: {
  readonly onAdd: (widget: WidgetInstance) => void;
  readonly onClose: () => void;
}) {
  const { catalog, status } = useMetricCatalog();
  const templates = useTemplates();
  const refs = useSourceRefs();
  const [category, setCategory] = useState<Category | 'Templates'>('CPU');
  const [query, setQuery] = useState('');
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);

  const entries = BLUEPRINTS.filter((blueprint) => blueprint.category === category);
  const numeric = useMemo(
    () =>
      catalog
        .filter(
          (definition) =>
            definition.valueType === 'number' &&
            (!definition.metric.sourceId.startsWith('process:') ||
              definition.metric.sourceId === 'process:system'),
        )
        .filter((definition) =>
          `${definition.displayName} ${definition.sourceLabel} ${definition.metric.key}`
            .toLowerCase()
            .includes(query.trim().toLowerCase()),
        )
        .slice(0, 200),
    [catalog, query],
  );

  const addCustom = (definition: MetricDefinition) => {
    const widget = createCustomWidget(definition);
    const ref = persistableRef(definition.metric.sourceId, refs);
    onAdd(
      ref
        ? {
            ...widget,
            bindings: widget.bindings.map((binding) => ({
              ...binding,
              source: { mode: 'fixed', ref },
            })),
          }
        : widget,
    );
  };

  return (
    <div className="dialog-backdrop" onMouseDown={onClose}>
      <div
        className="dialog library"
        role="dialog"
        aria-modal="true"
        aria-label="Add widget"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === 'Escape') onClose();
        }}
      >
        <header className="library__header">
          <h3 className="dialog__title">Add widget</h3>
          <button type="button" className="button button--quiet" onClick={onClose}>
            Close
          </button>
        </header>
        <div className="library__body">
          <nav className="library__categories" aria-label="Widget categories">
            {[...CATEGORIES, 'Templates' as const].map((name) => (
              <button
                key={name}
                type="button"
                className={`library__category${name === category ? ' library__category--active' : ''}`}
                aria-pressed={name === category}
                onClick={() => setCategory(name)}
              >
                {name}
              </button>
            ))}
          </nav>
          <div className="library__entries">
            {status === 'loading' && <p className="card__muted">Reading the metric catalog…</p>}

            {category !== 'Templates' &&
              entries.map((blueprint) => {
                const availability = blueprintAvailability(blueprint, catalog);
                const liveNote = blueprint.bindings
                  .map((b) => liveRefusalReason(b.key))
                  .find(Boolean);
                return (
                  <div key={blueprint.id} className="library__entry">
                    <div>
                      <p className="library__label">{blueprint.label}</p>
                      <p className="library__description">{blueprint.description}</p>
                      {!availability.ok && <p className="library__reason">{availability.reason}</p>}
                      {availability.ok && liveNote && <p className="library__reason">{liveNote}</p>}
                    </div>
                    <button
                      type="button"
                      className="button"
                      disabled={!availability.ok}
                      aria-label={`Add ${blueprint.label}`}
                      onClick={() => onAdd(createWidget(blueprint))}
                    >
                      Add
                    </button>
                  </div>
                );
              })}

            {category === 'Custom' && (
              <div className="library__custom">
                <p className="library__label">Any metric</p>
                <input
                  type="search"
                  className="customize__text"
                  placeholder="Search the metric catalog"
                  aria-label="Search metrics"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                />
                <ul className="library__metrics">
                  {numeric.map((definition) => {
                    const readable = definition.availability.status === 'available';
                    return (
                      <li key={`${definition.metric.key}@${definition.metric.sourceId}`}>
                        <button
                          type="button"
                          className="library__metric"
                          disabled={!readable}
                          title={readable ? definition.description : 'Not readable on this machine'}
                          onClick={() => addCustom(definition)}
                        >
                          <span>{definition.displayName}</span>
                          <span className="library__metric-source">{definition.sourceLabel}</span>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </div>
            )}

            {category === 'Templates' &&
              (templates.items.length === 0 ? (
                <p className="card__muted">
                  No template yet. Customize a widget and use “Save as template”.
                </p>
              ) : (
                templates.items.map((template) => (
                  <div key={template.id} className="library__entry">
                    {renaming?.id === template.id ? (
                      <input
                        type="text"
                        className="customize__text"
                        aria-label="Template name"
                        value={renaming.name}
                        autoFocus
                        onChange={(event) =>
                          setRenaming({ id: template.id, name: event.target.value })
                        }
                        onKeyDown={(event) => {
                          if (event.key === 'Enter') {
                            updateTemplates((section) =>
                              renameTemplate(section, template.id, renaming.name),
                            );
                            setRenaming(null);
                          }
                        }}
                      />
                    ) : (
                      <p className="library__label">{template.name}</p>
                    )}
                    <div className="customize__row">
                      <button
                        type="button"
                        className="button"
                        onClick={() => onAdd(instantiateTemplate(template))}
                      >
                        Add
                      </button>
                      <button
                        type="button"
                        className="button button--quiet"
                        onClick={() => setRenaming({ id: template.id, name: template.name })}
                      >
                        Rename
                      </button>
                      <button
                        type="button"
                        className="button button--quiet"
                        onClick={() =>
                          updateTemplates((section) => deleteTemplate(section, template.id))
                        }
                      >
                        Delete
                      </button>
                    </div>
                  </div>
                ))
              ))}
          </div>
        </div>
      </div>
    </div>
  );
}
