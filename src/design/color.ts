/**
 * Small colour arithmetic for the design system. Inputs are `#rrggbb` (the
 * only format PULSE stores); outputs are CSS strings.
 */

const HEX = /^#([0-9a-f]{6})$/i;

export function isHex(value: unknown): value is string {
  return typeof value === 'string' && HEX.test(value);
}

export function rgbOf(hex: string): [number, number, number] {
  const match = HEX.exec(hex);
  if (!match) return [0, 0, 0];
  const n = Number.parseInt(match[1]!, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function clamp01(value: number): number {
  return Math.min(1, Math.max(0, Number.isFinite(value) ? value : 0));
}

/** `#rrggbb` at `alpha`, as `rgba(…)`; opaque colours stay hex. */
export function alpha(hex: string, a: number): string {
  const value = clamp01(a);
  if (value >= 1) return hex.toLowerCase();
  const [r, g, b] = rgbOf(hex);
  return `rgba(${r}, ${g}, ${b}, ${Number(value.toFixed(3))})`;
}

/** `a` mixed toward `b` by `t` (0 → a, 1 → b), as `#rrggbb`. */
export function mix(a: string, b: string, t: number): string {
  const k = clamp01(t);
  const [ar, ag, ab] = rgbOf(a);
  const [br, bg, bb] = rgbOf(b);
  const channel = (x: number, y: number) =>
    Math.round(x + (y - x) * k)
      .toString(16)
      .padStart(2, '0');
  return `#${channel(ar, br)}${channel(ag, bg)}${channel(ab, bb)}`;
}

/** WCAG relative luminance, 0–1. */
export function luminance(hex: string): number {
  const [r, g, b] = rgbOf(hex).map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG contrast ratio between two colours, 1–21. */
export function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

/** Black or white, whichever reads better on `hex`. */
export function onColor(hex: string): string {
  return contrast(hex, '#000000') >= contrast(hex, '#ffffff') ? '#05070a' : '#ffffff';
}
