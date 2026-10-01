import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { LocaleCode } from '@/i18n/locales';
import { LOCALE_CODES } from '@/i18n/locales';
import { LOCALE_MESSAGES, RESOURCES } from '@/i18n/resources';

/**
 * The catalogs, checked against English: every key present, nothing extra,
 * the same placeholders and inline tags, and the plural forms each language
 * actually uses (from `Intl.PluralRules`, not a hand-kept list).
 */

const SRC = join(__dirname, '..');
const LOCALES_DIR = join(__dirname, 'locales');
const PLURAL = /_(zero|one|two|few|many|other)$/;

function flatten(node: unknown, prefix = '', out = new Map<string, unknown>()) {
  if (node && typeof node === 'object' && !Array.isArray(node)) {
    for (const [key, value] of Object.entries(node)) {
      flatten(value, prefix ? `${prefix}.${key}` : key, out);
    }
  } else {
    out.set(prefix, node);
  }
  return out;
}

const placeholders = (text: string) =>
  [...text.matchAll(/\{\{\s*(\w+)\s*\}\}/g)].map((match) => match[1]!).sort();
const tags = (text: string) => [...text.matchAll(/<\/?(\w+)>/g)].map((match) => match[0]).sort();

const EN = flatten(LOCALE_MESSAGES.en);

/** Plural bases in English (`foo` for `foo_one` / `foo_other`). */
const PLURAL_BASES = new Set(
  [...EN.keys()].filter((key) => PLURAL.test(key)).map((key) => key.replace(PLURAL, '')),
);
const SINGULAR_KEYS = [...EN.keys()].filter((key) => !PLURAL.test(key));

function pluralCategories(code: LocaleCode): readonly string[] {
  return new Intl.PluralRules(code).resolvedOptions().pluralCategories;
}

describe('bundled catalogs', () => {
  it('registers exactly one catalog per locale, each with a file', () => {
    expect(Object.keys(LOCALE_MESSAGES).sort()).toEqual([...LOCALE_CODES].sort());
    expect(Object.keys(RESOURCES).sort()).toEqual([...LOCALE_CODES].sort());
    const files = readdirSync(LOCALES_DIR)
      .filter((name) => name.endsWith('.json'))
      .map((name) => name.replace(/\.json$/, ''))
      .sort();
    expect(files).toEqual([...LOCALE_CODES].sort());
  });

  it('has a substantial English reference', () => {
    expect(SINGULAR_KEYS.length).toBeGreaterThan(1000);
    expect(PLURAL_BASES.size).toBeGreaterThan(0);
  });

  it.each(LOCALE_CODES)('%s has no duplicate key in its file', (code) => {
    const raw = readFileSync(join(LOCALES_DIR, `${code}.json`), 'utf8');
    // A JSON object keeps only the last of two equal keys; count per object.
    const stack: Set<string>[] = [];
    const duplicates: string[] = [];
    const token = /"((?:[^"\\]|\\.)*)"\s*:|[{}]/g;
    for (const match of raw.matchAll(token)) {
      if (match[0] === '{') stack.push(new Set());
      else if (match[0] === '}') stack.pop();
      else {
        const scope = stack.at(-1)!;
        if (scope.has(match[1]!)) duplicates.push(match[1]!);
        scope.add(match[1]!);
      }
    }
    expect(duplicates).toEqual([]);
  });

  describe.each(LOCALE_CODES)('%s', (code) => {
    const messages = flatten(LOCALE_MESSAGES[code]);
    const categories = pluralCategories(code);

    it('has every English key, as non-empty text, with the same placeholders and tags', () => {
      const problems: string[] = [];
      for (const key of SINGULAR_KEYS) {
        const reference = EN.get(key) as string;
        const value = messages.get(key);
        if (typeof value !== 'string') problems.push(`missing ${key}`);
        else if (!value.trim()) problems.push(`empty ${key}`);
        else {
          if (placeholders(value).join() !== placeholders(reference).join()) {
            problems.push(`placeholders ${key}`);
          }
          if (tags(value).join() !== tags(reference).join()) problems.push(`tags ${key}`);
        }
      }
      expect(problems).toEqual([]);
    });

    it('has every plural form the language needs, and keeps the placeholders', () => {
      const problems: string[] = [];
      for (const base of PLURAL_BASES) {
        const reference = new Set(placeholders(EN.get(`${base}_other`) as string));
        reference.delete('count');
        for (const category of categories) {
          const value = messages.get(`${base}_${category}`);
          if (typeof value !== 'string' || !value.trim()) {
            problems.push(`missing ${base}_${category}`);
            continue;
          }
          const own = new Set(placeholders(value));
          own.delete('count');
          if ([...own].sort().join() !== [...reference].sort().join()) {
            problems.push(`placeholders ${base}_${category}`);
          }
        }
      }
      expect(problems).toEqual([]);
    });

    it('has no key English does not have', () => {
      const extra = [...messages.keys()].filter((key) => {
        if (!PLURAL.test(key)) return !EN.has(key);
        return !PLURAL_BASES.has(key.replace(PLURAL, ''));
      });
      expect(extra).toEqual([]);
    });
  });
});

// --- keys the code asks for -----------------------------------------------

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return sourceFiles(path);
    return /\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name) ? [path] : [];
  });
}

/** Literal keys: `t('a.b')`, `i18nKey="a.b"`, and `…Key: 'a.b'` fields. */
const KEY_PATTERNS = [
  /\bt\(\s*'([a-zA-Z][\w-]*(?:\.[\w-]+)+)'/g,
  /\bi18nKey=["']([a-zA-Z][\w-]*(?:\.[\w-]+)+)["']/g,
  /\b\w*Key:\s*'([a-zA-Z][\w-]*(?:\.[\w-]+)+)'/g,
];

describe('keys used in the code', () => {
  const used = new Map<string, string>();
  for (const file of sourceFiles(SRC)) {
    const text = readFileSync(file, 'utf8');
    for (const pattern of KEY_PATTERNS) {
      for (const match of text.matchAll(pattern)) used.set(match[1]!, relative(SRC, file));
    }
  }

  it('finds the literal keys', () => {
    expect(used.size).toBeGreaterThan(500);
  });

  it('every literal key exists in English', () => {
    const missing = [...used]
      .filter(([key]) => !EN.has(key) && !PLURAL_BASES.has(key) && !isBranch(key))
      .map(([key, file]) => `${file}: ${key}`);
    expect(missing).toEqual([]);
  });
});

function isBranch(key: string) {
  const prefix = `${key}.`;
  return [...EN.keys()].some((candidate) => candidate.startsWith(prefix));
}
