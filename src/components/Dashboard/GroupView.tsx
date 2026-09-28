import { useMemo } from 'react';
import { metricRefId } from '@/types/wellknown';
import type { LivePoint } from '@/live/liveFeed';
import type { VisualizationConfig } from '@/visualization/config';
import { mergeConfig } from '@/visualization/config';
import { resolveColors, thresholdColor } from '@/visualization/color';
import { formatValue } from '@/visualization/format';
import { MetricVisualization } from '@/visualization/MetricVisualization';
import type { ResolvedBinding } from '@/dashboard/bindings';
import { liveSeriesOf } from '@/dashboard/widgetData';
import { metaFor } from '@/dashboard/metricInfo';
import { describeAvailability } from '@/utils/metrics';

/**
 * Several related values in one widget — a *group* (rows: `Usage 37 %`,
 * `Temp 68 °C`) or a *summary* strip (`CPU 24 % | GPU 84 % | RAM 41 %`),
 * optionally each with a tiny live trend.
 *
 * Each value keeps its own unit and its own honesty: an unreadable metric
 * shows `—` with the reason, never `0`.
 */
export function GroupView({
  resolved,
  live,
  config,
  orientation,
  sparklines,
  width,
  height,
}: {
  readonly resolved: readonly ResolvedBinding[];
  readonly live: ReadonlyMap<string, readonly LivePoint[]>;
  readonly config: VisualizationConfig;
  readonly orientation: 'rows' | 'inline';
  readonly sparklines: boolean;
  readonly width: number;
  readonly height: number;
}) {
  const colors = useMemo(() => resolveColors(config), [config]);
  const sparkConfig = useMemo(
    () =>
      mergeConfig(config, {
        renderer: 'sparkline',
        display: { compact: true, current: false },
        text: { showLabel: false },
        background: { mode: 'none', opacity: 0 },
        frame: { border: 'none', radius: 0, shadow: 'none' },
      }),
    [config],
  );
  const count = Math.max(1, resolved.length);
  const inline = orientation === 'inline';
  const cellWidth = inline ? width / count : width;
  const rowHeight = inline ? height : height / count;
  const fontPx = Math.max(9, Math.min(20, rowHeight * (inline ? 0.32 : 0.5))) * config.text.scale;
  const showSpark = sparklines && (inline ? rowHeight >= 34 : cellWidth >= 160);

  return (
    <ul
      className={`widget-group widget-group--${orientation}`}
      style={{ width, height, color: colors.text, fontSize: fontPx }}
    >
      {resolved.map((entry, index) => {
        if (!entry.ok) {
          return (
            <li key={index} className="widget-group__item" title={entry.reason}>
              <span className="widget-group__label">{entry.label}</span>
              <span className="widget-group__value value--unavailable">—</span>
            </li>
          );
        }
        const id = metricRefId(entry.ref);
        const points = live.get(id) ?? [];
        const series = liveSeriesOf(id, entry.label, points);
        const unit = metaFor(entry.binding.key, entry.label, entry.definition);
        const readable = entry.definition.availability.status === 'available';
        const current = series.latest?.v ?? null;
        const text =
          current === null
            ? '—'
            : formatValue(
                current,
                unit.unit,
                config.text.decimals ?? unit.decimals,
                config.text.showUnit,
              );
        return (
          <li
            key={id}
            className="widget-group__item"
            title={readable ? undefined : describeAvailability(entry.definition.availability)}
          >
            <span className="widget-group__label" style={{ color: colors.muted }}>
              {entry.label}
            </span>
            <span
              className={`widget-group__value${current === null ? ' value--unavailable' : ''}`}
              style={{ color: thresholdColor(config, current, colors.text) }}
            >
              {text}
            </span>
            {showSpark && series.points.length > 1 && (
              <MetricVisualization
                className="widget-group__spark"
                data={{ status: 'ready', series: [series], gapThresholdMs: 3_000 }}
                meta={unit}
                config={sparkConfig}
                width={inline ? Math.max(24, Math.min(90, cellWidth * 0.35)) : 80}
                height={Math.max(12, Math.min(28, rowHeight * 0.55))}
              />
            )}
          </li>
        );
      })}
    </ul>
  );
}
