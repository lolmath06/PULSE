import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
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
import { fitLine, presentationFor, renderedConfig } from '@/visualization/presentation';
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
  config: storedConfig,
  width,
  height,
  className,
}: MetricVisualizationProps) {
  const { t } = useTranslation();
  const [rootRef, measured] = useElementSize<HTMLDivElement>();
  const exact = width !== undefined && height !== undefined;

  const boxWidth = width ?? (measured.width || storedConfig.size.width || FALLBACK_WIDTH);
  const boxHeight = exact ? height : configuredHeight(storedConfig);

  // Responsive density: decided from the real box, applied to rendering only.
  const presentation = useMemo(
    () =>
      presentationFor({
        config: storedConfig,
        width: boxWidth,
        height: boxHeight,
        seriesCount: data.series.length,
        exact,
      }),
    [storedConfig, boxWidth, boxHeight, data.series.length, exact],
  );
  const config = useMemo(
    () => renderedConfig(storedConfig, presentation),
    [storedConfig, presentation],
  );

  const colors = useMemo(() => resolveColors(config), [config]);
  const info = rendererInfo(config.renderer);
  const support = rendererSupport(config.renderer, meta, config);

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

  const bodyWidth = Math.max(8, boxWidth - 2);
  const bodyHeight = Math.max(8, presentation.heights.body);

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

  const currentText =
    primary?.current !== null && primary?.current !== undefined
      ? formatParts(primary.current, meta.unit, decimals)
      : null;

  const micro = presentation.density === 'micro';
  // In a micro box the message shrinks to a glyph; the full reason stays in
  // the tooltip and the accessible name, so it is never lost.
  const message = (text: string, warning = false, short = '…') =>
    info.family === 'instant' && short === '—' && !micro ? (
      // A number, bar or ring with nothing to show: its name and a dash, the
      // reason small beneath — never a paragraph where a figure belongs.
      <p className="viz__message viz__message--instant" title={text} aria-label={text}>
        <span className="viz__message-value">{config.text.showLabel ? `${meta.label} ` : ''}—</span>
        <span className="viz__message-reason">{text}</span>
      </p>
    ) : (
      <p
        className={`viz__message${warning ? ' viz__message--warning' : ''}`}
        title={text}
        aria-label={text}
      >
        {/* A micro box keeps its label, so a missing reading is never a lone dash. */}
        {micro ? (config.text.showLabel && short === '—' ? `${meta.label} —` : short) : text}
      </p>
    );

  let body: ReactNode;
  if (!support.ok) {
    body = message(support.reason, false, '—');
  } else if (data.status === 'loading' && points === 0 && !hasCurrent) {
    body = message(t('viz.status.loading'));
  } else if (data.status === 'unavailable' && (info.family === 'timeseries' || !hasCurrent)) {
    body = message(
      t('viz.status.unavailable', {
        reason: data.message ?? t('history.status.unknownReason'),
      }),
      true,
      '—',
    );
  } else if (info.family === 'timeseries' && points === 0) {
    body = message(t('viz.status.collecting'));
  } else if (info.family === 'timeseries' && points === 1) {
    const current = currentText ? formatValue(primary!.current!, meta.unit, decimals) : null;
    body = message(
      `${current ? `${current} · ` : ''}${t('viz.status.oneSample')}`,
      false,
      current ?? '…',
    );
  } else if (info.family === 'instant' && !hasCurrent) {
    body = message(t('viz.status.collecting'), false, '—');
  } else if (presentation.strip) {
    body = (
      <MicroStrip
        data={data}
        meta={meta}
        config={config}
        colors={colors}
        width={bodyWidth}
        height={bodyHeight}
        labels={labels}
        currentText={currentText}
      />
    );
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
      [t('viz.stats.current'), 'current'],
      [t('viz.stats.min'), 'min'],
      [t('viz.stats.max'), 'max'],
      [t('viz.stats.average'), 'average'],
    ] as const
  ).filter(([, key]) => config.display[key]);

  const headerLine = (full: boolean) => (
    <div
      className={`viz__header${full ? '' : ' viz__header--inline'}`}
      style={{ fontSize: 12.5 * fontScale, height: presentation.heights.header }}
    >
      {config.text.showLabel && <span className="viz__label">{meta.label}</span>}
      {config.display.current && currentText && (
        <span className="viz__current" style={{ fontSize: (full ? 20 : 12.5) * fontScale }}>
          {currentText.value}
          {config.text.showUnit && currentText.unit && (
            <span className="viz__unit">{` ${currentText.unit}`}</span>
          )}
        </span>
      )}
      {full && meta.secondary && <span className="viz__secondary">{meta.secondary}</span>}
    </div>
  );

  return (
    <div
      ref={rootRef}
      className={`viz viz--${config.renderer} viz--density-${presentation.density}${
        config.display.compact ? ' viz--compact' : ''
      }${className ? ` ${className}` : ''}`}
      style={rootStyle}
      data-renderer={config.renderer}
      data-density={presentation.density}
    >
      <div className="viz__backdrop" style={backdropStyle} aria-hidden="true" />

      {presentation.header !== 'none' && headerLine(presentation.header === 'full')}

      <div className="viz__body" style={{ height: bodyHeight }}>
        {body}
      </div>

      {presentation.legend && (
        <ul
          className="viz__legend"
          style={{ fontSize: 11.5 * fontScale, height: presentation.heights.legend }}
        >
          {data.series.map((series, index) => (
            <li key={series.id} className="viz__legend-item">
              <span className="viz__swatch" style={{ background: colors.series(index) }} />
              {labels[index]}
              {series.points.length === 0 && !series.latest && (
                <span className="viz__legend-empty"> · {t('viz.status.noData')}</span>
              )}
            </li>
          ))}
        </ul>
      )}

      {presentation.stats && statistics.length > 0 && hasCurrent && (
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

/**
 * The micro presentation of a time series: `CPU 7 %` and, beside it — never
 * on top of it — a sparkline when at least 28 px are left for one.
 */
function MicroStrip({
  data,
  meta,
  config,
  colors,
  width,
  height,
  labels,
  currentText,
}: {
  readonly data: VisualizationData;
  readonly meta: VisualizationMeta;
  readonly config: VisualizationConfig;
  readonly colors: ReturnType<typeof resolveColors>;
  readonly width: number;
  readonly height: number;
  readonly labels: readonly string[];
  readonly currentText: { value: string; unit: string } | null;
}) {
  const wantsSpark = width >= 90;
  const textBudget = wantsSpark ? Math.max(40, width * 0.55) : width;
  const fit = fitLine(
    textBudget,
    height,
    {
      label: config.text.showLabel ? meta.label : null,
      value: currentText && config.display.current ? currentText.value : '—',
      unit: config.text.showUnit ? currentText?.unit : null,
    },
    config.text.scale,
    18,
  );
  // A trend never stretches into a ribbon: in a wide cell (a full-width bar)
  // it keeps a readable length and the pair is centred.
  const sparkWidth = Math.min(180, width - Math.min(fit.textWidth, textBudget) - 6);
  const showSpark = wantsSpark && sparkWidth >= 28;
  return (
    <div
      className="viz-strip"
      style={{ width, height, justifyContent: width > 360 ? 'center' : undefined }}
    >
      <span
        className="viz-strip__text"
        style={{ fontSize: fit.fontPx, maxWidth: showSpark ? textBudget : width }}
      >
        {fit.label && <span className="viz-strip__label">{meta.label} </span>}
        <span className="viz-strip__value">
          {currentText && config.display.current ? currentText.value : '—'}
          {fit.unit && currentText?.unit ? ` ${currentText.unit}` : ''}
        </span>
      </span>
      {showSpark && (
        <TimeSeriesChart
          variant="sparkline"
          data={data}
          meta={meta}
          config={config}
          colors={colors}
          width={sparkWidth}
          height={height}
          labels={labels}
        />
      )}
    </div>
  );
}
