// Shared capture helpers for the PULSE showcase scripts.
//
// Serves the production build of the real frontend (`pnpm build` → dist/),
// opens it in headless Chromium and injects the showcase fixture backend
// before any application code runs. See README.md.

import { spawn } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';

export const HERE = dirname(fileURLToPath(import.meta.url));
export const ROOT = resolve(HERE, '../..');
export const OUTPUT = join(ROOT, 'showcase-output');
export const ASSETS = join(ROOT, 'docs/assets');

/** The fixed wall-clock moment every capture starts at (UTC). */
export const START_TIME = Date.parse('2026-09-15T14:32:00Z');

const PORT = Number(process.env.SHOWCASE_PORT ?? 4319);
export const BASE_URL = `http://127.0.0.1:${PORT}`;

export function ensureDir(path) {
  mkdirSync(path, { recursive: true });
  return path;
}

/** Starts `vite preview` on the existing dist/ build. */
export async function startServer() {
  const child = spawn(
    'pnpm',
    ['exec', 'vite', 'preview', '--host', '127.0.0.1', '--port', String(PORT), '--strictPort'],
    { cwd: ROOT, stdio: 'ignore', detached: true },
  );
  // Own process group, so stopping it also stops vite under pnpm.
  const stop = () => {
    try {
      process.kill(-child.pid, 'SIGTERM');
    } catch {
      /* already gone */
    }
  };
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(BASE_URL);
      if (response.ok) return stop;
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  stop();
  throw new Error('vite preview did not start — run `pnpm build` first');
}

function fixtureScript(config) {
  const generated = JSON.parse(readFileSync(join(HERE, 'fixture/catalog.json'), 'utf8'));
  const backend = readFileSync(join(HERE, 'fixture/backend.js'), 'utf8');
  const data = { catalog: generated.catalog, sourceRefs: generated.sourceRefs, config };
  return `window.__PULSE_SHOWCASE__ = ${JSON.stringify(data)};\n${backend}`;
}

/**
 * Opens a browser page running the real PULSE frontend on the fixture.
 *
 * `scale` is the device pixel ratio; the viewport is in CSS pixels.
 */
export async function openPulse({
  width = 1440,
  height = 900,
  scale = 1,
  config = {},
  video = null,
} = {}) {
  const browser = await chromium.launch({
    executablePath: process.env.SHOWCASE_CHROMIUM || undefined,
  });
  const context = await browser.newContext({
    viewport: { width, height },
    deviceScaleFactor: scale,
    locale: 'en-US',
    timezoneId: 'UTC',
    colorScheme: 'dark',
    reducedMotion: 'no-preference',
    ...(video ? { recordVideo: { dir: video, size: { width, height } } } : {}),
  });
  await context.clock.install({ time: START_TIME });
  await context.addInitScript({ content: fixtureScript(config) });
  const page = await context.newPage();
  page.on('console', (message) => {
    if (message.type() === 'error' || message.text().startsWith('[showcase]')) {
      console.log(`  page: ${message.text()}`);
    }
  });
  page.on('pageerror', (error) => console.log(`  page error: ${error.message}`));
  await page.clock.resume();
  return { browser, context, page };
}

/** Lets live data, history fetches and entry animations settle. */
export async function settle(page, ms = 1500) {
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(ms);
}

/** Navigates inside the SPA through the real sidebar. */
export async function nav(page, label) {
  await page
    .getByRole('navigation', { name: 'Main' })
    .getByRole('link', { name: label, exact: true })
    .click();
}
