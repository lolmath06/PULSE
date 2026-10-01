import { englishText, hasKey, i18n } from '@/i18n/i18n';

/**
 * Built-in text inside persisted data.
 *
 * A widget made from a template carries labels such as "Download" and titles
 * such as "Temperatures". They are PULSE's words, not the user's, so they must
 * follow the interface language — yet the widget is saved, exported and
 * possibly edited. Such text is therefore stored as a pair:
 *
 * - a stable translation key (`labelKey`, `titleKey`, `nameKey`), which is what
 *   the display uses whenever the running PULSE knows the key;
 * - the English text, which an older PULSE (or a missing key) shows instead.
 *
 * Nothing translated is ever persisted. As soon as the user types their own
 * text, the key is dropped and their words are kept exactly as typed.
 */

/** A reference to a built-in string by its translation key. */
export interface TextRef {
  readonly key: string;
}

/** Text in a built-in definition: language-neutral as is (`CPU`, `↓`), or a key. */
export type Text = string | TextRef;

const KEY_PATTERN = /^[a-z][A-Za-z0-9_-]*(\.[A-Za-z0-9_-]+){1,8}$/;

/** Whether a stored value is a well-formed translation key. */
export function isTextKey(value: unknown): value is string {
  return typeof value === 'string' && value.length <= 128 && KEY_PATTERN.test(value);
}

/** The key of a built-in text, or `null` for language-neutral text. */
export function keyOf(text: Text | undefined): string | null {
  return typeof text === 'object' && text !== null ? text.key : null;
}

/** The persisted fallback of a built-in text: always English. */
export function englishOf(text: Text): string {
  return typeof text === 'string' ? text : englishText(text.key);
}

/** The text in the active language (React components re-render on change). */
export function display(text: Text): string {
  return typeof text === 'string' ? text : i18n.t(text.key);
}

/**
 * What to show for a stored built-in text: its translation when this PULSE
 * knows the key, otherwise the stored text exactly.
 */
export function storedText(key: string | null | undefined, stored: string): string;
export function storedText(key: string | null | undefined, stored: string | null): string | null;
export function storedText(key: string | null | undefined, stored: string | null): string | null {
  return key && hasKey(key) ? i18n.t(key) : stored;
}
