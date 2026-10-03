import { expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

it('keeps idle status and editing indicators free of perpetual repaint animations', () => {
  // Fedora/nouveau WebKit software rasterization can saturate a core even for
  // a small pulsing shadow. Live metrics remain dynamic; decoration must settle.
  const css = readFileSync('src/styles/design.css', 'utf8');
  expect(css).not.toMatch(/\banimation(?:-iteration-count)?\s*:[^;{}]*\binfinite\b/);
});
