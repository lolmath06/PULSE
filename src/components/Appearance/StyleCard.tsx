import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import type { Look } from '@/design/look';
import { StyleScope } from '@/design/LookContext';
import { Icon } from '@/components/Icon';

/** A fixed trace, so every card shows the same "data" and only the style differs. */
const TRACE = [38, 44, 41, 52, 49, 61, 57, 66, 58, 63, 71, 64, 69, 62, 74];

function tracePath(width: number, height: number): { line: string; area: string } {
  const step = width / (TRACE.length - 1);
  const y = (v: number) => height - ((v - 30) / 50) * height;
  const points = TRACE.map((v, i) => `${(i * step).toFixed(1)},${y(v).toFixed(1)}`);
  const line = `M${points.join(' L')}`;
  return { line, area: `${line} L${width},${height} L0,${height} Z` };
}

/**
 * A miniature of a style: one card with a title, a number, a trend and three
 * meters, drawn with the style's own tokens. Static — eight of them cost
 * nothing — and identical in content, so only the look differs.
 */
export function StyleCard({
  look,
  name,
  tagline,
  active,
  onSelect,
  badge,
}: {
  readonly look: Look;
  readonly name: string;
  readonly tagline: string;
  readonly active: boolean;
  readonly onSelect: () => void;
  readonly badge?: string;
}) {
  const { t } = useTranslation();
  const { line, area } = tracePath(160, 46);
  // Unique per card: a name in any script (or two cards of one style) never collides.
  const gradient = `style-card-fill-${useId().replace(/[^a-z0-9]/gi, '')}`;
  return (
    <button
      type="button"
      className={`style-card${active ? ' style-card--active' : ''}`}
      aria-pressed={active}
      aria-label={t('appearance.styleAria', { name })}
      onClick={onSelect}
    >
      <StyleScope look={look} className="style-card__canvas">
        <span className="style-card__panel">
          <span className="style-card__head">
            <span className="style-card__title">CPU</span>
            <span className="style-card__value">
              64<small>%</small>
            </span>
          </span>
          <svg className="style-card__chart" viewBox="0 0 160 46" preserveAspectRatio="none">
            <defs>
              <linearGradient id={gradient} x1="0" x2="0" y1="0" y2="1">
                <stop offset="0%" stopColor="var(--pulse-viz-1)" stopOpacity="0.45" />
                <stop offset="100%" stopColor="var(--pulse-viz-1)" stopOpacity="0" />
              </linearGradient>
            </defs>
            <path d={area} fill={`url(#${gradient})`} />
            <path
              d={line}
              className="style-card__line"
              fill="none"
              stroke="var(--pulse-viz-1)"
              strokeWidth="2"
              strokeLinejoin="round"
              strokeLinecap="round"
              vectorEffect="non-scaling-stroke"
            />
          </svg>
          <span className="style-card__meters">
            {[72, 41, 58].map((value, index) => (
              <span key={index} className="style-card__meter">
                <span style={{ width: `${value}%`, background: `var(--pulse-viz-${index + 1})` }} />
              </span>
            ))}
          </span>
        </span>
      </StyleScope>
      <span className="style-card__caption">
        <span className="style-card__name">
          {name}
          {active && <Icon name="check" className="style-card__check" />}
        </span>
        <span className="style-card__tagline">{badge ?? tagline}</span>
      </span>
    </button>
  );
}
