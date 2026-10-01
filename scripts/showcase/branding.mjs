// Renders the README hero and the GitHub social preview.
//
//   pnpm build && node scripts/showcase/branding.mjs
//
// 1. Captures a real, high-DPI view of PULSE on the fixture (the "Fancy
//    showcase" dashboard template in the default Clean style).
// 2. Composes it with the real logo (public/pulse.png) in two small HTML
//    layouts styled with PULSE's own Clean tokens (branding/frame.css), and
//    renders them with the same headless Chromium.
//
// Outputs:
//   docs/assets/branding/pulse-hero.webp            1600 × 600  (tracked)
//   docs/assets/branding/pulse-social-preview.png   1280 × 640  (tracked)
//   showcase-output/branding/*.png                  masters     (ignored)

import { execFileSync } from 'node:child_process';
import { readFileSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { chromium } from 'playwright-core';
import {
  ASSETS,
  HERE,
  OUTPUT,
  ROOT,
  ensureDir,
  nav,
  openPulse,
  settle,
  startServer,
} from './lib.mjs';
import { finishWelcome } from './screenshots.mjs';

const work = ensureDir(join(OUTPUT, 'branding'));
const tracked = ensureDir(join(ASSETS, 'branding'));

const LOGO = pathToFileURL(join(ROOT, 'public/pulse.png')).href;
const CSS = readFileSync(join(HERE, 'branding/frame.css'), 'utf8');
/** `APP_TAGLINE` in src/app/constants.ts. */
const TAGLINE = 'Your system, at a glance.';

/** A real capture: the Fancy showcase template, in the app's Clean style. */
async function captureSource() {
  const stop = await startServer();
  const { browser, page } = await openPulse({ width: 1440, height: 900, scale: 2 });
  try {
    await finishWelcome(page);
    await nav(page, 'Dashboard');
    await settle(page, 800);
    await page.getByRole('button', { name: 'New', exact: true }).click();
    await settle(page, 600);
    await page
      .getByRole('article', { name: 'Fancy showcase template' })
      .getByRole('button')
      .click();
    await settle(page, 1000);
    await page.getByLabel('Dashboard style').selectOption({ label: 'App style' });
    await settle(page, 4000);
    await page.mouse.move(2, 898);
    // Selecting the style can scroll the content area; show it from the top.
    await page.evaluate(() => document.querySelector('.app-shell__content')?.scrollTo(0, 0));
    await page.waitForTimeout(400);
    const path = join(work, 'source-dashboard.png');
    await page.screenshot({ path });
    return pathToFileURL(path).href;
  } finally {
    await browser.close();
    stop();
  }
}

/** The logo's pulse, stretched into a long trace: flat, one beat, flat. */
function trace(width, y, beatX, scale, opacity) {
  const p = (dx, dy) => `${beatX + dx * scale},${y + dy * scale}`;
  const d = [
    `M0,${y}`,
    `L${p(-4, 0)}`,
    `L${p(-1.5, -6)}`,
    `L${p(2.5, 6)}`,
    `L${p(5.5, -3)}`,
    `L${p(7.5, 0)}`,
    `L${width},${y}`,
  ].join(' ');
  return `
    <svg class="trace" viewBox="0 0 ${width} ${y * 2}" preserveAspectRatio="none">
      <defs>
        <linearGradient id="fade" x1="0" x2="1">
          <stop offset="0" stop-color="#38d6c4" stop-opacity="0"/>
          <stop offset="0.35" stop-color="#38d6c4" stop-opacity="${opacity}"/>
          <stop offset="0.7" stop-color="#8f9cff" stop-opacity="${opacity * 0.7}"/>
          <stop offset="1" stop-color="#8f9cff" stop-opacity="0"/>
        </linearGradient>
        <filter id="glow"><feGaussianBlur stdDeviation="6"/></filter>
      </defs>
      <path d="${d}" stroke="url(#fade)" stroke-width="${scale * 0.9}" filter="url(#glow)"/>
      <path d="${d}" stroke="url(#fade)" stroke-width="${scale * 0.32}"/>
    </svg>`;
}

function heroHtml(shot) {
  return `<!doctype html><html><head><meta charset="utf-8"><style>${CSS}
    .copy { position: absolute; left: 96px; top: 0; bottom: 0; display: flex;
            flex-direction: column; justify-content: center; gap: 26px; z-index: 2; }
    .brand { gap: 26px; }
    .brand img { width: 96px; height: 96px; }
    .wordmark { font-size: 84px; }
    .tagline { font-size: 30px; }
    .platforms { font-size: 19px; }
    .shot { left: 760px; top: 64px; width: 1040px; height: 650px;
            -webkit-mask-image: linear-gradient(90deg, rgba(0,0,0,0.25), #000 18%),
                                linear-gradient(180deg, #000 70%, rgba(0,0,0,0.35));
            -webkit-mask-composite: source-in; }
  </style></head><body><div class="canvas">
    ${trace(1600, 300, 640, 14, 0.22)}
    <div class="shot"><img src="${shot}"></div>
    <div class="copy">
      <div class="brand"><img src="${LOGO}"><span class="wordmark">PULSE</span></div>
      <p class="tagline">${TAGLINE}</p>
      <p class="platforms"><span class="dot"></span>Windows<span class="sep">·</span>Fedora Linux</p>
    </div>
  </div></body></html>`;
}

function socialHtml(shot) {
  return `<!doctype html><html><head><meta charset="utf-8"><style>${CSS}
    .copy { position: absolute; left: 80px; top: 0; bottom: 0; width: 640px; display: flex;
            flex-direction: column; justify-content: center; gap: 28px; z-index: 2; }
    .brand { gap: 26px; }
    .brand img { width: 112px; height: 112px; }
    .wordmark { font-size: 96px; }
    .tagline { font-size: 36px; line-height: 1.25; }
    .platforms { font-size: 26px; }
    .shot { left: 700px; top: 80px; width: 900px; height: 562px;
            -webkit-mask-image: linear-gradient(90deg, rgba(0,0,0,0.2), #000 22%); }
  </style></head><body><div class="canvas">
    ${trace(1280, 320, 650, 15, 0.2)}
    <div class="shot"><img src="${shot}"></div>
    <div class="copy">
      <div class="brand"><img src="${LOGO}"><span class="wordmark">PULSE</span></div>
      <p class="tagline">Cross-platform system<br>monitoring</p>
      <p class="platforms"><span class="dot"></span>Windows<span class="sep">·</span>Fedora Linux</p>
    </div>
  </div></body></html>`;
}

async function render(browser, html, width, height, path) {
  const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 1 });
  const file = path.replace(/\.png$/, '.html');
  writeFileSync(file, html);
  await page.goto(pathToFileURL(file).href, { waitUntil: 'load' });
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path });
  await page.close();
}

async function main() {
  const shot = await captureSource();
  const browser = await chromium.launch({
    executablePath: process.env.SHOWCASE_CHROMIUM || undefined,
    args: ['--allow-file-access-from-files'],
  });
  try {
    const heroPng = join(work, 'pulse-hero.png');
    await render(browser, heroHtml(shot), 1600, 600, heroPng);
    const hero = join(tracked, 'pulse-hero.webp');
    execFileSync('ffmpeg', [
      '-v',
      'error',
      '-y',
      '-i',
      heroPng,
      '-c:v',
      'libwebp',
      '-quality',
      '88',
      '-compression_level',
      '6',
      '-preset',
      'picture',
      hero,
    ]);

    const socialPng = join(work, 'pulse-social-preview.png');
    await render(browser, socialHtml(shot), 1280, 640, socialPng);
    const social = join(tracked, 'pulse-social-preview.png');
    // Palette-quantised PNG: GitHub requires < 1 MB for the social preview.
    execFileSync('convert', [
      socialPng,
      '-strip',
      '-colors',
      '256',
      '-define',
      'png:compression-level=9',
      social,
    ]);

    for (const file of [hero, social]) console.log(`  ${file}: ${statSync(file).size} bytes`);
  } finally {
    await browser.close();
  }
}

await main();
