import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { matchesMetricQuery, metricDescription, metricName, sourceLabel } from '@/i18n/metrics';
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
  const { t, i18n } = useTranslation();
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
        .filter((definition) => matchesMetricQuery(definition, query))
        .slice(0, 200),
    // The language is a dependency: names are matched as they are shown.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [catalog, query, i18n.language],
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
        aria-label={t('dashboard.addWidget')}
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === 'Escape') onClose();
        }}
      >
        <header className="library__header">
          <h3 className="dialog__title">{t('dashboard.addWidget')}</h3>
          <button type="button" className="button button--quiet" onClick={onClose}>
            {t('common.close')}
          </button>
        </header>
        <div className="library__body">
          <nav className="library__categories" aria-label={t('library.categoriesLabel')}>
            {[...CATEGORIES, 'Templates' as const].map((name) => (
              <button
                key={name}
                type="button"
                className={`library__category${name === category ? ' library__category--active' : ''}`}
                aria-pressed={name === category}
                onClick={() => setCategory(name)}
              >
                {t(`library.categories.${name}`)}
              </button>
            ))}
          </nav>
          <div className="library__entries">
            {status === 'loading' && <p className="card__muted">{t('library.loading')}</p>}

            {category !== 'Templates' &&
              entries.map((blueprint) => {
                const availability = blueprintAvailability(blueprint, catalog);
                const liveNote = blueprint.bindings
                  .map((b) => liveRefusalReason(b.key))
                  .find(Boolean);
                return (
                  <div key={blueprint.id} className="library__entry">
                    <div>
                      <p className="library__label">
                        {t(`library.blueprints.${blueprint.id}.label`)}
                      </p>
                      <p className="library__description">
                        {t(`library.blueprints.${blueprint.id}.description`)}
                      </p>
                      {!availability.ok && <p className="library__reason">{availability.reason}</p>}
                      {availability.ok && liveNote && <p className="library__reason">{liveNote}</p>}
                    </div>
                    <button
                      type="button"
                      className="button"
                      disabled={!availability.ok}
                      aria-label={t('library.addNamed', {
                        name: t(`library.blueprints.${blueprint.id}.label`),
                      })}
                      onClick={() => onAdd(createWidget(blueprint))}
                    >
                      {t('common.add')}
                    </button>
                  </div>
                );
              })}

            {category === 'Custom' && (
              <div className="library__custom">
                <p className="library__label">{t('library.anyMetric')}</p>
                <input
                  type="search"
                  className="customize__text"
                  placeholder={t('library.searchPlaceholder')}
                  aria-label={t('library.searchLabel')}
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
                          title={
                            readable ? metricDescription(definition) : t('library.notReadable')
                          }
                          onClick={() => addCustom(definition)}
                        >
                          <span>{metricName(definition)}</span>
                          <span className="library__metric-source">{sourceLabel(definition)}</span>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </div>
            )}

            {category === 'Templates' &&
              (templates.items.length === 0 ? (
                <p className="card__muted">{t('library.noTemplates')}</p>
              ) : (
                templates.items.map((template) => (
                  <div key={template.id} className="library__entry">
                    {renaming?.id === template.id ? (
                      <input
                        type="text"
                        className="customize__text"
                        aria-label={t('library.templateName')}
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
                        {t('common.add')}
                      </button>
                      <button
                        type="button"
                        className="button button--quiet"
                        onClick={() => setRenaming({ id: template.id, name: template.name })}
                      >
                        {t('common.rename')}
                      </button>
                      <button
                        type="button"
                        className="button button--quiet"
                        onClick={() =>
                          updateTemplates((section) => deleteTemplate(section, template.id))
                        }
                      >
                        {t('common.delete')}
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
