// Captures the README screenshots from the real PULSE frontend.
//
//   pnpm build && node scripts/showcase/screenshots.mjs
//
// Writes PNG masters to showcase-output/screenshots/ (ignored) and optimised
// WebP files to docs/assets/screenshots/ (tracked). Every state is reached
// through the real interface — the welcome sheet, the sidebar, the template
// gallery, the process table — exactly as a person would click through it.

import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import {
  ASSETS,
  BASE_URL,
  OUTPUT,
  ensureDir,
  nav,
  openPulse,
  settle,
  startServer,
} from './lib.mjs';

export const WIDTH = 1600;
export const HEIGHT = 900;

const raw = ensureDir(join(OUTPUT, 'screenshots'));
const tracked = ensureDir(join(ASSETS, 'screenshots'));

/** Scrolls the app's content area so `locator` sits `offset` px below the top. */
export async function scrollTo(page, locator, offset = 28) {
  await locator.first().evaluate((element, gap) => {
    const scroller = element.closest('.app-shell__content') ?? document.scrollingElement;
    const top =
      element.getBoundingClientRect().top -
      scroller.getBoundingClientRect().top +
      scroller.scrollTop;
    scroller.scrollTo({ top: Math.max(0, top - gap), behavior: 'instant' });
  }, offset);
}

/** First run through the real welcome sheet: the default Clean style, no mode. */
export async function finishWelcome(page) {
  await page.goto(BASE_URL + '/');
  await settle(page, 800);
  await page.getByRole('button', { name: 'Skip' }).click();
  await settle(page, 1500);
}

/** Creates a dashboard from one of the built-in templates via the real gallery. */
export async function dashboardFromTemplate(page, templateName) {
  await nav(page, 'Dashboard');
  await settle(page, 800);
  await page.getByRole('button', { name: 'New', exact: true }).click();
  await settle(page, 600);
  await page
    .getByRole('article', { name: `${templateName} template` })
    .getByRole('button')
    .click();
  await settle(page, 2500);
}

export async function pickStyle(page, styleName) {
  await nav(page, 'Appearance');
  await settle(page, 600);
  await page
    .getByRole('button', { name: new RegExp(`^${styleName}\\b`) })
    .first()
    .click();
  await settle(page, 1200);
}

function toWebp(name, quality = 86) {
  const source = join(raw, `${name}.png`);
  const target = join(tracked, `${name}.webp`);
  execFileSync('ffmpeg', [
    '-v',
    'error',
    '-y',
    '-i',
    source,
    '-c:v',
    'libwebp',
    '-lossless',
    '0',
    '-quality',
    String(quality),
    '-compression_level',
    '6',
    '-preset',
    'picture',
    target,
  ]);
  return target;
}

async function shoot(page, name, options = {}) {
  // Park the pointer where it highlights nothing.
  await page.mouse.move(2, (page.viewportSize()?.height ?? 900) - 2);
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(raw, `${name}.png`), ...options });
  console.log(`  ${name}: ${toWebp(name)}`);
}

async function main() {
  const stop = await startServer();
  const { browser, page } = await openPulse({ width: WIDTH, height: HEIGHT });
  try {
    // 1. Overview — the home page, Clean style.
    await finishWelcome(page);
    await settle(page, 2500);
    await shoot(page, 'overview');

    // 2. The template gallery, then the "Fancy showcase" dashboard it creates.
    await nav(page, 'Dashboard');
    await settle(page, 800);
    await page.getByRole('button', { name: 'New', exact: true }).click();
    await settle(page, 1200);
    await shoot(page, 'dashboard-templates');
    await page
      .getByRole('article', { name: 'Fancy showcase template' })
      .getByRole('button')
      .click();
    await settle(page, 4000);
    await shoot(page, 'dashboard');

    // 3. Processes: the process table with the inspector open on a busy rustc.
    await nav(page, 'Overview');
    await settle(page, 1200);
    const processCard = page.getByLabel('Process details');
    await scrollTo(page, processCard, 24);
    await processCard
      .getByRole('group', { name: 'Process view' })
      .getByRole('button', { name: 'Processes' })
      .click();
    await settle(page, 800);
    await processCard.getByRole('table', { name: 'Processes' }).getByText('rustc').first().click();
    await settle(page, 1800);
    await shoot(page, 'processes');
    await page.getByRole('button', { name: 'Close inspector' }).click();

    // 4. History: CPU history over the last 24 hours, per logical processor
    //    detail above it.
    const cpuHistory = page.getByRole('region', { name: 'CPU history history' });
    await scrollTo(page, page.getByLabel('CPU details'), 24);
    await settle(page, 1500);
    await cpuHistory.getByRole('button', { name: '24h' }).click();
    await settle(page, 2000);
    await shoot(page, 'history');

    // 5. Appearance studio with the Glass style selected.
    await pickStyle(page, 'Glass');
    await scrollTo(page, page.getByRole('heading', { name: 'Appearance', level: 1 }), 56);
    await settle(page, 1500);
    await shoot(page, 'appearance');

    // 6. Overlays: the pack gallery (twelve built-in packs).
    await pickStyle(page, 'Clean');
    await nav(page, 'Overlays');
    await settle(page, 1200);
    await scrollTo(page, page.getByRole('heading', { name: 'Overlay packs' }), 24);
    await settle(page, 1500);
    await shoot(page, 'overlays');

    // 7. Development mode page in its Technical style.
    await nav(page, 'Development');
    await settle(page, 1000);
    await scrollTo(page, page.getByRole('heading', { name: 'Development', level: 1 }), 64);
    await settle(page, 1500);
    await shoot(page, 'mode-development');
  } finally {
    await browser.close();
  }

  // 8. The Mini window itself: a separate real window kind (`?window=mini`),
  //    at its native 380 × 280 size, captured at 2× for sharpness.
  const mini = await openPulse({
    width: 380,
    height: 280,
    scale: 2,
    config: { appearance: { setupDone: true } },
  });
  try {
    await mini.page.goto(BASE_URL + '/?window=mini');
    await settle(mini.page, 3000);
    await shoot(mini.page, 'mini-window');
  } finally {
    await mini.browser.close();
    stop();
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  await main();
}
