import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import ts from 'typescript';
import { describe, expect, it } from 'vitest';

/**
 * A permanent audit for user-visible English left in the interface code.
 *
 * It parses every component and module under `src/` (tests, fixtures, the
 * i18n layer itself and type mirrors excluded) and reports:
 *
 * - JSX text containing words, e.g. `<p>Loading…</p>`;
 * - string literals given to the attributes a user sees or hears —
 *   `aria-label`, `title`, `placeholder`, `alt`, and PULSE's own `label`,
 *   `text`, `hint`, `confirmLabel`… props;
 * - template literals with words in those same attributes.
 *
 * It deliberately ignores comments, class names, keys, ids, CSS, units and
 * technical identifiers. A string that must stay as it is — a product name,
 * a protocol, a unit — goes in {@link ALLOWED} with the reason, so every
 * exception is a reviewed decision rather than an accident.
 */

const ROOT = join(__dirname, '..');

/** Directories and files that hold no interface copy, or copy by design. */
const SKIPPED = [/\.test\.tsx?$/, /^test\//, /^i18n\//, /^types\//, /fixtures\.ts$/, /\.d\.ts$/];

/** Attributes and props whose value is shown to (or read to) the user. */
const VISIBLE_ATTRIBUTES = new Set([
  'aria-label',
  'aria-description',
  'title',
  'placeholder',
  'alt',
  'label',
  'text',
  'hint',
  'note',
  'subtitle',
  'footnote',
  'ariaLabel',
  'confirmLabel',
  'followLabel',
  'actionLabel',
  'short',
]);

/**
 * Exact strings that are correct in every language: product and protocol
 * names, hardware abbreviations, units and symbols. Each one is visible on
 * purpose.
 */
const ALLOWED = new Set([
  // Product, platform and protocol names.
  'PULSE',
  'PULSE Mini',
  'GNOME Shell',
  'Windows',
  'Linux',
  'macOS',
  'X11',
  'WAL',
  'NORMAL',
  // Hardware abbreviations used as-is in every language PULSE ships.
  'CPU',
  'GPU',
  'RAM',
  'VRAM',
  'SSD',
  'Wi-Fi',
  'MTU',
  'PID',
  'SHA-256',
  'IOPS',
  // Single-letter axis and size labels (geometry, not words).
  'W',
  'H',
  'X',
  'Y',
]);

/** Words in a string: at least two letters in a row, in any script. */
const WORDS = /\p{L}{2,}/u;

interface Finding {
  readonly file: string;
  readonly line: number;
  readonly text: string;
}

function sourceFiles(directory: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) out.push(...sourceFiles(path));
    else if (/\.(ts|tsx)$/.test(name)) out.push(path);
  }
  return out;
}

function visible(text: string): boolean {
  // Separators around an allowed word (`· PID`, `MTU:`) do not make it prose.
  const trimmed = text
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/^[\s·•—–:|/(]+|[\s·•—–:|/)]+$/gu, '');
  if (trimmed === '' || ALLOWED.has(trimmed)) return false;
  // Colours and CSS are never copy.
  if (/^#[0-9a-f]{3,8}$/i.test(trimmed) || /^var\(/.test(trimmed)) return false;
  // `CPU °C`, `GPU %`: allowed abbreviations with units or symbols only.
  const rest = trimmed
    .split(/\s+/)
    .filter((word) => !ALLOWED.has(word))
    .join(' ');
  return WORDS.test(rest);
}

/** An English fallback stored beside its translation key (`{ label, labelKey }`). */
function hasKeySibling(node: ts.PropertyAssignment): boolean {
  const object = node.parent;
  if (!ts.isObjectLiteralExpression(object)) return false;
  const key = `${node.name.getText()}Key`;
  return object.properties.some(
    (property) => property.name !== undefined && property.name.getText() === key,
  );
}

/**
 * Object properties whose string value becomes interface copy when a
 * component renders the object (`{ label: 'Done' }`).
 */
const VISIBLE_PROPERTIES = new Set([
  'label',
  'title',
  'description',
  'tagline',
  'detail',
  'summary',
  'message',
  'reason',
  'text',
  'confirmLabel',
  'body',
  'name',
]);

function audit(path: string): Finding[] {
  const file = relative(ROOT, path);
  const source = ts.createSourceFile(
    path,
    readFileSync(path, 'utf8'),
    ts.ScriptTarget.Latest,
    true,
    path.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );
  const findings: Finding[] = [];
  const report = (node: ts.Node, text: string) =>
    findings.push({
      file,
      line: source.getLineAndCharacterOfPosition(node.getStart()).line + 1,
      text: text.replace(/\s+/g, ' ').trim(),
    });

  const visit = (node: ts.Node) => {
    if (ts.isJsxText(node) && visible(node.text)) {
      // `<code>` content is a command or an identifier, never prose.
      const parent = node.parent;
      const tag =
        ts.isJsxElement(parent) && ts.isIdentifier(parent.openingElement.tagName)
          ? parent.openingElement.tagName.text
          : null;
      if (tag !== 'code') report(node, node.text);
    }
    if (
      ts.isPropertyAssignment(node) &&
      VISIBLE_PROPERTIES.has(node.name.getText(source)) &&
      !hasKeySibling(node) &&
      (ts.isStringLiteral(node.initializer) ||
        ts.isNoSubstitutionTemplateLiteral(node.initializer)) &&
      visible(node.initializer.text)
    ) {
      report(node.initializer, node.initializer.text);
    }
    if (
      ts.isJsxAttribute(node) &&
      VISIBLE_ATTRIBUTES.has(node.name.getText(source)) &&
      node.initializer
    ) {
      const value = node.initializer;
      if (ts.isStringLiteral(value) && visible(value.text)) report(value, value.text);
      if (ts.isJsxExpression(value) && value.expression) {
        const expression = value.expression;
        if (ts.isStringLiteral(expression) && visible(expression.text)) {
          report(expression, expression.text);
        }
        if (ts.isTemplateExpression(expression)) {
          const literal = [
            expression.head.text,
            ...expression.templateSpans.map((s) => s.literal.text),
          ].join(' ');
          if (visible(literal)) report(expression, literal);
        }
        if (ts.isNoSubstitutionTemplateLiteral(expression) && visible(expression.text)) {
          report(expression, expression.text);
        }
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  return findings;
}

describe('hard-coded interface strings', () => {
  it('leaves no user-visible English in components (beyond the reviewed allowlist)', () => {
    const findings = sourceFiles(ROOT)
      .filter((path) => !SKIPPED.some((pattern) => pattern.test(relative(ROOT, path))))
      .flatMap(audit);
    expect(findings.map((f) => `${f.file}:${f.line} ${JSON.stringify(f.text)}`)).toEqual([]);
  });

  it('detects the patterns it is meant to catch', () => {
    // A self-check, so a refactor of the audit cannot silently make it blind.
    const path = join(ROOT, 'i18n', '__audit_probe__.tsx');
    const probe = ts.createSourceFile(
      path,
      'const a = <p title="Close the panel">Hello there <code>pnpm dev</code></p>;',
      ts.ScriptTarget.Latest,
      true,
      ts.ScriptKind.TSX,
    );
    const texts: string[] = [];
    const visit = (node: ts.Node) => {
      if (ts.isJsxText(node) && visible(node.text)) texts.push(node.text.trim());
      if (ts.isJsxAttribute(node) && node.initializer && ts.isStringLiteral(node.initializer)) {
        texts.push(node.initializer.text);
      }
      ts.forEachChild(node, visit);
    };
    visit(probe);
    expect(texts).toContain('Hello there');
    expect(texts).toContain('Close the panel');
    expect(visible('CPU')).toBe(false);
    expect(visible('—')).toBe(false);
    expect(visible('Kein Netzwerk')).toBe(true);
  });
});
