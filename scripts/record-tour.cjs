// Records the short tour at the top of the README as a GIF, in light or dark colors, against a
// real kubri-server: the overview, `:deploy shop`, the pods of api, its logs as a table of
// fields, a shell, and `:ctx staging`. See docs/SCREENSHOTS.md for the cluster it expects.
//
//   node scripts/record-tour.cjs <playwright-module> <kubri-url> <light|dark>
//
// Needs ffmpeg. Writes docs/assets/kubri-tour-<scheme>.gif.
const { chromium } = require(process.argv[2] || 'playwright');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const base = (process.argv[3] || 'http://127.0.0.1:7878').replace(/\/$/, '');
const scheme = process.argv[4] || 'light';
const gif = path.resolve(__dirname, `../docs/assets/kubri-tour-${scheme}.gif`);
const work = fs.mkdtempSync(path.join(os.tmpdir(), 'kubri-tour-'));

(async () => {
  const browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1280, height: 760 }, colorScheme: scheme });
  await context.addInitScript(() => localStorage.setItem('kubri.tour', 'seen'));
  const page = await context.newPage();
  page.on('pageerror', (e) => {
    throw e;
  });
  await page.goto(base + '/');
  // Start on the first context, whatever the last run switched to.
  await page.evaluate(() =>
    fetch('api/context', { method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ name: 'lab' }) })
  );
  await page.goto(base + '/#/');
  await page.reload();
  await page.getByRole('heading', { name: 'Overview' }).waitFor();
  await page.getByText('Live · lab').waitFor();
  await page.waitForTimeout(3000);

  // The browser's own screencast, with each frame's time.
  const cdp = await context.newCDPSession(page);
  const frames = [];
  cdp.on('Page.screencastFrame', async (f) => {
    frames.push({ t: f.metadata.timestamp, data: f.data });
    await cdp.send('Page.screencastFrameAck', { sessionId: f.sessionId }).catch(() => {});
  });
  await cdp.send('Page.startScreencast', { format: 'png', everyNthFrame: 1 });
  const pause = (ms) => page.waitForTimeout(ms);
  const type = (text) => page.keyboard.type(text, { delay: 90 });

  await pause(2200);
  await page.keyboard.press(':');
  await pause(300);
  await type('deploy shop');
  await pause(600);
  await page.keyboard.press('Enter');
  await pause(1600);
  await page.keyboard.press('Enter'); // api, the first row
  await pause(1600);
  await page.keyboard.press('l');
  await pause(1800);
  await page.getByText('View', { exact: true }).click();
  await pause(700);
  await page.getByText('Table of fields', { exact: true }).click();
  await pause(250);
  await page.keyboard.press('Escape');
  await pause(3000);
  await page.keyboard.press('Escape');
  await pause(300);
  await page.keyboard.press('s');
  await pause(1600);
  await type('hostname && ps\n');
  await pause(1800);
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');
  await pause(500);
  await page.keyboard.press(':');
  await pause(300);
  await type('ctx staging');
  await pause(700);
  await page.keyboard.press('Enter');
  await page.getByText('Live · staging').waitFor({ timeout: 15000 });
  // End on the pods of staging, not on the list still loading.
  await page.getByText(/^[1-9]\d* of \d+ pods$/).waitFor({ timeout: 15000 });
  await pause(2200);
  await cdp.send('Page.stopScreencast');
  await browser.close();

  // Each frame shown for as long as the browser showed it.
  const name = (i) => path.join(work, `${String(i).padStart(5, '0')}.png`);
  const list = [];
  frames.forEach((f, i) => {
    fs.writeFileSync(name(i), Buffer.from(f.data, 'base64'));
    const next = frames[i + 1]?.t ?? f.t + 1.5;
    list.push(`file '${name(i)}'`, `duration ${Math.max(0.02, next - f.t).toFixed(3)}`);
  });
  list.push(`file '${name(frames.length - 1)}'`);
  fs.writeFileSync(path.join(work, 'list.txt'), list.join('\n') + '\n');
  const ffmpeg = (...args) => execFileSync('ffmpeg', ['-loglevel', 'error', '-y', ...args], { stdio: 'inherit' });
  const video = path.join(work, 'tour.mp4');
  ffmpeg('-f', 'concat', '-safe', '0', '-i', path.join(work, 'list.txt'), '-vsync', 'vfr', '-pix_fmt', 'yuv420p', '-c:v', 'libx264', '-crf', '18', video);
  ffmpeg(
    '-i', video,
    '-vf', 'fps=12,scale=1024:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle',
    gif
  );
  fs.rmSync(work, { recursive: true, force: true });
  console.log(`${path.relative(process.cwd(), gif)}: ${frames.length} frames, ${(fs.statSync(gif).size / 1e6).toFixed(1)} MB`);
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
