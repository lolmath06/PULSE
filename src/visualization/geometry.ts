/** Small SVG geometry helpers shared by the renderers. */

function polar(cx: number, cy: number, radius: number, degrees: number) {
  const radians = ((degrees - 90) * Math.PI) / 180;
  return { x: cx + radius * Math.cos(radians), y: cy + radius * Math.sin(radians) };
}

/** An SVG arc path from `start` to `end` degrees, clockwise from 12 o'clock. */
export function arcPath(
  cx: number,
  cy: number,
  radius: number,
  start: number,
  end: number,
): string {
  if (end - start <= 0.01) return '';
  const from = polar(cx, cy, radius, start);
  const to = polar(cx, cy, radius, end);
  const large = end - start > 180 ? 1 : 0;
  return `M ${from.x.toFixed(2)} ${from.y.toFixed(2)} A ${radius} ${radius} 0 ${large} 1 ${to.x.toFixed(2)} ${to.y.toFixed(2)}`;
}
