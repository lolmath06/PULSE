import { useMemo } from 'react';
import type { CSSProperties, ReactNode } from 'react';
import type { VisualizationConfig } from '@/visualization/config';
import { configuredHeight } from '@/visualization/config';
import { resolveColors } from '@/visualization/color';
import { formatParts, formatValue } from '@/visualization/format';
import { rendererInfo, rendererSupport } from '@/visualization/registry';
import { maxPointCount, summarize } from '@/visualization/series';
import type { SeriesSummary } from '@/visualization/series';
import type { VisualizationData, VisualizationMeta } from '@/visualization/types';
import { useElementSize } from '@/visualization/useElementSize';
import { TimeSeriesChart } from '@/visualization/renderers/TimeSeriesChart';
import { BarView, GaugeView, ValueView } from '@/visualization/renderers/InstantViews';

/**
 * **The** visualization entry point: render `data` about `meta` with `config`,
 * inside the box it is given.
 *
 * This is the whole contract a dashboard widget or a desktop overlay needs:
 *
 * ```tsx
 * <MetricVisualization data={data} meta={meta} config={config} width={120} height={40} />
 * ```
 *
 * - With `width` and `height`, the visualization fills exactly that rectangle —
 *   header, chart and statistics included.
 * - Without them, it fills its container's width and takes the configured
 *   height (Small, Medium, Large or Custom).
 *
 * It knows nothing about the page it is on, about history, SQLite or Tauri,
 * and nothing about which chart library draws the lines.
 */
export interface MetricVisualizationProps {
  readonly data: VisualizationData;
  readonly meta: VisualizationMeta;
  readonly config: VisualizationConfig;
  /** Exact outer width in px. Omit to fill the container. */
  readonly width?: number;
  /** Exact outer height in px. Omit to use the configured size. */
  readonly height?: number;
  readonly className?: string;
}

const FALLBACK_WIDTH = 600;
const WEIGHTS = { regular: 400, medium: 500, bold: 650 } as const;

function frameStyle(config: VisualizationConfig, border: string, glow: string): CSSProperties {
  return {
    borderRadius: config.frame.radius,
    border: config.frame.border === 'thin' ? `1px solid ${border}` : '1px solid transparent',
    boxShadow:
      config.frame.shadow === 'subtle'
        ? '0 2px 10px rgba(0, 0, 0, 0.35)'
        : config.frame.shadow === 'glow'
          ? `0 0 18px -6px ${glow}`
          : undefined,
  };
}

export function MetricVisualization({
  data,
  meta,
  config,
  width,
  height,
  className,
}: MetricVisualizationProps) {
  const [bodyRef, measured] = useElementSize<HTMLDivElement>();
  const colors = useMemo(() => resolveColors(config), [config]);
  const info = rendererInfo(config.renderer);
  const support = rendererSupport(config.renderer, meta, config);
  const compact = config.display.compact || config.renderer === 'sparkline';
  const exact = width !== undefined && height !== undefined;

  const labels = useMemo(
    () =>
      data.series.map(
        (series, index) => config.colors.series[String(index)]?.label ?? series.label,
      ),
    [data.series, config.colors.series],
  );
  const summaries: SeriesSummary[] = useMemo(() => data.series.map(summarize), [data.series]);
  const primary = summaries[0];
  const decimals = config.text.decimals ?? meta.decimals;
  const points = maxPointCount(data.series);
  const hasCurrent = summaries.some((summary) => summary.current !== null);

  const bodyWidth = measured.width || width || config.size.width || FALLBACK_WIDTH;
  const bodyHeight = exact
    ? measured.height || Math.max(8, height - (compact ? 4 : 64))
    : configuredHeight(config);

  const fontScale = config.text.scale;
  const rootStyle: CSSProperties = {
    ...frameStyle(config, colors.border, colors.series(0)),
    color: colors.text,
    fontWeight: WEIGHTS[config.text.weight],
    width: width ?? config.size.width ?? undefined,
    height: height,
    maxWidth: '100%',
  };
  const backdropStyle: CSSProperties = {
    borderRadius: config.frame.radius,
    opacity: config.background.opacity,
    background:
      config.background.mode === 'solid'
        ? colors.background
        : config.background.mode === 'gradient'
          ? `linear-gradient(160deg, ${colors.gradientStart}, ${colors.gradientEnd})`
          : 'transparent',
  };

  const showHeader = !compact && (config.text.showLabel || config.display.current);
  const currentText =
    primary?.current !== null && primary?.current !== undefined
      ? formatParts(primary.current, meta.unit, decimals)
      : null;

  let body: ReactNode;
  if (!support.ok) {
    body = <p className="viz__message">{support.reason}</p>;
  } else if (data.status === 'loading' && points === 0 && !hasCurrent) {
    body = <p className="viz__message">Loading history…</p>;
  } else if (data.status === 'unavailable' && (info.family === 'timeseries' || !hasCurrent)) {
    body = (
      <p className="viz__message viz__message--warning">
        {`History unavailable: ${data.message ?? 'unknown reason'}`}
      </p>
    );
  } else if (info.family === 'timeseries' && points === 0) {
    body = <p className="viz__message">Collecting history…</p>;
  } else if (info.family === 'timeseries' && points === 1) {
    body = (
      <p className="viz__message">
        {currentText ? `${formatValue(primary!.current!, meta.unit, decimals)} · ` : ''}
        Collecting history… one sample so far.
      </p>
    );
  } else if (info.family === 'instant' && !hasCurrent) {
    body = <p className="viz__message">Collecting history…</p>;
  } else {
    const common = { meta, config, colors, width: bodyWidth, height: bodyHeight, labels };
    switch (config.renderer) {
      case 'value':
        body = <ValueView {...common} series={data.series} summaries={summaries} />;
        break;
      case 'bar':
        body = <BarView {...common} series={data.series} summaries={summaries} />;
        break;
      case 'gauge':
        body = <GaugeView {...common} series={data.series} summaries={summaries} />;
        break;
      default:
        body = (
          <TimeSeriesChart
            {...common}
            data={data}
            variant={
              config.renderer === 'area'
                ? 'area'
                : config.renderer === 'sparkline'
                  ? 'sparkline'
                  : 'line'
            }
          />
        );
    }
  }

  const statistics = (
    [
      ['Current', 'current'],
      ['Min', 'min'],
      ['Max', 'max'],
      ['Avg', 'average'],
    ] as const
  ).filter(([, key]) => config.display[key]);

  return (
    <div
      className={`viz viz--${config.renderer}${compact ? ' viz--compact' : ''}${className ? ` ${className}` : ''}`}
      style={rootStyle}
      data-renderer={config.renderer}
    >
      <div className="viz__backdrop" style={backdropStyle} aria-hidden="true" />

      {showHeader && (
        <div className="viz__header" style={{ fontSize: 12.5 * fontScale }}>
          {config.text.showLabel && <span className="viz__label">{meta.label}</span>}
          {config.display.current && currentText && info.family === 'timeseries' && (
            <span className="viz__current" style={{ fontSize: 20 * fontScale }}>
              {currentText.value}
              {config.text.showUnit && currentText.unit && (
                <span className="viz__unit">{` ${currentText.unit}`}</span>
              )}
            </span>
          )}
          {meta.secondary && info.family === 'timeseries' && (
            <span className="viz__secondary">{meta.secondary}</span>
          )}
        </div>
      )}

      {compact && (config.text.showLabel || config.display.current) && currentText && (
        <div className="viz__overlay" style={{ fontSize: 11 * fontScale }}>
          {config.text.showLabel && <span className="viz__overlay-label">{meta.label}</span>}
          {config.display.current && (
            <span className="viz__overlay-value">
              {currentText.value}
              {config.text.showUnit && currentText.unit ? ` ${currentText.unit}` : ''}
            </span>
          )}
        </div>
      )}

      <div
        ref={bodyRef}
        className="viz__body"
        style={exact ? { flex: '1 1 auto', minHeight: 0 } : { height: bodyHeight }}
      >
        {body}
      </div>

      {!compact && config.display.legend && data.series.length > 1 && (
        <ul className="viz__legend" style={{ fontSize: 11.5 * fontScale }}>
          {data.series.map((series, index) => (
            <li key={series.id} className="viz__legend-item">
              <span className="viz__swatch" style={{ background: colors.series(index) }} />
              {labels[index]}
              {series.points.length === 0 && !series.latest && (
                <span className="viz__legend-empty"> · no data</span>
              )}
            </li>
          ))}
        </ul>
      )}

      {!compact && statistics.length > 0 && hasCurrent && (
        <dl className="viz__stats" style={{ fontSize: 11.5 * fontScale }}>
          {summaries.map((summary, index) => (
            <div key={index} className="viz__stats-row">
              {summaries.length > 1 && <dt className="viz__stats-series">{labels[index]}</dt>}
              {statistics.map(([label, key]) => (
                <div key={key} className="viz__stat">
                  <dt>{label}</dt>
                  <dd>
                    {summary[key] === null
                      ? '—'
                      : formatValue(summary[key]!, meta.unit, decimals, config.text.showUnit)}
                  </dd>
                </div>
              ))}
            </div>
          ))}
        </dl>
      )}
    </div>
  );
}
