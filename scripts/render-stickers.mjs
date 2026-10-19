// Renders the SVG sources of every starter sticker pack to 320x320 PNG (D-289).
//
//   node scripts/render-stickers.mjs           write packs/<pack>/stickers/<name>.png from packs/<pack>/src/<name>.svg
//   node scripts/render-stickers.mjs --check   render in memory and fail if a committed PNG differs
//
// It opens each SVG in the Playwright Chromium that the e2e package already installs (no new download), inside a
// small HTML wrapper that loads the OFL fonts of apps/web/public/fonts, and screenshots it at 320x320 device
// pixels with a transparent background (the rounded corners). Every PNG must be 320x320 and under 512 KiB.
// --check compares pixels with a small tolerance, because anti-aliasing differs a little between operating systems
// (the committed files come from a developer machine, CI renders on Linux); a changed SVG is far outside it.
import {
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
  mkdirSync,
  existsSync,
} from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const packsDir = join(root, 'packages/starter-packs');
const fontsDir = join(root, 'apps/web/public/fonts');
const SIZE = 320;
const MAX_BYTES = 512 * 1024;
const check = process.argv.includes('--check');

const require = createRequire(join(root, 'e2e/package.json'));
const { chromium } = require('@playwright/test');

function pngSize(buf) {
  if (buf.length < 24 || buf.toString('latin1', 1, 4) !== 'PNG') throw new Error('not a PNG');
  return [buf.readUInt32BE(16), buf.readUInt32BE(20)];
}

const jobs = [];
for (const pack of readdirSync(packsDir, { withFileTypes: true })) {
  const src = join(packsDir, pack.name, 'src');
  if (!pack.isDirectory() || !existsSync(src)) continue;
  for (const f of readdirSync(src)
    .filter((n) => n.endsWith('.svg'))
    .sort()) {
    jobs.push({
      svg: join(src, f),
      png: join(packsDir, pack.name, 'stickers', f.replace(/\.svg$/, '.png')),
    });
  }
}
if (jobs.length === 0) throw new Error(`no SVG sources under ${packsDir}`);

const tmp = mkdtempSync(join(tmpdir(), 'sr-stickers-'));
const font = (family, file, style) =>
  `@font-face{font-family:'${family}';src:url('${pathToFileURL(join(fontsDir, file)).href}') format('woff2');font-weight:200 800;font-style:${style}}`;
const wrapper = (svg) => `<!doctype html><meta charset="utf-8"><style>
${font('Newsreader', 'newsreader-variable.woff2', 'normal')}
${font('Newsreader', 'newsreader-italic-variable.woff2', 'italic')}
html,body{margin:0;background:transparent}svg{display:block}</style>${svg}`;

const browser = await chromium.launch();
const failures = [];
try {
  const page = await browser.newPage({
    viewport: { width: SIZE, height: SIZE },
    deviceScaleFactor: 1,
  });
  for (const job of jobs) {
    const html = join(tmp, 'sticker.html');
    writeFileSync(html, wrapper(readFileSync(job.svg, 'utf8')));
    await page.goto(pathToFileURL(html).href);
    await page.evaluate(async () => {
      await document.fonts.load('italic 500 100px Newsreader');
      await document.fonts.ready;
    });
    const png = await page.screenshot({
      omitBackground: true,
      clip: { x: 0, y: 0, width: SIZE, height: SIZE },
    });
    const name = job.png.slice(packsDir.length + 1);
    const [w, h] = pngSize(png);
    if (w !== SIZE || h !== SIZE) failures.push(`${name}: ${w}x${h}, expected ${SIZE}x${SIZE}`);
    if (png.length >= MAX_BYTES) failures.push(`${name}: ${png.length} bytes, limit ${MAX_BYTES}`);
    if (!check) {
      mkdirSync(dirname(job.png), { recursive: true });
      writeFileSync(job.png, png);
      continue;
    }
    if (!existsSync(job.png)) {
      failures.push(`${name}: not committed (run \`just stickers\`)`);
      continue;
    }
    // Compare in the browser: decode both PNGs and measure how many pixels differ noticeably.
    const diff = await page.evaluate(
      async ([a, b]) => {
        const load = async (b64) => {
          const img = new Image();
          img.src = `data:image/png;base64,${b64}`;
          await img.decode();
          const c = new OffscreenCanvas(img.width, img.height);
          const ctx = c.getContext('2d');
          ctx.drawImage(img, 0, 0);
          return ctx.getImageData(0, 0, img.width, img.height).data;
        };
        const [x, y] = [await load(a), await load(b)];
        if (x.length !== y.length) return 1;
        let bad = 0;
        for (let i = 0; i < x.length; i += 4) {
          const d = Math.max(
            Math.abs(x[i] - y[i]),
            Math.abs(x[i + 1] - y[i + 1]),
            Math.abs(x[i + 2] - y[i + 2]),
            Math.abs(x[i + 3] - y[i + 3]),
          );
          if (d > 48) bad += 1;
        }
        return bad / (x.length / 4);
      },
      [png.toString('base64'), readFileSync(job.png).toString('base64')],
    );
    if (diff > 0.02)
      failures.push(
        `${name}: differs from its SVG source (${(diff * 100).toFixed(1)}% of pixels); run \`just stickers\``,
      );
    const committed = readFileSync(job.png);
    const [cw, ch] = pngSize(committed);
    if (cw !== SIZE || ch !== SIZE || committed.length >= MAX_BYTES)
      failures.push(`${name}: committed file is out of bounds`);
  }
} finally {
  await browser.close();
  rmSync(tmp, { recursive: true, force: true });
}
if (failures.length) {
  console.error(failures.join('\n'));
  process.exit(1);
}
console.log(`${check ? 'checked' : 'rendered'} ${jobs.length} stickers`);
