import { useEffect, useState } from 'react';
import type { ChangeEvent, ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { t as translate } from '@/i18n/i18n';
import { formatFixed } from '@/i18n/format';
import type { HistoryRange } from '@/types/history';
import { HISTORY_RANGES } from '@/types/history';
import type {
  DeepPartial,
  RendererKind,
  ThresholdBand,
  VisualizationConfig,
} from '@/visualization/config';
import {
  BACKGROUND_MODES,
  BORDER_STYLES,
  COLOR_MODES,
  CURVE_STYLES,
  FILL_MODES,
  FONT_WEIGHTS,
  GAUGE_ARCS,
  LIMITS,
  POINT_STYLES,
  SHADOW_STYLES,
  SIZE_PRESETS,
  SMOOTHING_LEVELS,
  isHexColor,
  scaleError,
  sortThresholds,
} from '@/visualization/config';
import { PERCENT_THRESHOLDS } from '@/visualization/color';
import { MetricVisualization } from '@/visualization/MetricVisualization';
import { RENDERERS, rendererSupport } from '@/visualization/registry';
import { presetDescription, presetName as nameOfPreset } from '@/visualization/presets';
import type { ChartVisualization } from '@/visualization/store';
import type { VisualizationData, VisualizationMeta } from '@/visualization/types';

/**
 * The one Customize panel every chart uses.
 *
 * Every control applies immediately — the chart on the page and the preview at
 * the top of the panel both update as you drag — and is persisted as it
 * changes. There is no Save button to forget; *Reset* returns to the current
 * preset, and choosing a preset is only ever a starting point.
 */
export interface CustomizePanelProps {
  readonly title: string;
  readonly chart: ChartVisualization;
  readonly data: VisualizationData;
  readonly meta: VisualizationMeta;
  readonly onClose: () => void;
  /**
   * Options that belong to the host rather than to the visualization — a
   * widget's title, frame, metrics and data source. Shown first; every
   * visual option stays in the sections below, never duplicated.
   */
  readonly extra?: ReactNode;
}

/** An option value's words: `viz.options.<value>`; the value itself when unknown. */
const label = (value: string) => {
  const key = `viz.options.${value}`;
  const text = translate(key);
  return text === key ? value : text;
};

export function Section({
  title,
  children,
  open = false,
}: {
  title: string;
  children: ReactNode;
  open?: boolean;
}) {
  return (
    <details className="customize__section" open={open}>
      <summary className="customize__section-title">{title}</summary>
      <div className="customize__section-body">{children}</div>
    </details>
  );
}

export function Field({
  label: text,
  children,
  hint,
}: {
  label: string;
  children: ReactNode;
  hint?: string;
}) {
  return (
    <div className="customize__field">
      <span className="customize__label">{text}</span>
      <div className="customize__control">{children}</div>
      {hint && <p className="customize__hint">{hint}</p>}
    </div>
  );
}

export function Choice<T extends string>({
  options,
  value,
  onChange,
  ariaLabel,
  labelFor = label,
}: {
  options: readonly T[];
  value: T;
  onChange: (value: T) => void;
  ariaLabel: string;
  labelFor?: (value: T) => string;
}) {
  return (
    <div className="segmented" role="group" aria-label={ariaLabel}>
      {options.map((option) => (
        <button
          key={option}
          type="button"
          className={`segmented__option${option === value ? ' segmented__option--active' : ''}`}
          aria-pressed={option === value}
          onClick={() => onChange(option)}
        >
          {labelFor(option)}
        </button>
      ))}
    </div>
  );
}

export function Toggle({
  text,
  checked,
  onChange,
  disabled,
}: {
  text: string;
  checked: boolean;
  onChange: (value: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <label className={`customize__toggle${disabled ? ' customize__toggle--disabled' : ''}`}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
      {text}
    </label>
  );
}

export function Slider({
  value,
  min,
  max,
  step,
  onChange,
  format,
  ariaLabel,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
  format?: (value: number) => string;
  ariaLabel: string;
}) {
  return (
    <span className="customize__slider">
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        aria-label={ariaLabel}
        onChange={(event) => onChange(Number(event.target.value))}
      />
      <output className="customize__slider-value">{format ? format(value) : value}</output>
    </span>
  );
}

/** A real colour picker (the platform's own) plus an exact hex field. */
export function ColorField({
  text,
  value,
  onChange,
}: {
  text: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  const [shown, setShown] = useState(value);
  if (shown !== value) {
    // The value changed from outside (a preset, Reset): show it.
    setShown(value);
    setDraft(value);
  }

  return (
    <div className="customize__color">
      <span className="customize__color-name">{text}</span>
      <input
        type="color"
        className="customize__color-picker"
        value={value}
        aria-label={translate('viz.ui.colourOf', { name: text })}
        onChange={(event: ChangeEvent<HTMLInputElement>) => onChange(event.target.value)}
      />
      <input
        type="text"
        className="customize__color-hex mono"
        value={draft}
        aria-label={translate('viz.ui.hexOf', { name: text })}
        spellCheck={false}
        maxLength={7}
        onChange={(event) => {
          setDraft(event.target.value);
          if (isHexColor(event.target.value)) onChange(event.target.value.toLowerCase());
        }}
      />
    </div>
  );
}

const pct = (value: number) => `${formatFixed(value * 100, 0)} %`;

export function CustomizePanel({ title, chart, data, meta, onClose, extra }: CustomizePanelProps) {
  const { t } = useTranslation();
  const { config, update } = chart;
  const [presetName, setPresetName] = useState('');
  const [scaleDraft, setScaleDraft] = useState({
    min: config.scale.min === null ? '' : String(config.scale.min),
    max: config.scale.max === null ? '' : String(config.scale.max),
  });

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const preset = chart.presets.find((entry) => entry.id === chart.presetId);
  const presetStatus = chart.modified
    ? t('viz.ui.customFrom', { name: preset ? nameOfPreset(preset) : t('viz.ui.presetWord') })
    : preset
      ? nameOfPreset(preset)
      : '';

  /** Changing any colour means the user wants their own colours now. */
  const setColor = (patch: DeepPartial<VisualizationConfig['colors']>) =>
    update({
      colors: { ...patch, mode: config.colors.mode === 'theme' ? 'manual' : config.colors.mode },
    });

  const parsedMin = scaleDraft.min.trim() === '' ? null : Number(scaleDraft.min);
  const parsedMax = scaleDraft.max.trim() === '' ? null : Number(scaleDraft.max);
  const draftError = scaleError(parsedMin, parsedMax);

  const setThresholds = (bands: readonly ThresholdBand[]) =>
    update({ colors: { thresholds: sortThresholds(bands), mode: 'threshold' } });

  const compactLocks = config.display.compact || config.renderer === 'sparkline';

  return (
    <aside
      className="customize"
      role="dialog"
      aria-label={t('viz.ui.customizeNamed', { name: title })}
    >
      <header className="customize__header">
        <div>
          <p className="customize__eyebrow">{t('common.customize')}</p>
          <h3 className="customize__title">{title}</h3>
        </div>
        <button type="button" className="button button--quiet" onClick={onClose}>
          {t('common.close')}
        </button>
      </header>

      <div className="customize__preview" aria-label={t('viz.ui.preview')}>
        <MetricVisualization data={data} meta={meta} config={config} width={332} height={150} />
      </div>

      <div className="customize__scroll">
        {extra}
        <Section title={t('viz.ui.preset')} open>
          <p className="customize__preset-status" aria-live="polite">
            {presetStatus}
          </p>
          <div className="customize__presets">
            {chart.presets.map((entry) => (
              <button
                key={entry.id}
                type="button"
                title={presetDescription(entry)}
                className={`customize__preset${entry.id === chart.presetId ? ' customize__preset--active' : ''}`}
                onClick={() => chart.applyPreset(entry.id)}
              >
                {nameOfPreset(entry)}
              </button>
            ))}
          </div>
          <div className="customize__row">
            <button type="button" className="button" onClick={chart.reset}>
              {t('viz.ui.resetVisualization')}
            </button>
            {preset?.custom && (
              <button
                type="button"
                className="button button--quiet"
                onClick={() => chart.deletePreset(preset.id)}
              >
                {t('viz.ui.deletePreset')}
              </button>
            )}
          </div>
          <div className="customize__row">
            <input
              type="text"
              className="customize__text"
              placeholder={t('viz.ui.presetName')}
              aria-label={t('viz.ui.newPresetName')}
              value={presetName}
              maxLength={40}
              onChange={(event) => setPresetName(event.target.value)}
            />
            <button
              type="button"
              className="button"
              disabled={presetName.trim() === ''}
              onClick={() => {
                chart.saveAsPreset(presetName);
                setPresetName('');
              }}
            >
              {t('viz.ui.saveAsPreset')}
            </button>
          </div>
        </Section>

        <Section title={t('viz.ui.visualization')} open>
          <div
            className="customize__renderers"
            role="group"
            aria-label={t('viz.ui.visualizationType')}
          >
            {RENDERERS.map((renderer) => {
              const support = rendererSupport(renderer.kind, meta, config);
              return (
                <button
                  key={renderer.kind}
                  type="button"
                  className={`customize__renderer${renderer.kind === config.renderer ? ' customize__renderer--active' : ''}`}
                  aria-pressed={renderer.kind === config.renderer}
                  disabled={!support.ok}
                  title={
                    support.ok
                      ? t(`visualization.renderers.${renderer.kind}.description`)
                      : support.reason
                  }
                  onClick={() => update({ renderer: renderer.kind as RendererKind })}
                >
                  {t(`visualization.renderers.${renderer.kind}.label`)}
                </button>
              );
            })}
          </div>
          {!rendererSupport('gauge', meta, config).ok && (
            <p className="customize__hint">{t('viz.ui.gaugeUnavailable')}</p>
          )}
          <Field label={t('viz.ui.timeRange')}>
            <Choice
              ariaLabel={t('viz.ui.timeRange')}
              options={HISTORY_RANGES}
              value={chart.range}
              labelFor={(value) => t(`history.ranges.${value}`)}
              onChange={(value: HistoryRange) => chart.setRange(value)}
            />
          </Field>
          <Field label={t('viz.ui.size')}>
            <Choice
              ariaLabel={t('viz.ui.size')}
              options={SIZE_PRESETS}
              value={config.size.preset}
              onChange={(preset) => update({ size: { preset } })}
            />
          </Field>
          {config.size.preset === 'custom' && (
            <div className="customize__row">
              <label className="customize__inline">
                {t('viz.ui.height')}
                <input
                  type="number"
                  min={LIMITS.height.min}
                  max={LIMITS.height.max}
                  value={config.size.height ?? 180}
                  onChange={(event) => update({ size: { height: Number(event.target.value) } })}
                />
              </label>
              <label className="customize__inline">
                {t('viz.ui.width')}
                <input
                  type="number"
                  min={LIMITS.width.min}
                  max={LIMITS.width.max}
                  placeholder={t('viz.ui.fillPlaceholder')}
                  value={config.size.width ?? ''}
                  onChange={(event) =>
                    update({
                      size: {
                        width: event.target.value === '' ? null : Number(event.target.value),
                      },
                    })
                  }
                />
              </label>
            </div>
          )}
          <div className="customize__toggles">
            <Toggle
              text={t('viz.ui.compactMode')}
              checked={config.display.compact}
              onChange={(compact) => update({ display: { compact } })}
            />
          </div>
        </Section>

        <Section title={t('viz.ui.lineFill')}>
          <Field label={t('viz.ui.lineWidth')}>
            <Slider
              ariaLabel={t('viz.ui.lineWidth')}
              value={config.line.width}
              min={LIMITS.lineWidth.min}
              max={LIMITS.lineWidth.max}
              step={0.25}
              format={(value) => `${value} px`}
              onChange={(width) => update({ line: { width } })}
            />
          </Field>
          <div className="customize__row">
            {(
              [
                ['thin', 1],
                ['medium', 2],
                ['thick', 3.5],
              ] as const
            ).map(([id, width]) => (
              <button
                key={id}
                type="button"
                className="button button--quiet"
                onClick={() => update({ line: { width } })}
              >
                {t(`viz.lineWidths.${id}`)}
              </button>
            ))}
          </div>
          <Field label={t('viz.ui.curve')}>
            <Choice
              ariaLabel={t('viz.ui.curve')}
              options={CURVE_STYLES}
              value={config.line.curve}
              onChange={(curve) => update({ line: { curve } })}
            />
          </Field>
          <Field label={t('viz.ui.points')}>
            <Choice
              ariaLabel={t('viz.ui.pointMarkers')}
              options={POINT_STYLES}
              value={config.line.points}
              labelFor={(value) => label(value)}
              onChange={(points) => update({ line: { points } })}
            />
          </Field>
          <Field label={t('viz.ui.fill')}>
            <Choice
              ariaLabel={t('viz.ui.fill')}
              options={FILL_MODES}
              value={config.fill.mode}
              onChange={(mode) => update({ fill: { mode } })}
            />
          </Field>
          <Field label={t('viz.ui.fillOpacity')}>
            <Slider
              ariaLabel={t('viz.ui.fillOpacity')}
              value={config.fill.opacity}
              min={0}
              max={1}
              step={0.05}
              format={pct}
              onChange={(opacity) => update({ fill: { opacity } })}
            />
          </Field>
          <Field label={t('viz.ui.smoothing')} hint={t('viz.ui.smoothingHint')}>
            <Choice
              ariaLabel={t('viz.ui.smoothing')}
              options={SMOOTHING_LEVELS}
              value={config.smoothing}
              onChange={(smoothing) => update({ smoothing })}
            />
          </Field>
        </Section>

        <Section title={t('viz.ui.colors')}>
          <Field label={t('viz.ui.colorMode')}>
            <Choice
              ariaLabel={t('viz.ui.colorMode')}
              options={COLOR_MODES}
              value={config.colors.mode}
              onChange={(mode) =>
                update({
                  colors: {
                    mode,
                    thresholds:
                      mode === 'threshold' && config.colors.thresholds.length === 0 && meta.bounds
                        ? PERCENT_THRESHOLDS
                        : config.colors.thresholds,
                  },
                })
              }
            />
          </Field>
          {config.colors.mode === 'theme' && (
            <p className="customize__hint">{t('viz.ui.followingTheme')}</p>
          )}
          <ColorField
            text={t('viz.ui.primary')}
            value={config.colors.primary}
            onChange={(primary) => setColor({ primary })}
          />
          <ColorField
            text={t('viz.ui.secondary')}
            value={config.colors.secondary}
            onChange={(secondary) => setColor({ secondary })}
          />
          <ColorField
            text={t('viz.ui.text')}
            value={config.colors.text}
            onChange={(text) => setColor({ text })}
          />
          <ColorField
            text={t('viz.ui.grid')}
            value={config.colors.grid}
            onChange={(grid) => setColor({ grid })}
          />
          <ColorField
            text={t('viz.ui.background')}
            value={config.colors.background}
            onChange={(background) => setColor({ background })}
          />
          <ColorField
            text={t('viz.ui.border')}
            value={config.colors.border}
            onChange={(border) => setColor({ border })}
          />
          <ColorField
            text={t('viz.ui.fill')}
            value={config.colors.fill ?? config.colors.primary}
            onChange={(fill) => setColor({ fill })}
          />
          <Toggle
            text={t('viz.ui.fillFollowsLine')}
            checked={config.colors.fill === null}
            onChange={(follow) => setColor({ fill: follow ? null : config.colors.primary })}
          />
          <ColorField
            text={t('viz.ui.gradientStart')}
            value={config.colors.gradientStart}
            onChange={(gradientStart) => setColor({ gradientStart })}
          />
          <ColorField
            text={t('viz.ui.gradientEnd')}
            value={config.colors.gradientEnd}
            onChange={(gradientEnd) => setColor({ gradientEnd })}
          />

          {data.series.length > 0 && (
            <div className="customize__series">
              <p className="customize__subtitle">{t('viz.ui.series')}</p>
              {data.series.map((series, index) => {
                const style = config.colors.series[String(index)];
                return (
                  <div key={series.id} className="customize__series-row">
                    <input
                      type="color"
                      aria-label={t('viz.ui.colourOf', { name: series.label })}
                      value={
                        style?.color ??
                        (index === 0 ? config.colors.primary : config.colors.secondary)
                      }
                      onChange={(event) =>
                        setColor({
                          series: {
                            ...config.colors.series,
                            [String(index)]: { ...style, color: event.target.value },
                          },
                        })
                      }
                    />
                    <input
                      type="text"
                      className="customize__text"
                      aria-label={t('viz.ui.labelOf', { name: series.label })}
                      placeholder={series.label}
                      maxLength={40}
                      value={style?.label ?? ''}
                      onChange={(event) =>
                        update({
                          colors: {
                            series: {
                              ...config.colors.series,
                              [String(index)]: { ...style, label: event.target.value || undefined },
                            },
                          },
                        })
                      }
                    />
                  </div>
                );
              })}
            </div>
          )}

          {config.colors.mode === 'threshold' && (
            <div className="customize__thresholds">
              <p className="customize__subtitle">{t('viz.ui.thresholdBands')}</p>
              <p className="customize__hint">{t('viz.ui.thresholdHint')}</p>
              {config.colors.thresholds.map((band, index) => (
                <div key={index} className="customize__series-row">
                  <input
                    type="color"
                    aria-label={t('viz.ui.bandColour', { index: index + 1 })}
                    value={band.color}
                    onChange={(event) =>
                      setThresholds(
                        config.colors.thresholds.map((entry, i) =>
                          i === index ? { ...entry, color: event.target.value } : entry,
                        ),
                      )
                    }
                  />
                  <input
                    type="number"
                    aria-label={t('viz.ui.bandUpper', { index: index + 1 })}
                    placeholder={t('viz.ui.above')}
                    value={band.upTo ?? ''}
                    onChange={(event) =>
                      setThresholds(
                        config.colors.thresholds.map((entry, i) =>
                          i === index
                            ? {
                                ...entry,
                                upTo: event.target.value === '' ? null : Number(event.target.value),
                              }
                            : entry,
                        ),
                      )
                    }
                  />
                  <button
                    type="button"
                    className="button button--quiet"
                    aria-label={t('viz.ui.removeBand', { index: index + 1 })}
                    onClick={() =>
                      setThresholds(config.colors.thresholds.filter((_, i) => i !== index))
                    }
                  >
                    {t('common.remove')}
                  </button>
                </div>
              ))}
              <button
                type="button"
                className="button button--quiet"
                onClick={() =>
                  setThresholds([
                    ...config.colors.thresholds,
                    { upTo: null, color: config.colors.secondary },
                  ])
                }
              >
                {t('viz.ui.addBand')}
              </button>
            </div>
          )}
        </Section>

        <Section title={t('viz.ui.backgroundFrame')}>
          <Field label={t('viz.ui.background')}>
            <Choice
              ariaLabel={t('viz.ui.background')}
              options={BACKGROUND_MODES}
              value={config.background.mode}
              onChange={(mode) => update({ background: { mode } })}
            />
          </Field>
          <Field label={t('viz.ui.backgroundOpacity')}>
            <Slider
              ariaLabel={t('viz.ui.backgroundOpacity')}
              value={config.background.opacity}
              min={0}
              max={1}
              step={0.05}
              format={pct}
              onChange={(opacity) => update({ background: { opacity } })}
            />
          </Field>
          <Field label={t('viz.ui.border')}>
            <Choice
              ariaLabel={t('viz.ui.border')}
              options={BORDER_STYLES}
              value={config.frame.border}
              onChange={(border) => update({ frame: { border } })}
            />
          </Field>
          <Field label={t('viz.ui.cornerRadius')}>
            <Slider
              ariaLabel={t('viz.ui.cornerRadius')}
              value={config.frame.radius}
              min={LIMITS.radius.min}
              max={LIMITS.radius.max}
              step={1}
              format={(value) => `${value} px`}
              onChange={(radius) => update({ frame: { radius } })}
            />
          </Field>
          <Field label={t('viz.ui.shadow')}>
            <Choice
              ariaLabel={t('viz.ui.shadow')}
              options={SHADOW_STYLES}
              value={config.frame.shadow}
              onChange={(shadow) => update({ frame: { shadow } })}
            />
          </Field>
        </Section>

        <Section title={t('viz.ui.text')}>
          <Field label={t('viz.ui.textSize')}>
            <Slider
              ariaLabel={t('viz.ui.textSize')}
              value={config.text.scale}
              min={LIMITS.textScale.min}
              max={LIMITS.textScale.max}
              step={0.05}
              format={pct}
              onChange={(scale) => update({ text: { scale } })}
            />
          </Field>
          <Field label={t('viz.ui.weight')}>
            <Choice
              ariaLabel={t('viz.ui.fontWeight')}
              options={FONT_WEIGHTS}
              value={config.text.weight}
              labelFor={(value) => label(value)}
              onChange={(weight) => update({ text: { weight } })}
            />
          </Field>
          <Field label={t('viz.ui.decimals')}>
            <Choice
              ariaLabel={t('viz.ui.decimalPrecision')}
              options={['auto', '0', '1', '2', '3'] as const}
              value={config.text.decimals === null ? 'auto' : (String(config.text.decimals) as '0')}
              labelFor={(value) => (value === 'auto' ? t('viz.options.auto') : value)}
              onChange={(value) =>
                update({ text: { decimals: value === 'auto' ? null : Number(value) } })
              }
            />
          </Field>
          <div className="customize__toggles">
            <Toggle
              text={t('viz.ui.label')}
              checked={config.text.showLabel}
              onChange={(showLabel) => update({ text: { showLabel } })}
            />
            <Toggle
              text={t('viz.ui.unit')}
              checked={config.text.showUnit}
              onChange={(showUnit) => update({ text: { showUnit } })}
            />
          </div>
        </Section>

        <Section title={t('viz.ui.axesGridScale')}>
          <div className="customize__toggles">
            <Toggle
              text={t('viz.ui.xAxis')}
              checked={config.axes.x}
              disabled={compactLocks}
              onChange={(value) => update({ axes: { x: value } })}
            />
            <Toggle
              text={t('viz.ui.yAxis')}
              checked={config.axes.y}
              disabled={compactLocks}
              onChange={(value) => update({ axes: { y: value } })}
            />
            <Toggle
              text={t('viz.ui.grid')}
              checked={config.axes.grid}
              disabled={compactLocks}
              onChange={(grid) => update({ axes: { grid } })}
            />
          </div>
          {compactLocks && <p className="customize__hint">{t('viz.ui.compactHides')}</p>}
          <Field label={t('viz.ui.yScale')}>
            <Choice
              ariaLabel={t('viz.ui.yScale')}
              options={['auto', 'fixed'] as const}
              value={config.scale.mode}
              labelFor={(value) => t(`viz.options.${value}`)}
              onChange={(mode) => {
                if (mode === 'auto') update({ scale: { mode } });
                else if (!draftError) update({ scale: { mode, min: parsedMin, max: parsedMax } });
                else if (meta.bounds) {
                  setScaleDraft({ min: String(meta.bounds.min), max: String(meta.bounds.max) });
                  update({ scale: { mode, min: meta.bounds.min, max: meta.bounds.max } });
                }
              }}
            />
          </Field>
          <div className="customize__row">
            <label className="customize__inline">
              {t('viz.ui.min')}
              <input
                type="number"
                value={scaleDraft.min}
                onChange={(event) => setScaleDraft({ ...scaleDraft, min: event.target.value })}
              />
            </label>
            <label className="customize__inline">
              {t('viz.ui.max')}
              <input
                type="number"
                value={scaleDraft.max}
                onChange={(event) => setScaleDraft({ ...scaleDraft, max: event.target.value })}
              />
            </label>
            <button
              type="button"
              className="button"
              disabled={draftError !== null}
              onClick={() => update({ scale: { mode: 'fixed', min: parsedMin, max: parsedMax } })}
            >
              {t('common.apply')}
            </button>
          </div>
          {draftError && (scaleDraft.min !== '' || scaleDraft.max !== '') && (
            <p className="customize__error" role="alert">
              {draftError}
            </p>
          )}
        </Section>

        <Section title={t('viz.ui.legendTooltipStatistics')}>
          <div className="customize__toggles">
            <Toggle
              text={t('viz.ui.legend')}
              checked={config.display.legend}
              disabled={compactLocks}
              onChange={(legend) => update({ display: { legend } })}
            />
            <Toggle
              text={t('viz.ui.tooltip')}
              checked={config.display.tooltip}
              onChange={(tooltip) => update({ display: { tooltip } })}
            />
            <Toggle
              text={t('viz.ui.current')}
              checked={config.display.current}
              onChange={(value) => update({ display: { current: value } })}
            />
            <Toggle
              text={t('viz.ui.minimum')}
              checked={config.display.min}
              onChange={(value) => update({ display: { min: value } })}
            />
            <Toggle
              text={t('viz.ui.maximum')}
              checked={config.display.max}
              onChange={(value) => update({ display: { max: value } })}
            />
            <Toggle
              text={t('viz.ui.average')}
              checked={config.display.average}
              onChange={(average) => update({ display: { average } })}
            />
          </div>
        </Section>

        <Section title={t('viz.ui.gauge')}>
          <Field label={t('viz.ui.ringThickness')}>
            <Slider
              ariaLabel={t('viz.ui.ringThickness')}
              value={config.gauge.thickness}
              min={LIMITS.gaugeThickness.min}
              max={LIMITS.gaugeThickness.max}
              step={0.01}
              format={pct}
              onChange={(thickness) => update({ gauge: { thickness } })}
            />
          </Field>
          <Field label={t('viz.ui.arc')}>
            <Choice
              ariaLabel={t('viz.ui.gaugeArc')}
              options={GAUGE_ARCS}
              value={config.gauge.arc}
              onChange={(arc) => update({ gauge: { arc } })}
            />
          </Field>
        </Section>
      </div>
    </aside>
  );
}
