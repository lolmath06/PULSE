import { t } from '@/i18n/i18n';
import { useId, useMemo, useState } from 'react';
import type { PointerEvent } from 'react';
import {
  area as d3Area,
  curveLinear,
  curveMonotoneX,
  curveStepAfter,
  line as d3Line,
} from 'd3-shape';
import type { CurveFactory } from 'd3-shape';
import type { CurveStyle, VisualizationConfig } from '@/visualization/config';
import type { ResolvedColors } from '@/visualization/color';
import { thresholdColor, thresholdStops } from '@/visualization/color';
import {
  formatAxisValue,
  formatTime,
  formatTooltipTime,
  formatValue,
} from '@/visualization/format';
import type { Domain } from '@/visualization/scale';
import { niceTicks, project, timeTicks, yDomain } from '@/visualization/scale';
import { nearestPoint, smoothValues, splitSegments, timeExtent } from '@/visualization/series';
import type {
  VisualizationData,
  VisualizationMeta,
  VisualizationPoint,
} from '@/visualization/types';

/**
 * Line, area and sparkline: one renderer, three variants.
 *
 * They share everything that matters — the data path, gap handling, scales,
 * smoothing, colours and tooltip — and differ only in their defaults for
 * chrome and fill. That is the "no duplicated data engine" rule: an area chart
 * is a line chart that fills, and a sparkline is a line chart without chrome.
 *
 * Plain SVG sized to the exact pixel box it is given, so it is crisp from an
 * 80 × 24 micro-graph to a 1200 × 450 panel. `d3-shape` only builds the path
 * strings; scales, ticks and layout are PULSE's own.
 */
export type TimeSeriesVariant = 'line' | 'area' | 'sparkline';

export interface TimeSeriesChartProps {
  readonly variant: TimeSeriesVariant;
  readonly data: VisualizationData;
  readonly meta: VisualizationMeta;
  readonly config: VisualizationConfig;
  readonly colors: ResolvedColors;
  readonly width: number;
  readonly height: number;
  /** Series labels after configuration overrides. */
  readonly labels: readonly string[];
}

const CURVES: Readonly<Record<CurveStyle, CurveFactory>> = {
  straight: curveLinear,
  smooth: curveMonotoneX,
  stepped: curveStepAfter,
};

/** Point markers are skipped beyond this many points per series. */
const MAX_MARKERS = 300;

const FONT_PX = 11;

interface Layout {
  readonly left: number;
  readonly right: number;
  readonly top: number;
  readonly bottom: number;
}

export function TimeSeriesChart({
  variant,
  data,
  meta,
  config,
  colors,
  width,
  height,
  labels,
}: TimeSeriesChartProps) {
  const gradientBase = useId().replace(/:/g, '');
  const [hover, setHover] = useState<{ x: number; t: number } | null>(null);

  const chromeless = variant === 'sparkline' || config.display.compact;
  const showX = !chromeless && config.axes.x;
  const showY = !chromeless && config.axes.y;
  const showGrid = !chromeless && config.axes.grid;
  const fontPx = Math.round(FONT_PX * config.text.scale * 10) / 10;
  const fillMode = variant === 'line' ? 'none' : config.fill.mode;

  const domain: Domain = useMemo(
    () => yDomain(data.series, config, meta),
    [data.series, config, meta],
  );

  const xDomain = useMemo(() => {
    if (variant !== 'sparkline' && data.window) {
      return { min: data.window.fromMs, max: data.window.toMs };
    }
    const extent = timeExtent(data.series);
    if (!extent) return { min: 0, max: 1 };
    return extent.max > extent.min
      ? extent
      : { min: extent.min - data.gapThresholdMs, max: extent.max + data.gapThresholdMs };
  }, [variant, data.window, data.series, data.gapThresholdMs]);

  const yTicks = useMemo(
    () => niceTicks(domain.min, domain.max, Math.max(2, Math.min(6, Math.floor(height / 40)))),
    [domain, height],
  );

  const layout: Layout = useMemo(() => {
    const pad = Math.max(1, config.line.width);
    if (chromeless) return { left: pad, right: pad, top: pad + 1, bottom: pad + 1 };
    const span = domain.max - domain.min;
    const longest = yTicks.reduce(
      (most, tick) => Math.max(most, formatAxisValue(tick, meta.unit, span).length),
      0,
    );
    return {
      left: showY ? Math.ceil(longest * fontPx * 0.62) + 10 : pad + 2,
      right: 10,
      top: 8,
      bottom: showX ? Math.ceil(fontPx * 1.5) + 8 : pad + 2,
    };
  }, [chromeless, config.line.width, domain, yTicks, meta.unit, fontPx, showX, showY]);

  const plotLeft = layout.left;
  const plotRight = Math.max(layout.left + 1, width - layout.right);
  const plotTop = layout.top;
  const plotBottom = Math.max(layout.top + 1, height - layout.bottom);
  const plot = { left: plotLeft, right: plotRight, top: plotTop, bottom: plotBottom };

  const x = (t: number) => project(t, xDomain, plot.left, plot.right);
  const y = (v: number) => project(v, domain, plot.bottom, plot.top);

  const drawn = useMemo(() => {
    const curve = CURVES[config.line.curve];
    const px = (t: number) => project(t, xDomain, plotLeft, plotRight);
    const py = (v: number) => project(v, domain, plotBottom, plotTop);
    const baseline = py(Math.max(domain.min, Math.min(domain.max, 0)));

    return data.series.map((series) =>
      splitSegments(series.points, data.gapThresholdMs).map((segment) => {
        const smoothed = smoothValues(segment, config.smoothing);
        const indexed = segment.map((point, index) => ({ point, v: smoothed[index]! }));
        const lineBuilder = d3Line<{ point: VisualizationPoint; v: number }>()
          .x((entry) => px(entry.point.t))
          .y((entry) => py(entry.v))
          .curve(curve);
        const areaBuilder = d3Area<{ point: VisualizationPoint; v: number }>()
          .x((entry) => px(entry.point.t))
          .y0(baseline)
          .y1((entry) => py(entry.v))
          .curve(curve);
        const bandBuilder = d3Area<VisualizationPoint>()
          .x((point) => px(point.t))
          .y0((point) => py(point.min ?? point.v))
          .y1((point) => py(point.max ?? point.v))
          .curve(curve);
        return {
          segment,
          line: lineBuilder(indexed) ?? '',
          area: areaBuilder(indexed) ?? '',
          band: data.aggregated ? (bandBuilder(segment) ?? '') : '',
        };
      }),
    );
  }, [
    data.series,
    data.gapThresholdMs,
    data.aggregated,
    config.line.curve,
    config.smoothing,
    domain,
    xDomain,
    plotLeft,
    plotRight,
    plotTop,
    plotBottom,
  ]);

  const thresholdGradient =
    config.colors.mode === 'threshold' && config.colors.thresholds.length > 0;
  const strokeFor = (index: number) =>
    thresholdGradient && index === 0 ? `url(#${gradientBase}-threshold)` : colors.series(index);

  const spanMs = xDomain.max - xDomain.min;
  const xTicks = showX
    ? timeTicks(xDomain.min, xDomain.max, Math.max(2, Math.floor((plot.right - plot.left) / 90)))
    : [];

  const onPointerMove = (event: PointerEvent<SVGRectElement>) => {
    if (!config.display.tooltip) return;
    const box = event.currentTarget.ownerSVGElement?.getBoundingClientRect();
    const px = box ? event.clientX - box.left : 0;
    const clamped = Math.max(plot.left, Math.min(plot.right, px));
    const t = xDomain.min + ((clamped - plot.left) / (plot.right - plot.left)) * spanMs;
    setHover({ x: clamped, t });
  };

  const hovered = hover
    ? data.series.map((series) => nearestPoint(series.points, hover.t, data.gapThresholdMs / 2))
    : [];
  const hoverAnchor = hovered.find((point): point is VisualizationPoint => point !== null);

  const markerRadius =
    config.line.points === 'visible' ? 3 : config.line.points === 'small' ? 1.8 : 0;

  return (
    <div className="viz-chart" style={{ width, height }}>
      <svg
        className="viz-chart__svg"
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-label={t('viz.status.overTime', { name: meta.label })}
      >
        <defs>
          {data.series.map((_, index) => (
            <linearGradient
              key={index}
              id={`${gradientBase}-fill-${index}`}
              x1="0"
              y1="0"
              x2="0"
              y2="1"
            >
              <stop
                offset="0%"
                style={{ stopColor: colors.fill(index), stopOpacity: config.fill.opacity }}
              />
              <stop offset="100%" style={{ stopColor: colors.fill(index), stopOpacity: 0 }} />
            </linearGradient>
          ))}
          {thresholdGradient && (
            <linearGradient
              id={`${gradientBase}-threshold`}
              gradientUnits="userSpaceOnUse"
              x1="0"
              y1={plot.bottom}
              x2="0"
              y2={plot.top}
            >
              {thresholdStops(config, domain, colors.series(0)).map((stop, index) => (
                <stop key={index} offset={stop.offset} style={{ stopColor: stop.color }} />
              ))}
            </linearGradient>
          )}
        </defs>

        {showGrid &&
          yTicks.map((tick) => (
            <line
              key={`g${tick}`}
              className="viz-chart__grid"
              x1={plot.left}
              x2={plot.right}
              y1={y(tick)}
              y2={y(tick)}
              style={{ stroke: colors.grid }}
            />
          ))}

        {showY &&
          yTicks.map((tick) => (
            <text
              key={`y${tick}`}
              className="viz-chart__tick"
              x={plot.left - 6}
              y={y(tick)}
              textAnchor="end"
              dominantBaseline="middle"
              style={{ fill: colors.muted, fontSize: fontPx }}
            >
              {formatAxisValue(tick, meta.unit, domain.max - domain.min)}
            </text>
          ))}

        {showX &&
          xTicks.map((tick) => (
            <text
              key={`x${tick}`}
              className="viz-chart__tick"
              x={x(tick)}
              y={plot.bottom + fontPx + 4}
              textAnchor="middle"
              style={{ fill: colors.muted, fontSize: fontPx }}
            >
              {formatTime(tick, spanMs)}
            </text>
          ))}

        {drawn.map((segments, index) =>
          segments.map((drawnSegment, segmentIndex) => (
            <g
              key={`${index}-${segmentIndex}`}
              className="viz-chart__series"
              // `currentColor` for style effects (a glow follows its series).
              style={{ color: colors.series(index) }}
            >
              {drawnSegment.band && !chromeless && (
                <path
                  className="viz-chart__band"
                  d={drawnSegment.band}
                  style={{ fill: colors.series(index), fillOpacity: 0.12 }}
                />
              )}
              {fillMode !== 'none' && drawnSegment.segment.length > 1 && (
                <path
                  className="viz-chart__area"
                  d={drawnSegment.area}
                  style={
                    fillMode === 'gradient'
                      ? { fill: `url(#${gradientBase}-fill-${index})` }
                      : { fill: colors.fill(index), fillOpacity: config.fill.opacity }
                  }
                />
              )}
              <path
                className="viz-chart__line"
                d={drawnSegment.line}
                style={{
                  stroke: strokeFor(index),
                  strokeWidth: config.line.width,
                  fill: 'none',
                  strokeLinejoin: 'round',
                  strokeLinecap: 'round',
                }}
              />
              {markerRadius > 0 &&
                drawnSegment.segment.length <= MAX_MARKERS &&
                drawnSegment.segment.map((point) => (
                  <circle
                    key={point.t}
                    className="viz-chart__marker"
                    cx={x(point.t)}
                    cy={y(point.v)}
                    r={markerRadius}
                    style={{ fill: thresholdColor(config, point.v, colors.series(index)) }}
                  />
                ))}
            </g>
          )),
        )}

        {hover && hoverAnchor && (
          <g className="viz-chart__hover" pointerEvents="none">
            <line
              x1={x(hoverAnchor.t)}
              x2={x(hoverAnchor.t)}
              y1={plot.top}
              y2={plot.bottom}
              style={{ stroke: colors.muted, strokeOpacity: 0.5, strokeDasharray: '3 3' }}
            />
            {hovered.map((point, index) =>
              point ? (
                <circle
                  key={index}
                  cx={x(point.t)}
                  cy={y(point.v)}
                  r={3.5}
                  style={{
                    fill: colors.series(index),
                    stroke: colors.background,
                    strokeWidth: 1.5,
                  }}
                />
              ) : null,
            )}
          </g>
        )}

        <rect
          className="viz-chart__hit"
          x={plot.left}
          y={0}
          width={Math.max(0, plot.right - plot.left)}
          height={height}
          fill="transparent"
          onPointerMove={onPointerMove}
          onPointerLeave={() => setHover(null)}
        />
      </svg>

      {hover && hoverAnchor && config.display.tooltip && (
        <div
          className="viz-tooltip"
          role="tooltip"
          style={{
            left: x(hoverAnchor.t) > width * 0.6 ? undefined : x(hoverAnchor.t) + 12,
            right: x(hoverAnchor.t) > width * 0.6 ? width - x(hoverAnchor.t) + 12 : undefined,
          }}
        >
          <div className="viz-tooltip__time">{formatTooltipTime(hoverAnchor.t, spanMs)}</div>
          {hovered.map((point, index) =>
            point ? (
              <div key={index} className="viz-tooltip__row">
                <span
                  className="viz-tooltip__swatch"
                  style={{ background: colors.series(index) }}
                />
                <span className="viz-tooltip__label">{labels[index]}</span>
                <span className="viz-tooltip__value">
                  {formatValue(point.v, meta.unit, config.text.decimals ?? meta.decimals)}
                  {point.min !== undefined &&
                    point.max !== undefined &&
                    point.min !== point.max && (
                      <span className="viz-tooltip__range">
                        {` · ${formatValue(point.min, meta.unit, config.text.decimals ?? meta.decimals, false)}–${formatValue(point.max, meta.unit, config.text.decimals ?? meta.decimals)}`}
                      </span>
                    )}
                </span>
              </div>
            ) : null,
          )}
        </div>
      )}
    </div>
  );
}
