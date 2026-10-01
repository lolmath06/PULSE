import { useMemo } from 'react';
import type { WidgetInstance } from '@/dashboard/model';
import { GRID_COLUMNS } from '@/dashboard/model';
import type { DashboardTemplate } from '@/presets/dashboardTemplates';
import { DASHBOARD_TEMPLATES } from '@/presets/dashboardTemplates';
import { resolveLook } from '@/design/look';
import { useAppLook } from '@/design/hooks';
import { StyleScope } from '@/design/LookContext';
import { styleById } from '@/design/styles';
import { Icon } from '@/components/Icon';

function blockKind(widget: WidgetInstance): string {
  if (widget.kind === 'summary') return 'summary';
  if (widget.kind === 'group') return 'group';
  if (widget.kind === 'value') return 'value';
  return widget.visual.config.renderer === 'gauge'
    ? 'gauge'
    : widget.visual.config.renderer === 'bar'
      ? 'bar'
      : 'chart';
}

/** The template's own layout, in miniature and in its style. */
function Schematic({ template }: { readonly template: DashboardTemplate }) {
  const appLook = useAppLook();
  const widgets = useMemo(() => template.widgets(), [template]);
  const look = template.styleId ? resolveLook(template.styleId) : appLook;
  const rows = Math.max(1, ...widgets.map((w) => w.layout.y + w.layout.h));
  return (
    <StyleScope look={look} className="template-card__canvas">
      <span
        className="template-card__grid"
        style={{
          gridTemplateColumns: `repeat(${GRID_COLUMNS}, 1fr)`,
          gridTemplateRows: `repeat(${rows}, 1fr)`,
        }}
        aria-hidden="true"
      >
        {widgets.map((widget) => (
          <span
            key={widget.id}
            className={`template-card__block template-card__block--${blockKind(widget)}`}
            style={{
              gridColumn: `${widget.layout.x + 1} / span ${widget.layout.w}`,
              gridRow: `${widget.layout.y + 1} / span ${widget.layout.h}`,
            }}
          >
            <span className="template-card__glyph" />
          </span>
        ))}
      </span>
    </StyleScope>
  );
}

export function TemplateCard({
  template,
  onUse,
  actionLabel = 'Create dashboard',
}: {
  readonly template: DashboardTemplate;
  readonly onUse: () => void;
  readonly actionLabel?: string;
}) {
  return (
    <article
      className={`template-card${template.featured ? ' template-card--featured' : ''}`}
      aria-label={`${template.name} template`}
    >
      <Schematic template={template} />
      <div className="template-card__body">
        <div className="template-card__title">
          <h3>
            {template.featured && <Icon name="sparkles" className="template-card__star" />}
            {template.name}
          </h3>
          <span className="chip chip--style">
            {template.styleId ? styleById(template.styleId).name : 'App style'}
          </span>
        </div>
        <p className="template-card__description">{template.description}</p>
        <button type="button" className="button template-card__use" onClick={onUse}>
          {actionLabel} <Icon name="arrowRight" />
        </button>
      </div>
    </article>
  );
}

/** Every built-in template (or `only` these), as cards. */
export function TemplateGallery({
  onUse,
  only,
}: {
  readonly onUse: (template: DashboardTemplate) => void;
  readonly only?: readonly string[];
}) {
  const templates = only
    ? DASHBOARD_TEMPLATES.filter((template) => only.includes(template.id))
    : DASHBOARD_TEMPLATES;
  return (
    <div className="template-gallery">
      {templates.map((template) => (
        <TemplateCard key={template.id} template={template} onUse={() => onUse(template)} />
      ))}
    </div>
  );
}
