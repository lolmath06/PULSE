import { useEffect, useState } from 'react';
import type { ChangeEvent, ReactNode } from 'react';
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
}

const LABELS: Record<string, string> = {
  small: 'Small',
  medium: 'Medium',
  large: 'Large',
  custom: 'Custom',
  straight: 'Straight',
  smooth: 'Smooth',
  stepped: 'Stepped',
  none: 'None',
  visible: 'Visible',
  solid: 'Solid',
  gradient: 'Gradient',
  theme: 'Theme',
  manual: 'Manual',
  threshold: 'Threshold',
  light: 'Light',
  thin: 'Thin',
  subtle: 'Subtle',
  glow: 'Glow',
  regular: 'Regular',
  bold: 'Bold',
  full: 'Full circle',
  'three-quarter': 'Three quarters',
  half: 'Half',
};

const label = (value: string) => LABELS[value] ?? value;

function Section({
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

function Field({
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

function Choice<T extends string>({
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

function Toggle({
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

function Slider({
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
function ColorField({
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
        aria-label={`${text} colour`}
        onChange={(event: ChangeEvent<HTMLInputElement>) => onChange(event.target.value)}
      />
      <input
        type="text"
        className="customize__color-hex mono"
        value={draft}
        aria-label={`${text} hex value`}
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

const pct = (value: number) => `${Math.round(value * 100)} %`;

export function CustomizePanel({ title, chart, data, meta, onClose }: CustomizePanelProps) {
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
    ? `Custom (from ${preset?.name ?? 'preset'})`
    : (preset?.name ?? '');

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
    <aside className="customize" role="dialog" aria-label={`Customize ${title}`}>
      <header className="customize__header">
        <div>
          <p className="customize__eyebrow">Customize</p>
          <h3 className="customize__title">{title}</h3>
        </div>
        <button type="button" className="button button--quiet" onClick={onClose}>
          Close
        </button>
      </header>

      <div className="customize__preview" aria-label="Preview">
        <MetricVisualization data={data} meta={meta} config={config} width={332} height={150} />
      </div>

      <div className="customize__scroll">
        <Section title="Preset" open>
          <p className="customize__preset-status" aria-live="polite">
            {presetStatus}
          </p>
          <div className="customize__presets">
            {chart.presets.map((entry) => (
              <button
                key={entry.id}
                type="button"
                title={entry.description}
                className={`customize__preset${entry.id === chart.presetId ? ' customize__preset--active' : ''}`}
                onClick={() => chart.applyPreset(entry.id)}
              >
                {entry.name}
              </button>
            ))}
          </div>
          <div className="customize__row">
            <button type="button" className="button" onClick={chart.reset}>
              Reset visualization
            </button>
            {preset?.custom && (
              <button
                type="button"
                className="button button--quiet"
                onClick={() => chart.deletePreset(preset.id)}
              >
                Delete preset
              </button>
            )}
          </div>
          <div className="customize__row">
            <input
              type="text"
              className="customize__text"
              placeholder="Preset name"
              aria-label="New preset name"
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
              Save as preset
            </button>
          </div>
        </Section>

        <Section title="Visualization" open>
          <div className="customize__renderers" role="group" aria-label="Visualization type">
            {RENDERERS.map((renderer) => {
              const support = rendererSupport(renderer.kind, meta, config);
              return (
                <button
                  key={renderer.kind}
                  type="button"
                  className={`customize__renderer${renderer.kind === config.renderer ? ' customize__renderer--active' : ''}`}
                  aria-pressed={renderer.kind === config.renderer}
                  disabled={!support.ok}
                  title={support.ok ? renderer.description : support.reason}
                  onClick={() => update({ renderer: renderer.kind as RendererKind })}
                >
                  {renderer.label}
                </button>
              );
            })}
          </div>
          {!rendererSupport('gauge', meta, config).ok && (
            <p className="customize__hint">
              Gauge is unavailable: this metric has no natural bounds. Set a fixed scale to use it.
            </p>
          )}
          <Field label="Time range">
            <Choice
              ariaLabel="Time range"
              options={HISTORY_RANGES}
              value={chart.range}
              labelFor={(value) => value}
              onChange={(value: HistoryRange) => chart.setRange(value)}
            />
          </Field>
          <Field label="Size">
            <Choice
              ariaLabel="Size"
              options={SIZE_PRESETS}
              value={config.size.preset}
              onChange={(preset) => update({ size: { preset } })}
            />
          </Field>
          {config.size.preset === 'custom' && (
            <div className="customize__row">
              <label className="customize__inline">
                Height
                <input
                  type="number"
                  min={LIMITS.height.min}
                  max={LIMITS.height.max}
                  value={config.size.height ?? 180}
                  onChange={(event) => update({ size: { height: Number(event.target.value) } })}
                />
              </label>
              <label className="customize__inline">
                Width
                <input
                  type="number"
                  min={LIMITS.width.min}
                  max={LIMITS.width.max}
                  placeholder="fill"
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
              text="Compact mode"
              checked={config.display.compact}
              onChange={(compact) => update({ display: { compact } })}
            />
          </div>
        </Section>

        <Section title="Line & fill">
          <Field label="Line width">
            <Slider
              ariaLabel="Line width"
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
                ['Thin', 1],
                ['Medium', 2],
                ['Thick', 3.5],
              ] as const
            ).map(([text, width]) => (
              <button
                key={text}
                type="button"
                className="button button--quiet"
                onClick={() => update({ line: { width } })}
              >
                {text}
              </button>
            ))}
          </div>
          <Field label="Curve">
            <Choice
              ariaLabel="Curve"
              options={CURVE_STYLES}
              value={config.line.curve}
              onChange={(curve) => update({ line: { curve } })}
            />
          </Field>
          <Field label="Points">
            <Choice
              ariaLabel="Point markers"
              options={POINT_STYLES}
              value={config.line.points}
              labelFor={(value) => (value === 'small' ? 'Small' : label(value))}
              onChange={(points) => update({ line: { points } })}
            />
          </Field>
          <Field label="Fill">
            <Choice
              ariaLabel="Fill"
              options={FILL_MODES}
              value={config.fill.mode}
              onChange={(mode) => update({ fill: { mode } })}
            />
          </Field>
          <Field label="Fill opacity">
            <Slider
              ariaLabel="Fill opacity"
              value={config.fill.opacity}
              min={0}
              max={1}
              step={0.05}
              format={pct}
              onChange={(opacity) => update({ fill: { opacity } })}
            />
          </Field>
          <Field
            label="Smoothing"
            hint="Visual only: stored samples and the tooltip keep the real values."
          >
            <Choice
              ariaLabel="Smoothing"
              options={SMOOTHING_LEVELS}
              value={config.smoothing}
              onChange={(smoothing) => update({ smoothing })}
            />
          </Field>
        </Section>

        <Section title="Colors">
          <Field label="Color mode">
            <Choice
              ariaLabel="Color mode"
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
            <p className="customize__hint">
              Following PULSE&apos;s theme. Picking any colour switches to Manual.
            </p>
          )}
          <ColorField
            text="Primary"
            value={config.colors.primary}
            onChange={(primary) => setColor({ primary })}
          />
          <ColorField
            text="Secondary"
            value={config.colors.secondary}
            onChange={(secondary) => setColor({ secondary })}
          />
          <ColorField
            text="Text"
            value={config.colors.text}
            onChange={(text) => setColor({ text })}
          />
          <ColorField
            text="Grid"
            value={config.colors.grid}
            onChange={(grid) => setColor({ grid })}
          />
          <ColorField
            text="Background"
            value={config.colors.background}
            onChange={(background) => setColor({ background })}
          />
          <ColorField
            text="Border"
            value={config.colors.border}
            onChange={(border) => setColor({ border })}
          />
          <ColorField
            text="Fill"
            value={config.colors.fill ?? config.colors.primary}
            onChange={(fill) => setColor({ fill })}
          />
          <Toggle
            text="Fill follows the line colour"
            checked={config.colors.fill === null}
            onChange={(follow) => setColor({ fill: follow ? null : config.colors.primary })}
          />
          <ColorField
            text="Gradient start"
            value={config.colors.gradientStart}
            onChange={(gradientStart) => setColor({ gradientStart })}
          />
          <ColorField
            text="Gradient end"
            value={config.colors.gradientEnd}
            onChange={(gradientEnd) => setColor({ gradientEnd })}
          />

          {data.series.length > 0 && (
            <div className="customize__series">
              <p className="customize__subtitle">Series</p>
              {data.series.map((series, index) => {
                const style = config.colors.series[String(index)];
                return (
                  <div key={series.id} className="customize__series-row">
                    <input
                      type="color"
                      aria-label={`${series.label} colour`}
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
                      aria-label={`${series.label} label`}
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
              <p className="customize__subtitle">Threshold bands</p>
              <p className="customize__hint">
                Visual bands you choose — not a judgement of what is too high on your machine.
              </p>
              {config.colors.thresholds.map((band, index) => (
                <div key={index} className="customize__series-row">
                  <input
                    type="color"
                    aria-label={`Band ${index + 1} colour`}
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
                    aria-label={`Band ${index + 1} upper bound`}
                    placeholder="above"
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
                    aria-label={`Remove band ${index + 1}`}
                    onClick={() =>
                      setThresholds(config.colors.thresholds.filter((_, i) => i !== index))
                    }
                  >
                    Remove
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
                Add band
              </button>
            </div>
          )}
        </Section>

        <Section title="Background & frame">
          <Field label="Background">
            <Choice
              ariaLabel="Background"
              options={BACKGROUND_MODES}
              value={config.background.mode}
              onChange={(mode) => update({ background: { mode } })}
            />
          </Field>
          <Field label="Background opacity">
            <Slider
              ariaLabel="Background opacity"
              value={config.background.opacity}
              min={0}
              max={1}
              step={0.05}
              format={pct}
              onChange={(opacity) => update({ background: { opacity } })}
            />
          </Field>
          <Field label="Border">
            <Choice
              ariaLabel="Border"
              options={BORDER_STYLES}
              value={config.frame.border}
              onChange={(border) => update({ frame: { border } })}
            />
          </Field>
          <Field label="Corner radius">
            <Slider
              ariaLabel="Corner radius"
              value={config.frame.radius}
              min={LIMITS.radius.min}
              max={LIMITS.radius.max}
              step={1}
              format={(value) => `${value} px`}
              onChange={(radius) => update({ frame: { radius } })}
            />
          </Field>
          <Field label="Shadow">
            <Choice
              ariaLabel="Shadow"
              options={SHADOW_STYLES}
              value={config.frame.shadow}
              onChange={(shadow) => update({ frame: { shadow } })}
            />
          </Field>
        </Section>

        <Section title="Text">
          <Field label="Text size">
            <Slider
              ariaLabel="Text size"
              value={config.text.scale}
              min={LIMITS.textScale.min}
              max={LIMITS.textScale.max}
              step={0.05}
              format={pct}
              onChange={(scale) => update({ text: { scale } })}
            />
          </Field>
          <Field label="Weight">
            <Choice
              ariaLabel="Font weight"
              options={FONT_WEIGHTS}
              value={config.text.weight}
              labelFor={(value) => (value === 'medium' ? 'Medium' : label(value))}
              onChange={(weight) => update({ text: { weight } })}
            />
          </Field>
          <Field label="Decimals">
            <Choice
              ariaLabel="Decimal precision"
              options={['auto', '0', '1', '2', '3'] as const}
              value={config.text.decimals === null ? 'auto' : (String(config.text.decimals) as '0')}
              labelFor={(value) => (value === 'auto' ? 'Auto' : value)}
              onChange={(value) =>
                update({ text: { decimals: value === 'auto' ? null : Number(value) } })
              }
            />
          </Field>
          <div className="customize__toggles">
            <Toggle
              text="Label"
              checked={config.text.showLabel}
              onChange={(showLabel) => update({ text: { showLabel } })}
            />
            <Toggle
              text="Unit"
              checked={config.text.showUnit}
              onChange={(showUnit) => update({ text: { showUnit } })}
            />
          </div>
        </Section>

        <Section title="Axes, grid & scale">
          <div className="customize__toggles">
            <Toggle
              text="X axis"
              checked={config.axes.x}
              disabled={compactLocks}
              onChange={(value) => update({ axes: { x: value } })}
            />
            <Toggle
              text="Y axis"
              checked={config.axes.y}
              disabled={compactLocks}
              onChange={(value) => update({ axes: { y: value } })}
            />
            <Toggle
              text="Grid"
              checked={config.axes.grid}
              disabled={compactLocks}
              onChange={(grid) => update({ axes: { grid } })}
            />
          </div>
          {compactLocks && (
            <p className="customize__hint">
              Compact mode and sparklines hide axes, grid and legend.
            </p>
          )}
          <Field label="Y scale">
            <Choice
              ariaLabel="Y scale"
              options={['auto', 'fixed'] as const}
              value={config.scale.mode}
              labelFor={(value) => (value === 'auto' ? 'Auto' : 'Fixed')}
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
              Min
              <input
                type="number"
                value={scaleDraft.min}
                onChange={(event) => setScaleDraft({ ...scaleDraft, min: event.target.value })}
              />
            </label>
            <label className="customize__inline">
              Max
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
              Apply
            </button>
          </div>
          {draftError && (scaleDraft.min !== '' || scaleDraft.max !== '') && (
            <p className="customize__error" role="alert">
              {draftError}
            </p>
          )}
        </Section>

        <Section title="Legend, tooltip & statistics">
          <div className="customize__toggles">
            <Toggle
              text="Legend"
              checked={config.display.legend}
              disabled={compactLocks}
              onChange={(legend) => update({ display: { legend } })}
            />
            <Toggle
              text="Tooltip"
              checked={config.display.tooltip}
              onChange={(tooltip) => update({ display: { tooltip } })}
            />
            <Toggle
              text="Current"
              checked={config.display.current}
              onChange={(value) => update({ display: { current: value } })}
            />
            <Toggle
              text="Minimum"
              checked={config.display.min}
              onChange={(value) => update({ display: { min: value } })}
            />
            <Toggle
              text="Maximum"
              checked={config.display.max}
              onChange={(value) => update({ display: { max: value } })}
            />
            <Toggle
              text="Average"
              checked={config.display.average}
              onChange={(average) => update({ display: { average } })}
            />
          </div>
        </Section>

        <Section title="Gauge">
          <Field label="Ring thickness">
            <Slider
              ariaLabel="Ring thickness"
              value={config.gauge.thickness}
              min={LIMITS.gaugeThickness.min}
              max={LIMITS.gaugeThickness.max}
              step={0.01}
              format={pct}
              onChange={(thickness) => update({ gauge: { thickness } })}
            />
          </Field>
          <Field label="Arc">
            <Choice
              ariaLabel="Gauge arc"
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
