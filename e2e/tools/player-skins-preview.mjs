// Dev-only preview of the backdrop skins of the floating player (MUS-SKN-001). Not a shipped route and not part
// of the test run. Same mocks as player-preview.mjs, on its own port so it never meets the shared 5173 server.
//
//   1. start the web dev server:  cd apps/web && VITE_SR_E2E=1 node_modules/.bin/vite --host 127.0.0.1 --port 5178
//   2. node e2e/tools/player-skins-preview.mjs [outDir]
//
// For each backdrop skin, playing a generated tone with a queue of three tracks: a shot clipped around the player
// and a full viewport at 1280x800, and both again at 360x740 (device scale factor 2). It also runs axe (with
// colour contrast) on the dock of each skin at both sizes and prints what it finds.
import { mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';

const require = createRequire(new URL('../package.json', import.meta.url));
const { chromium } = require('@playwright/test');

const base = process.env.SR_E2E_BASE_URL ?? 'http://127.0.0.1:5178';
const out = process.argv[2] ?? 'C:/Users/braxt/Desktop/SIDEROOM/screenshots/player-skins';
mkdirSync(out, { recursive: true });

const TITLE = 'Midnight Drive (Extended Mix)';
const ARTIST = 'Neon Coast';

function wav(seconds) {
  const rate = 8000;
  const n = rate * seconds;
  const b = Buffer.alloc(44 + n);
  b.write('RIFF', 0);
  b.writeUInt32LE(36 + n, 4);
  b.write('WAVEfmt ', 8);
  b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20);
  b.writeUInt16LE(1, 22);
  b.writeUInt32LE(rate, 24);
  b.writeUInt32LE(rate, 28);
  b.writeUInt16LE(1, 32);
  b.writeUInt16LE(8, 34);
  b.write('data', 36);
  b.writeUInt32LE(n, 40);
  for (let i = 0; i < n; i++)
    b[44 + i] = 128 + Math.round(60 * Math.sin((2 * Math.PI * 440 * i) / rate));
  return b;
}
const TONE = wav(60);

const browser = await chromium.launch();
const ctx = await browser.newContext({
  viewport: { width: 1280, height: 800 },
  deviceScaleFactor: 2,
  colorScheme: 'dark',
  bypassCSP: true,
});
const page = await ctx.newPage();
await page.route('**/e2e-tone-*.wav', (route) =>
  route.fulfill({
    status: 200,
    body: TONE,
    headers: { 'content-type': 'audio/wav', 'accept-ranges': 'bytes' },
  }),
);
const SAMPLE = [
  ['Neon Coast', 'Night Shift', ['Midnight Drive (Extended Mix)', 'Harbour Lights', 'Tail Lights']],
  ['The Lanterns', 'Paper Moons', ['Static Bloom', 'Overpass', 'Last Train Home']],
  ['Ada Vale', 'Low Tide', ['Salt and Glass', 'Undertow']],
].flatMap(([artist, album, titles], a) =>
  titles.map((title, i) => ({
    id: `${a + 1}0${i + 1}`,
    mount_id: '77',
    key: `Music/${artist}/${album}/${i + 1} ${title}.mp3`,
    size: 4_000_000,
    container: 'mp3',
    web_native: true,
    title,
    artist,
    album,
    album_artist: artist,
    track_no: i + 1,
    disc_no: 1,
    year: 2024 - a,
    duration_ms: 180_000 + i * 23_000,
    genres: [a === 2 ? 'Folk' : 'Synthwave'],
    tags: [],
    tags_state: 'read',
    can_edit: true,
  })),
);
const PLAYLISTS = [
  ['p1', 'Late drive', 6],
  ['p2', 'Focus', 4],
  ['p3', 'Sunday morning', 3],
].map(([id, name, n]) => ({
  id,
  owner_type: 'user',
  owner_id: '1',
  name,
  description: '',
  item_count: n,
  created_ms: 1_761_000_000_000,
  updated_ms: 1_761_700_000_000,
  can_edit: true,
}));
const json = (route, body) =>
  route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
await page.route('**/api/v1/**', (route) => {
  const url = new URL(route.request().url());
  const path = url.pathname.replace(/^.*\/api\/v1/, '');
  if (path === '/music/library')
    return url.searchParams.get('pending') === 'true'
      ? json(route, { items: [], total: 0 })
      : json(route, { items: SAMPLE, total: SAMPLE.length });
  if (path === '/users/@me/music/playlists') return json(route, PLAYLISTS);
  if (path.startsWith('/music/playlists/'))
    return json(route, {
      playlist: PLAYLISTS[0],
      items: SAMPLE.slice(0, 6).map((track, i) => ({ item_id: `i${i}`, position: i, track })),
      hidden: 0,
    });
  if (path === '/users/@me/music/favorites')
    return json(route, { items: [{ track: SAMPLE[1], at_ms: 1_761_700_000_000 }], hidden: 0 });
  if (path.startsWith('/users/@me/music/recent'))
    return json(route, {
      items: SAMPLE.slice(0, 3).map((track, i) => ({ track, at_ms: 1_761_700_000_000 - i })),
      hidden: 0,
    });
  if (path === '/users/@me') return json(route, { id: '1', username: 'preview' });
  return json(route, {});
});
// Since D-385 the dev server serves the app document only to a session it checks with sr-server. This throwaway
// preview runs without sr-server, so it fetches the app shell the way a module request would (not a navigation,
// which the gate never touches) and hands it to the navigation. The page's API calls are mocked above.
await page.route(`${base}/preview-host`, async (route) => {
  const shell = await route.fetch({
    url: `${base}/app.html`,
    headers: { accept: '*/*', 'sec-fetch-mode': 'cors' },
  });
  await route.fulfill({ response: shell, headers: { 'content-type': 'text/html' } });
});
await page.goto(`${base}/preview-host`);
await page.mouse.click(2, 2);
await page.waitForFunction(() => window.__srPlayerActions);
await page.evaluate(
  ([title, artist]) =>
    window.__srPlayerActions.play(
      [1, 2, 3].map((n) => ({
        id: `p${n}`,
        title: n === 1 ? title : `Neon City Nights ${n}`,
        artist: n === 1 ? artist : 'Neon Coast',
        src: `/e2e-tone-${n}.wav`,
        durationSec: 60,
        ...(n === 1 ? { format: 'mp3' } : {}),
      })),
    ),
  [TITLE, ARTIST],
);
await page.waitForFunction(() => window.__srPlayer?.audio.currentTime > 2);

const axeSource = createRequire(new URL('../package.json', import.meta.url)).resolve(
  'axe-core/axe.min.js',
);
const SKINS = ['valve', 'tidepool', 'circuit', 'study', 'stack'];
let problems = 0;

const audit = async (skin, size) => {
  if (!(await page.evaluate(() => 'axe' in window))) await page.addScriptTag({ path: axeSource });
  const found = await page.evaluate(async () => {
    const r = await window.axe.run('.sr-ply-dock', {
      runOnly: {
        type: 'tag',
        values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'],
      },
    });
    return r.violations.map((v) => ({
      id: v.id,
      impact: v.impact,
      nodes: v.nodes.map((n) => `${n.target.join(' ')}: ${n.failureSummary}`),
    }));
  });
  const blocking = found.filter((v) => v.impact === 'serious' || v.impact === 'critical');
  problems += blocking.length;
  console.log(`axe ${skin} ${size}: ${found.length ? JSON.stringify(found, null, 1) : 'clean'}`);
};

const shoot = async (skin, size) => {
  await page.evaluate((s) => window.__srPlayerActions.setSkin(s), skin);
  await page.waitForSelector(`.sr-ply[data-skin="${skin}"]`);
  await page.waitForTimeout(900);
  const box = await page.locator('.sr-ply').boundingBox();
  const vp = page.viewportSize();
  const pad = 24;
  const clip = {
    x: Math.max(0, box.x - pad),
    y: Math.max(0, box.y - pad),
    width: Math.min(vp.width, box.x + box.width + pad) - Math.max(0, box.x - pad),
    height: Math.min(vp.height, box.y + box.height + pad) - Math.max(0, box.y - pad),
  };
  const crop = join(out, `${skin}-${size}-player.png`);
  await page.screenshot({ path: crop, clip });
  const full = join(out, `${skin}-${size}-fullviewport.png`);
  await page.screenshot({ path: full });
  console.log('saved', crop);
  console.log('saved', full);
  const fit = await page.evaluate(() => {
    const el = document.querySelector('.sr-ply');
    const r = el.getBoundingClientRect();
    return {
      inside:
        r.left >= 0 &&
        r.top >= 0 &&
        r.right <= document.documentElement.clientWidth &&
        r.bottom <= document.documentElement.clientHeight,
      sideways: el.scrollWidth - el.clientWidth,
      w: Math.round(r.width),
      h: Math.round(r.height),
    };
  });
  console.log(`fit ${skin} ${size}: ${JSON.stringify(fit)}`);
  if (!fit.inside || fit.sideways > 0) problems++;
  await audit(skin, size);
};

for (const skin of SKINS) await shoot(skin, '1280x800');
await page.setViewportSize({ width: 360, height: 740 });
for (const skin of SKINS) await shoot(skin, '360x740');
await browser.close();
console.log(problems ? `${problems} problem(s)` : 'all skins fit and pass axe');
process.exitCode = problems ? 1 : 0;
