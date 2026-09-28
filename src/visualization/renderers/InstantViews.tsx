import type { VisualizationConfig } from '@/visualization/config';
import type { ResolvedColors } from '@/visualization/color';
import { thresholdColor } from '@/visualization/color';
import { formatParts, formatValue } from '@/visualization/format';
import { fitLine } from '@/visualization/presentation';
import { barBounds, gaugeBounds } from '@/visualization/registry';
import { arcPath } from '@/visualization/geometry';
import type { SeriesSummary } from '@/visualization/series';
import type { VisualizationMeta, VisualizationSeries } from '@/visualization/types';

/**
 * The renderers that show the **current** value: a number, a bar, a ring.
 *
 * All three read the same summary as the time-series header — the latest
 * recorded sample — so switching a chart from Area to Gauge never changes the
 * number shown. Several series (Download and Upload, say) become several
 * rows or rings, never a sum.
 */
export interface InstantViewProps {
  readonly series: readonly VisualizationSeries[];
  readonly summaries: readonly SeriesSummary[];
  readonly meta: VisualizationMeta;
  readonly config: VisualizationConfig;
  readonly colors: ResolvedColors;
  readonly width: number;
  readonly height: number;
  readonly labels: readonly string[];
}

const WEIGHTS = { regular: 400, medium: 500, bold: 650 } as const;

function decimalsOf(config: VisualizationConfig, meta: VisualizationMeta) {
  return config.text.decimals ?? meta.decimals;
}

/**
 * Just the number: `27 %`, or `CPU 27 %`, as large as the box allows.
 *
 * Tall rows stack the label above the number; short ones put everything on
 * one line fitted by `fitLine` — the label goes first, then the unit, before
 * the text is allowed to become unreadable. Nothing wraps or overlaps.
 */
export function ValueView({
  summaries,
  meta,
  config,
  colors,
  width,
  height,
  labels,
}: InstantViewProps) {
  const rows = Math.max(1, summaries.length);
  const rowHeight = height / rows;
  const stacked = rowHeight >= 56 && width >= 110;

  return (
    <div className="viz-value" style={{ width, height }}>
      {summaries.map((summary, index) => {
        const parts =
          summary.current === null
            ? { value: '—', unit: '' }
            : formatParts(summary.current, meta.unit, decimalsOf(config, meta));
        const label = summaries.length > 1 ? labels[index] : meta.label;
        const wantLabel = config.text.showLabel ? label : null;
        const wantUnit = config.text.showUnit && parts.unit ? parts.unit : null;
        const color = thresholdColor(config, summary.current, colors.text);

        if (stacked) {
          const numberFit = fitLine(
            width - 16,
            rowHeight * 0.62,
            { value: parts.value, unit: wantUnit },
            config.text.scale,
            64,
          );
          const labelPx = Math.max(9, Math.min(14, rowHeight * 0.2)) * config.text.scale;
          return (
            <div key={index} className="viz-value__row" style={{ height: rowHeight }}>
              {wantLabel && (
                <span
                  className="viz-value__label"
                  style={{ fontSize: labelPx, color: colors.muted }}
                >
                  {wantLabel}
                </span>
              )}
              <span
                className="viz-value__number"
                style={{
                  fontSize: numberFit.fontPx,
                  fontWeight: WEIGHTS[config.text.weight],
                  color,
                }}
              >
                {parts.value}
                {numberFit.unit && wantUnit && (
                  <span className="viz-value__unit" style={{ fontSize: numberFit.fontPx * 0.5 }}>
                    {` ${wantUnit}`}
                  </span>
                )}
              </span>
              {index === 0 && meta.secondary && rowHeight >= 80 && (
                <span
                  className="viz-value__secondary"
                  style={{ fontSize: labelPx, color: colors.muted }}
                >
                  {meta.secondary}
                </span>
              )}
            </div>
          );
        }

        const fit = fitLine(
          width - 8,
          rowHeight,
          { label: wantLabel, value: parts.value, unit: wantUnit },
          config.text.scale,
          32,
        );
        return (
          <div
            key={index}
            className="viz-value__row viz-value__row--inline"
            style={{ height: rowHeight, fontSize: fit.fontPx }}
          >
            {fit.label && wantLabel && (
              <span className="viz-value__label" style={{ color: colors.muted }}>
                {wantLabel}
              </span>
            )}
            <span
              className="viz-value__number"
              style={{ fontWeight: WEIGHTS[config.text.weight], color }}
            >
              {parts.value}
              {fit.unit && wantUnit ? ` ${wantUnit}` : ''}
            </span>
          </div>
        );
      })}
    </div>
  );
}

/** `CPU ███████░░░ 72 %` — one row per series. */
export function BarView({
  series,
  summaries,
  meta,
  config,
  colors,
  width,
  height,
  labels,
}: InstantViewProps) {
  const bounds = barBounds(meta, config, series);
  const rows = Math.max(1, summaries.length);
  const rowHeight = Math.min(height / rows, 48);
  const trackHeight = Math.max(4, Math.min(18, rowHeight * 0.36));
  const fontPx = Math.max(8, Math.min(14, rowHeight * 0.5)) * config.text.scale;
  const span = bounds.max - bounds.min || 1;
  // The label is the first thing to go: the bar and its value keep the room.
  const showLabel = config.text.showLabel && width >= 160 && rowHeight >= 16;
  const showNote = height - rows * rowHeight >= 14;

  return (
    <div className="viz-bar" style={{ width, height }}>
      {summaries.map((summary, index) => {
        const value = summary.current;
        const ratio = value === null ? 0 : Math.max(0, Math.min(1, (value - bounds.min) / span));
        const color = thresholdColor(config, value, colors.series(index));
        const label = summaries.length > 1 ? labels[index] : meta.label;
        return (
          <div key={index} className="viz-bar__row" style={{ height: rowHeight, fontSize: fontPx }}>
            {showLabel && (
              <span className="viz-bar__label" style={{ color: colors.muted }}>
                {label}
              </span>
            )}
            <span
              className="viz-bar__track"
              style={{ height: trackHeight, background: colors.grid }}
              role="meter"
              aria-label={`${label}`}
              aria-valuemin={bounds.min}
              aria-valuemax={bounds.max}
              aria-valuenow={value ?? undefined}
            >
              {value !== null && (
                <span
                  className="viz-bar__fill"
                  style={{
                    width: `${(ratio * 100).toFixed(2)}%`,
                    background:
                      config.fill.mode === 'gradient'
                        ? `linear-gradient(90deg, transparent -40%, ${color})`
                        : color,
                  }}
                />
              )}
            </span>
            <span
              className="viz-bar__value"
              style={{ color: colors.text, fontWeight: WEIGHTS[config.text.weight] }}
            >
              {value === null
                ? '—'
                : formatValue(value, meta.unit, decimalsOf(config, meta), config.text.showUnit)}
            </span>
          </div>
        );
      })}
      {bounds.auto &&
        showNote &&
        summaries.some((summary) => summary.current !== null) &&
        !config.display.compact && (
          <span className="viz-bar__note" style={{ color: colors.muted }}>
            {`Scaled to the window's peak, ${formatValue(bounds.max, meta.unit, decimalsOf(config, meta))}`}
          </span>
        )}
    </div>
  );
}

const ARC_SWEEP = { full: 359.99, 'three-quarter': 270, half: 180 } as const;

/**
 * A ring per series. Only for metrics with bounds — see
 * `rendererSupport`; `MetricVisualization` never reaches here without them.
 */
export function GaugeView({
  summaries,
  meta,
  config,
  colors,
  width,
  height,
  labels,
}: InstantViewProps) {
  const bounds = gaugeBounds(meta, config) ?? { min: 0, max: 100 };
  const count = Math.max(1, summaries.length);
  const cell = Math.max(16, Math.min(width / count, height));
  const sweep = ARC_SWEEP[config.gauge.arc];
  const start = -sweep / 2;
  const span = bounds.max - bounds.min || 1;

  return (
    <div className="viz-gauge" style={{ width, height }}>
      {summaries.map((summary, index) => {
        const size = cell;
        const thickness = Math.max(2, size * config.gauge.thickness * 0.5);
        const radius = size / 2 - thickness / 2 - 1;
        const cx = size / 2;
        const cy = config.gauge.arc === 'half' ? size * 0.62 : size / 2;
        const value = summary.current;
        const ratio = value === null ? 0 : Math.max(0, Math.min(1, (value - bounds.min) / span));
        const color = thresholdColor(config, value, colors.series(index));
        const parts =
          value === null
            ? { value: '—', unit: '' }
            : formatParts(value, meta.unit, decimalsOf(config, meta));
        const label = summaries.length > 1 ? labels[index] : meta.label;
        const valuePx = Math.max(9, size * 0.2) * config.text.scale;

        return (
          <svg
            key={index}
            className="viz-gauge__ring"
            width={size}
            height={size}
            viewBox={`0 0 ${size} ${size}`}
            role="meter"
            aria-label={label}
            aria-valuemin={bounds.min}
            aria-valuemax={bounds.max}
            aria-valuenow={value ?? undefined}
          >
            <path
              className="viz-gauge__track"
              d={arcPath(cx, cy, radius, start, start + sweep)}
              style={{
                stroke: colors.grid,
                strokeWidth: thickness,
                fill: 'none',
                strokeLinecap: 'round',
              }}
            />
            {value !== null && ratio > 0 && (
              <path
                className="viz-gauge__value"
                d={arcPath(cx, cy, radius, start, start + sweep * ratio)}
                style={{
                  stroke: color,
                  strokeWidth: thickness,
                  fill: 'none',
                  strokeLinecap: 'round',
                }}
              />
            )}
            <text
              className="viz-gauge__number"
              x={cx}
              y={cy}
              textAnchor="middle"
              dominantBaseline="central"
              style={{
                fill: colors.text,
                fontSize: valuePx,
                fontWeight: WEIGHTS[config.text.weight],
              }}
            >
              {parts.value}
              {config.text.showUnit && parts.unit ? (
                <tspan style={{ fontSize: valuePx * 0.5 }}>{` ${parts.unit}`}</tspan>
              ) : null}
            </text>
            {config.text.showLabel && size >= 56 && (
              <text
                className="viz-gauge__label"
                x={cx}
                y={cy + valuePx * 0.95}
                textAnchor="middle"
                dominantBaseline="central"
                style={{ fill: colors.muted, fontSize: Math.max(8, valuePx * 0.42) }}
              >
                {label}
              </text>
            )}
          </svg>
        );
      })}
    </div>
  );
}
