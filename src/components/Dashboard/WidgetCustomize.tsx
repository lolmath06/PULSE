import { useMemo, useState } from 'react';
import type { WidgetInstance } from '@/dashboard/model';
import { DATA_MODES } from '@/dashboard/model';
import type { DeepPartial, VisualizationConfig } from '@/visualization/config';
import { mergeConfig, normalizeConfig } from '@/visualization/config';
import { applyPreset as applyPresetTo, resolvePreset } from '@/visualization/presets';
import type { ChartVisualization } from '@/visualization/store';
import {
  deleteCustomPreset,
  lookupPreset,
  saveCustomPreset,
  usePresets,
} from '@/visualization/store';
import {
  Choice,
  ColorField,
  CustomizePanel,
  Field,
  Section,
  Slider,
  Toggle,
} from '@/visualization/CustomizePanel';
import { useMetricCatalog } from '@/hooks/useMetricCatalog';
import { persistableRef, useSourceRefs } from '@/dashboard/bindings';
import { chartDefaultsFor } from '@/dashboard/metricInfo';
import { effectiveDataMode, useWidgetData } from '@/dashboard/widgetData';
import { widgetTitle } from '@/dashboard/geometry';
import { useResolvedBindings } from '@/components/Dashboard/useWidget';

/**
 * Customize for a widget: the Phase 10 panel, unchanged, with the widget's
 * own options on top — title, frame, metrics and sources, data mode,
 * template. Every change applies live and is saved with the widget.
 */
export function WidgetCustomize({
  widget,
  onChange,
  onClose,
  onSaveTemplate,
}: {
  readonly widget: WidgetInstance;
  readonly onChange: (change: (widget: WidgetInstance) => WidgetInstance) => void;
  readonly onClose: () => void;
  readonly onSaveTemplate?: (name: string) => void;
}) {
  const presets = usePresets();
  const resolved = useResolvedBindings(widget);
  const { data, meta } = useWidgetData(widget, resolved, true);
  const { catalog } = useMetricCatalog();
  const refs = useSourceRefs();
  const [templateName, setTemplateName] = useState('');
  const defaults = useMemo<DeepPartial<VisualizationConfig>>(
    () => chartDefaultsFor(widget.bindings[0]?.key ?? 'cpu.usage.total'),
    [widget.bindings],
  );

  const setVisual = (change: (visual: WidgetInstance['visual']) => WidgetInstance['visual']) =>
    onChange((current) => ({ ...current, visual: change(current.visual) }));

  const chart: ChartVisualization = {
    ...widget.visual,
    presets,
    update: (patch) =>
      setVisual((visual) => ({
        ...visual,
        modified: true,
        config: normalizeConfig(mergeConfig(visual.config, patch), visual.config),
      })),
    setRange: (range) => setVisual((visual) => ({ ...visual, range })),
    applyPreset: (presetId) => {
      const preset = lookupPreset(presetId);
      if (!preset) return;
      setVisual((visual) => ({
        ...visual,
        presetId,
        modified: false,
        config: applyPresetTo(preset, defaults, visual.config),
      }));
    },
    reset: () =>
      setVisual((visual) => ({
        ...visual,
        modified: false,
        config: resolvePreset(lookupPreset(visual.presetId), defaults),
      })),
    saveAsPreset: (name) => {
      const id = saveCustomPreset(name, widget.visual.config);
      if (id) setVisual((visual) => ({ ...visual, presetId: id, modified: false }));
    },
    deletePreset: deleteCustomPreset,
  };

  const setFrame = (patch: Partial<WidgetInstance['frame']>) =>
    onChange((current) => ({ ...current, frame: { ...current.frame, ...patch } }));

  const extra = (
    <>
      <Section title="Widget" open>
        <Field label="Title">
          <input
            type="text"
            className="customize__text"
            aria-label="Widget title"
            placeholder={widgetTitle({ ...widget, title: null })}
            maxLength={48}
            value={widget.title ?? ''}
            onChange={(event) =>
              onChange((current) => ({
                ...current,
                title: event.target.value.trim() ? event.target.value : null,
              }))
            }
          />
        </Field>
        <div className="customize__toggles">
          <Toggle
            text="Show title"
            checked={widget.frame.showTitle}
            onChange={(showTitle) => setFrame({ showTitle })}
          />
          <Toggle
            text="Widget background"
            checked={widget.frame.background !== null}
            onChange={(on) => setFrame({ background: on ? '#14181d' : null })}
          />
        </div>
        {widget.frame.background !== null && (
          <ColorField
            text="Widget background"
            value={widget.frame.background}
            onChange={(background) => setFrame({ background })}
          />
        )}
        <Field label="Widget opacity">
          <Slider
            ariaLabel="Widget background opacity"
            value={widget.frame.opacity}
            min={0}
            max={1}
            step={0.05}
            format={(value) => `${Math.round(value * 100)} %`}
            onChange={(opacity) => setFrame({ opacity })}
          />
        </Field>
        <Field label="Padding">
          <Slider
            ariaLabel="Widget padding"
            value={widget.frame.padding}
            min={0}
            max={24}
            step={1}
            format={(value) => `${value} px`}
            onChange={(padding) => setFrame({ padding })}
          />
        </Field>
        <Field label="Widget border">
          <Choice
            ariaLabel="Widget border"
            options={['none', 'thin'] as const}
            value={widget.frame.border}
            labelFor={(value) => (value === 'none' ? 'None' : 'Thin')}
            onChange={(border) => setFrame({ border })}
          />
        </Field>
        <Field label="Widget corners">
          <Slider
            ariaLabel="Widget corner radius"
            value={widget.frame.radius}
            min={0}
            max={24}
            step={1}
            format={(value) => `${value} px`}
            onChange={(radius) => setFrame({ radius })}
          />
        </Field>
      </Section>

      <Section title="Metrics & source" open>
        {resolved.map((entry, index) => {
          const candidates = catalog.filter(
            (definition) => definition.metric.key === entry.binding.key,
          );
          const selected =
            entry.binding.source.mode === 'fixed' ? entry.binding.source.ref : 'auto';
          return (
            <div key={index} className="customize__binding">
              <input
                type="text"
                className="customize__text"
                aria-label={`Label of metric ${index + 1}`}
                placeholder={entry.label}
                maxLength={24}
                value={entry.binding.label ?? ''}
                onChange={(event) =>
                  onChange((current) => ({
                    ...current,
                    bindings: current.bindings.map((binding, i) =>
                      i === index ? { ...binding, label: event.target.value || null } : binding,
                    ),
                  }))
                }
              />
              {candidates.length > 1 || !entry.ok ? (
                <select
                  className="history-panel__select"
                  aria-label={`Source of metric ${index + 1}`}
                  value={selected}
                  onChange={(event) =>
                    onChange((current) => ({
                      ...current,
                      bindings: current.bindings.map((binding, i) =>
                        i === index
                          ? {
                              ...binding,
                              source:
                                event.target.value === 'auto'
                                  ? { mode: 'auto' }
                                  : { mode: 'fixed', ref: event.target.value },
                            }
                          : binding,
                      ),
                    }))
                  }
                >
                  <option value="auto">
                    {entry.ok && entry.auto
                      ? `Automatic — ${entry.definition.sourceLabel}`
                      : 'Automatic'}
                  </option>
                  {!entry.ok && selected !== 'auto' && (
                    <option value={selected}>Source unavailable</option>
                  )}
                  {candidates.map((definition) => {
                    const ref = persistableRef(definition.metric.sourceId, refs);
                    return ref ? (
                      <option key={definition.metric.sourceId} value={ref}>
                        {definition.sourceLabel}
                      </option>
                    ) : null;
                  })}
                </select>
              ) : null}
              {!entry.ok && <p className="customize__error">{entry.reason}</p>}
              {entry.ok && entry.auto && candidates.length > 1 && (
                <p className="customize__hint">
                  Chosen automatically — a reasonable default, not a claim about which one matters.
                </p>
              )}
            </div>
          );
        })}
        {(widget.kind === 'group' || widget.kind === 'summary') && (
          <>
            {widget.kind === 'group' && (
              <Field label="Layout">
                <Choice
                  ariaLabel="Group layout"
                  options={['rows', 'inline'] as const}
                  value={widget.group.orientation}
                  labelFor={(value) => (value === 'rows' ? 'Rows' : 'Inline')}
                  onChange={(orientation) =>
                    onChange((current) => ({
                      ...current,
                      group: { ...current.group, orientation },
                    }))
                  }
                />
              </Field>
            )}
            <Toggle
              text="Tiny trends"
              checked={widget.group.sparklines}
              onChange={(sparklines) =>
                onChange((current) => ({ ...current, group: { ...current.group, sparklines } }))
              }
            />
          </>
        )}
      </Section>

      <Section title="Data">
        <Field
          label="Data source"
          hint={`Now: ${effectiveDataMode(widget) === 'live' ? 'live, 1 s, last 5 minutes in memory' : `history, ${widget.visual.range}`}.`}
        >
          <Choice
            ariaLabel="Data source"
            options={DATA_MODES}
            value={widget.dataMode}
            labelFor={(value) => ({ auto: 'Automatic', history: 'History', live: 'Live' })[value]}
            onChange={(dataMode) => onChange((current) => ({ ...current, dataMode }))}
          />
        </Field>
      </Section>

      {onSaveTemplate && (
        <Section title="Template">
          <div className="customize__row">
            <input
              type="text"
              className="customize__text"
              placeholder="Template name, e.g. My CPU tiny"
              aria-label="Template name"
              maxLength={40}
              value={templateName}
              onChange={(event) => setTemplateName(event.target.value)}
            />
            <button
              type="button"
              className="button"
              disabled={!templateName.trim()}
              onClick={() => {
                onSaveTemplate(templateName);
                setTemplateName('');
              }}
            >
              Save as template
            </button>
          </div>
        </Section>
      )}
    </>
  );

  return (
    <CustomizePanel
      title={widgetTitle(widget)}
      chart={chart}
      data={data}
      meta={meta}
      onClose={onClose}
      extra={extra}
    />
  );
}
