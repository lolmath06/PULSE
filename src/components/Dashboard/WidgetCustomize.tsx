import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { sourceLabel } from '@/i18n/metrics';
import { formatFixed } from '@/i18n/format';
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
import { bindingLabelText, widgetTitle, widgetTitleText } from '@/dashboard/geometry';
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
  const { t } = useTranslation();
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
      <Section title={t('customize.widget.section')} open>
        <Field label={t('customize.widget.title')}>
          <input
            type="text"
            className="customize__text"
            aria-label={t('customize.widget.titleLabel')}
            placeholder={widgetTitle({ ...widget, title: null, titleKey: undefined })}
            maxLength={48}
            value={widgetTitleText(widget) ?? ''}
            onChange={(event) =>
              // Typed by the user: their words, kept as typed in every language.
              onChange(({ titleKey: _builtIn, ...current }) => ({
                ...current,
                title: event.target.value.trim() ? event.target.value : null,
              }))
            }
          />
        </Field>
        <div className="customize__toggles">
          <Toggle
            text={t('customize.widget.showTitle')}
            checked={widget.frame.showTitle}
            onChange={(showTitle) => setFrame({ showTitle })}
          />
          <Toggle
            text={t('customize.widget.background')}
            checked={widget.frame.background !== null}
            onChange={(on) => setFrame({ background: on ? '#14181d' : null })}
          />
        </div>
        {widget.frame.background !== null && (
          <ColorField
            text={t('customize.widget.background')}
            value={widget.frame.background}
            onChange={(background) => setFrame({ background })}
          />
        )}
        <Field label={t('customize.widget.opacity')}>
          <Slider
            ariaLabel={t('customize.widget.backgroundOpacity')}
            value={widget.frame.opacity}
            min={0}
            max={1}
            step={0.05}
            format={(value) => `${formatFixed(value * 100, 0)} %`}
            onChange={(opacity) => setFrame({ opacity })}
          />
        </Field>
        <Field label={t('customize.padding')}>
          <Slider
            ariaLabel={t('customize.widget.padding')}
            value={widget.frame.padding}
            min={0}
            max={24}
            step={1}
            format={(value) => `${value} px`}
            onChange={(padding) => setFrame({ padding })}
          />
        </Field>
        <Field label={t('customize.widget.border')}>
          <Choice
            ariaLabel={t('customize.widget.border')}
            options={['none', 'thin'] as const}
            value={widget.frame.border}
            labelFor={(value) => t(`customize.borders.${value}`)}
            onChange={(border) => setFrame({ border })}
          />
        </Field>
        <Field label={t('customize.widget.corners')}>
          <Slider
            ariaLabel={t('customize.widget.cornerRadius')}
            value={widget.frame.radius}
            min={0}
            max={24}
            step={1}
            format={(value) => `${value} px`}
            onChange={(radius) => setFrame({ radius })}
          />
        </Field>
      </Section>

      <Section title={t('customize.metrics.section')} open>
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
                aria-label={t('customize.metrics.label', { index: index + 1 })}
                placeholder={entry.label}
                maxLength={24}
                value={bindingLabelText(entry.binding) ?? ''}
                onChange={(event) =>
                  onChange((current) => ({
                    ...current,
                    bindings: current.bindings.map((binding, i) => {
                      if (i !== index) return binding;
                      // The user's own label from now on, kept as typed.
                      const { labelKey: _builtIn, ...rest } = binding;
                      return { ...rest, label: event.target.value || null };
                    }),
                  }))
                }
              />
              {candidates.length > 1 || !entry.ok ? (
                <select
                  className="history-panel__select"
                  aria-label={t('customize.metrics.source', { index: index + 1 })}
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
                      ? t('customize.metrics.automaticNamed', {
                          name: sourceLabel(entry.definition),
                        })
                      : t('customize.metrics.automatic')}
                  </option>
                  {!entry.ok && selected !== 'auto' && (
                    <option value={selected}>{t('metrics.sourceUnavailable')}</option>
                  )}
                  {candidates.map((definition) => {
                    const ref = persistableRef(definition.metric.sourceId, refs);
                    return ref ? (
                      <option key={definition.metric.sourceId} value={ref}>
                        {sourceLabel(definition)}
                      </option>
                    ) : null;
                  })}
                </select>
              ) : null}
              {!entry.ok && <p className="customize__error">{entry.reason}</p>}
              {entry.ok && entry.auto && candidates.length > 1 && (
                <p className="customize__hint">{t('customize.metrics.autoHint')}</p>
              )}
            </div>
          );
        })}
        {(widget.kind === 'group' || widget.kind === 'summary') && (
          <>
            {widget.kind === 'group' && (
              <Field label={t('customize.layout')}>
                <Choice
                  ariaLabel={t('customize.metrics.groupLayout')}
                  options={['rows', 'inline'] as const}
                  value={widget.group.orientation}
                  labelFor={(value) => t(`customize.groupLayouts.${value}`)}
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
              text={t('customize.metrics.tinyTrends')}
              checked={widget.group.sparklines}
              onChange={(sparklines) =>
                onChange((current) => ({ ...current, group: { ...current.group, sparklines } }))
              }
            />
          </>
        )}
      </Section>

      <Section title={t('customize.data.section')}>
        <Field
          label={t('customize.data.source')}
          hint={
            effectiveDataMode(widget) === 'live'
              ? t('customize.data.nowLive')
              : t('customize.data.nowHistory', {
                  range: t(`history.ranges.${widget.visual.range}`),
                })
          }
        >
          <Choice
            ariaLabel={t('customize.data.source')}
            options={DATA_MODES}
            value={widget.dataMode}
            labelFor={(value) => t(`customize.data.modes.${value}`)}
            onChange={(dataMode) => onChange((current) => ({ ...current, dataMode }))}
          />
        </Field>
      </Section>

      {onSaveTemplate && (
        <Section title={t('customize.template.section')}>
          <div className="customize__row">
            <input
              type="text"
              className="customize__text"
              placeholder={t('customize.template.placeholder')}
              aria-label={t('library.templateName')}
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
              {t('customize.template.save')}
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
