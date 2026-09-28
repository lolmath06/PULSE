/**
 * Identifiers for dashboards, widgets, overlays and templates.
 *
 * Random, short, lowercase, `[a-z0-9-]` only — never derived from a name the
 * user typed. The overlay window label is built from one (`overlay-<id>`), and
 * the backend re-validates it with the same pattern.
 */
export const ID_PATTERN = /^[a-z0-9][a-z0-9-]{0,39}$/;

export function isValidId(value: unknown): value is string {
  return typeof value === 'string' && ID_PATTERN.test(value);
}

export function newId(prefix = 'w'): string {
  const bytes = new Uint8Array(8);
  globalThis.crypto.getRandomValues(bytes);
  const random = [...bytes]
    .map((byte) => byte.toString(36).padStart(2, '0'))
    .join('')
    .slice(0, 12);
  return `${prefix}-${random}`;
}
