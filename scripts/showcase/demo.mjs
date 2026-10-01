// Records the product demo from the real PULSE frontend.
//
//   pnpm build && node scripts/showcase/demo.mjs
//
// The app runs on the fixture in headless Chromium at 1920 × 1080 and is
// driven through its real interface. Chrome's screencast delivers frames in
// real time; only the marked scene windows are kept. Captions and the intro /
// outro cards are separate layers composed by ffmpeg — nothing is drawn into
// the application page.
//
// Outputs:
//   showcase-output/Pulse-demo.mp4      1920 × 1080 H.264, ~30 s   (ignored)
//   docs/assets/demo/pulse-demo.webp    960 × 540 animated WebP     (tracked)

import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
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
import { finishWelcome, scrollTo } from './screenshots.mjs';

/** Output size. The app is laid out at 1600 × 900 CSS px, rendered at 1.2×. */
const W = 1920;
const H = 1080;
const CSS_W = 1600;
const CSS_H = 900;
const FPS = 30;
const work = join(OUTPUT, 'demo');
const frameDir = join(work, 'frames');

const ffmpeg = (...args) => execFileSync('ffmpeg', ['-v', 'error', '-y', ...args]);

// --- recording ------------------------------------------------------------------

async function startScreencast(context, page) {
  rmSync(frameDir, { recursive: true, force: true });
  mkdirSync(frameDir, { recursive: true });
  const client = await context.newCDPSession(page);
  const frames = [];
  client.on('Page.screencastFrame', async ({ data, metadata, sessionId }) => {
    const file = join(frameDir, `${String(frames.length).padStart(6, '0')}.jpg`);
    writeFileSync(file, Buffer.from(data, 'base64'));
    frames.push({ file, t: metadata.timestamp });
    await client.send('Page.screencastFrameAck', { sessionId }).catch(() => undefined);
  });
  await client.send('Page.startScreencast', {
    format: 'jpeg',
    quality: 95,
    maxWidth: W,
    maxHeight: H,
    everyNthFrame: 1,
  });
  return {
    frames,
    now: () => Date.now() / 1000,
    stop: () => client.send('Page.stopScreencast'),
  };
}

/** Writes one scene as a constant-frame-rate clip from the frames in [from, to]. */
function encodeScene(frames, scene, index) {
  const inside = frames.filter((f) => f.t >= scene.from && f.t <= scene.to);
  const before = frames.filter((f) => f.t < scene.from).at(-1);
  const list = before ? [{ ...before, t: scene.from }, ...inside] : inside;
  if (list.length === 0) throw new Error(`scene ${scene.name}: no frames`);
  let concat = 'ffconcat version 1.0\n';
  list.forEach((frame, i) => {
    const next = list[i + 1]?.t ?? scene.to;
    concat += `file '${frame.file}'\nduration ${Math.max(0.001, next - frame.t).toFixed(4)}\n`;
  });
  concat += `file '${list.at(-1).file}'\n`;
  console.log(
    `  scene ${scene.name}: ${(scene.to - scene.from).toFixed(1)} s, ` +
      `${(inside.length / (scene.to - scene.from)).toFixed(1)} captured frames/s`,
  );
  const listFile = join(work, `scene-${index}.txt`);
  writeFileSync(listFile, concat);

  const out = join(work, `scene-${index}.mp4`);
  const duration = scene.to - scene.from;
  const caption = join(work, `caption-${index}.png`);
  ffmpeg(
    '-f',
    'concat',
    '-safe',
    '0',
    '-i',
    listFile,
    '-loop',
    '1',
    '-t',
    duration.toFixed(3),
    '-i',
    caption,
    '-filter_complex',
    `[0:v]fps=${FPS},scale=${W}:${H},setsar=1,format=yuv420p[base];` +
      `[1:v]format=rgba,fade=t=in:st=0.25:d=0.45:alpha=1,` +
      `fade=t=out:st=${(duration - 0.7).toFixed(3)}:d=0.45:alpha=1[cap];` +
      `[base][cap]overlay=0:0:shortest=1,format=yuv420p`,
    '-t',
    duration.toFixed(3),
    '-r',
    String(FPS),
    '-c:v',
    'libx264',
    '-preset',
    'slow',
    '-crf',
    '12',
    out,
  );
  return { file: out, duration };
}

// --- caption and title layers -----------------------------------------------------

const CSS = readFileSync(join(HERE, 'branding/frame.css'), 'utf8');
const LOGO = pathToFileURL(join(ROOT, 'public/pulse.png')).href;

const captionHtml = (text) => `<!doctype html><html><head><meta charset="utf-8"><style>${CSS}
  html, body { background: transparent; }
  /* Centred over the app's content column (the sidebar is 232 CSS px × 1.2). */
  .row { position: absolute; left: 278px; right: 0; bottom: 60px; display: flex;
         justify-content: center; }
  .cap { display: inline-flex; align-items: center;
         gap: 14px; padding: 16px 26px 16px 20px; border-radius: 999px;
         background: rgba(11, 14, 18, 0.82); border: 1px solid rgba(255, 255, 255, 0.1);
         box-shadow: 0 16px 40px rgba(0, 0, 0, 0.45); font-size: 30px; font-weight: 600;
         letter-spacing: -0.005em; color: #e9eef4; }
  .cap i { width: 10px; height: 10px; border-radius: 50%; background: #38d6c4;
           box-shadow: 0 0 12px rgba(56, 214, 196, 0.8); }
</style></head><body><div class="row"><div class="cap"><i></i>${text}</div></div></body></html>`;

const cardHtml = (big) => `<!doctype html><html><head><meta charset="utf-8"><style>${CSS}
  .center { position: absolute; inset: 0; display: flex; flex-direction: column;
            align-items: center; justify-content: center; gap: 30px; }
  .brand { gap: 30px; }
  .brand img { width: ${big ? 132 : 112}px; height: ${big ? 132 : 112}px; }
  .wordmark { font-size: ${big ? 120 : 104}px; }
  .tagline { font-size: 40px; }
  .platforms { font-size: 26px; }
</style></head><body><div class="canvas"><div class="center">
  <div class="brand"><img src="${LOGO}"><span class="wordmark">PULSE</span></div>
  <p class="tagline">Your system, at a glance.</p>
  <p class="platforms"><span class="dot"></span>Windows<span class="sep">·</span>Fedora Linux</p>
</div></div></body></html>`;

async function renderLayer(browser, html, file, transparent) {
  const page = await browser.newPage({ viewport: { width: W, height: H } });
  const htmlFile = file.replace(/\.png$/, '.html');
  writeFileSync(htmlFile, html);
  await page.goto(pathToFileURL(htmlFile).href, { waitUntil: 'load' });
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: file, omitBackground: transparent });
  await page.close();
}

function stillClip(image, seconds, index) {
  const out = join(work, `still-${index}.mp4`);
  ffmpeg(
    '-loop',
    '1',
    '-t',
    String(seconds),
    '-i',
    image,
    '-vf',
    `fps=${FPS},scale=${W}:${H},setsar=1,format=yuv420p`,
    '-c:v',
    'libx264',
    '-preset',
    'slow',
    '-crf',
    '12',
    out,
  );
  return { file: out, duration: seconds };
}

// --- the narrative ------------------------------------------------------------------

const CAPTIONS = [
  'Live metrics, one glance',
  'Dashboards from eight templates',
  'Eight styles — app-wide or per dashboard',
  'Local history, from 15 minutes to 7 days',
  'Processes, with an inspector',
  'Twelve overlay packs',
];

async function record() {
  const stop = await startServer();
  const { browser, context, page } = await openPulse({
    width: CSS_W,
    height: CSS_H,
    scale: W / CSS_W,
  });
  const scenes = [];
  try {
    await finishWelcome(page);
    await settle(page, 2500);
    const cast = await startScreencast(context, page);
    const scene = async (name, body) => {
      const from = cast.now();
      await body();
      scenes.push({ name, from, to: cast.now() });
    };
    await page.mouse.move(2, CSS_H - 2);

    // 1. Overview: live strip ticking.
    await scene('overview', async () => {
      await page.waitForTimeout(2800);
    });

    // 2. Template gallery → the Fancy showcase dashboard (Glass).
    await nav(page, 'Dashboard');
    await settle(page, 800);
    await scene('dashboard', async () => {
      await page.getByRole('button', { name: 'New', exact: true }).click();
      await page.waitForTimeout(1600);
      await page
        .getByRole('article', { name: 'Fancy showcase template' })
        .getByRole('button')
        .click();
      await page.mouse.move(2, CSS_H - 2);
      await page.waitForTimeout(3200);
    });

    // 3. Styles: the same dashboard re-dressed by the dashboard style picker.
    await scene('styles', async () => {
      const picker = page.getByLabel('Dashboard style');
      for (const label of ['Technical', 'Neon', 'App style']) {
        await picker.selectOption({ label });
        await page.mouse.move(2, CSS_H - 2);
        await page.waitForTimeout(1500);
      }
      await page.waitForTimeout(500);
    });

    // 4. History: CPU history, 24 h then 7 d.
    await nav(page, 'Overview');
    await settle(page, 1000);
    const cpuHistory = page.getByRole('region', { name: 'CPU history history' });
    await scrollTo(page, page.getByLabel('CPU details'), 24);
    await settle(page, 1500);
    await scene('history', async () => {
      await page.waitForTimeout(900);
      await cpuHistory.getByRole('button', { name: '24h' }).click();
      await page.waitForTimeout(1500);
      await cpuHistory.getByRole('button', { name: '7d' }).click();
      await page.waitForTimeout(1700);
    });

    // 5. Processes: the table, then the inspector on a busy rustc.
    const processCard = page.getByLabel('Process details');
    await scrollTo(page, processCard, 24);
    await processCard
      .getByRole('group', { name: 'Process view' })
      .getByRole('button', { name: 'Processes' })
      .click();
    await settle(page, 800);
    await page.mouse.move(2, CSS_H - 2);
    await scene('processes', async () => {
      await page.waitForTimeout(1000);
      await processCard
        .getByRole('table', { name: 'Processes' })
        .getByText('rustc')
        .first()
        .click();
      await page.mouse.move(2, CSS_H - 2);
      await page.waitForTimeout(2800);
    });
    await page.getByRole('button', { name: 'Close inspector' }).click();

    // 6. Overlays: the pack gallery, scrolled gently.
    await nav(page, 'Overlays');
    await settle(page, 1200);
    await scrollTo(page, page.getByRole('heading', { name: 'Overlay packs' }), 24);
    await settle(page, 1200);
    await scene('overlays', async () => {
      await page.waitForTimeout(700);
      await page.evaluate(() =>
        document.querySelector('.app-shell__content')?.scrollBy({ top: 360, behavior: 'smooth' }),
      );
      await page.waitForTimeout(2600);
    });

    // 7. Back to the dashboard to close.
    await nav(page, 'Dashboard');
    await settle(page, 1500);
    await page.mouse.move(2, CSS_H - 2);
    await scene('finale', async () => {
      await page.waitForTimeout(2600);
    });

    await cast.stop();
    await page.waitForTimeout(300);
    return { frames: cast.frames, scenes };
  } finally {
    await browser.close();
    stop();
  }
}

async function main() {
  ensureDir(work);
  const { frames, scenes } = await record();
  console.log(`  ${frames.length} frames captured`);

  const browser = await chromium.launch({
    executablePath: process.env.SHOWCASE_CHROMIUM || undefined,
  });
  try {
    for (const [i, text] of CAPTIONS.entries()) {
      await renderLayer(browser, captionHtml(text), join(work, `caption-${i}.png`), true);
    }
    // The finale carries no caption: a fully transparent layer.
    await renderLayer(
      browser,
      '<html><body></body></html>',
      join(work, `caption-${CAPTIONS.length}.png`),
      true,
    );
    await renderLayer(browser, cardHtml(true), join(work, 'intro.png'), false);
    await renderLayer(browser, cardHtml(false), join(work, 'outro.png'), false);
  } finally {
    await browser.close();
  }

  const clips = [stillClip(join(work, 'intro.png'), 2.6, 0)];
  scenes.forEach((scene, i) => clips.push(encodeScene(frames, scene, i)));
  clips.push(stillClip(join(work, 'outro.png'), 3.2, 1));

  // Join with short crossfades.
  const X = 0.5;
  const inputs = clips.flatMap((clip) => ['-i', clip.file]);
  let filter = '';
  let offset = 0;
  let previous = '[0:v]';
  for (let i = 1; i < clips.length; i += 1) {
    offset += clips[i - 1].duration - X;
    const label = i === clips.length - 1 ? '[joined]' : `[x${i}]`;
    filter += `${previous}[${i}:v]xfade=transition=fade:duration=${X}:offset=${offset.toFixed(3)}${label};`;
    previous = label;
  }
  filter +=
    '[joined]fade=t=out:st=' + (offset + clips.at(-1).duration - 0.6).toFixed(3) + ':d=0.6[v]';

  const mp4 = join(OUTPUT, 'Pulse-demo.mp4');
  ffmpeg(
    ...inputs,
    '-filter_complex',
    filter,
    '-map',
    '[v]',
    '-r',
    String(FPS),
    '-c:v',
    'libx264',
    '-profile:v',
    'high',
    '-level',
    '4.1',
    '-preset',
    'slow',
    '-crf',
    '18',
    '-pix_fmt',
    'yuv420p',
    '-movflags',
    '+faststart',
    '-an',
    mp4,
  );
  console.log(`  ${mp4}: ${statSync(mp4).size} bytes`);

  // README animation: the app scenes only (no title cards), smaller and at
  // a lower frame rate.
  const total = offset + clips.at(-1).duration;
  const from = clips[0].duration - X / 2;
  const to = total - clips.at(-1).duration + X / 2;
  const webp = join(ensureDir(join(ASSETS, 'demo')), 'pulse-demo.webp');
  ffmpeg(
    '-ss',
    from.toFixed(3),
    '-to',
    to.toFixed(3),
    '-i',
    mp4,
    '-vf',
    'fps=12,scale=1024:576:flags=lanczos',
    '-c:v',
    'libwebp_anim',
    '-lossless',
    '0',
    '-quality',
    '64',
    '-compression_level',
    '6',
    '-preset',
    'picture',
    '-loop',
    '0',
    '-an',
    webp,
  );
  console.log(`  ${webp}: ${statSync(webp).size} bytes`);
}

await main();
