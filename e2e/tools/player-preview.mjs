// Dev-only preview of the floating player (MUS-PLY-001). Not a shipped route and not part of the test run.
//
//   1. start the web dev server:  cd apps/web && VITE_SR_E2E=1 node_modules/.bin/vite --host 127.0.0.1 --port 5173
//   2. node e2e/tools/player-preview.mjs [outDir]
//
// It opens a path the router treats as a signed-in app route (the not-found page, so no data is needed) with a
// mocked API session and the Lounge Dark colour scheme, plays a track with a long title, and saves one PNG per skin
// (device scale factor 2, clipped to the player) plus a full-viewport shot of the Retro skin docked at 1280x800.
// The music endpoints answer with a small sample library (wave-30 integration), so the Full Library Manager shows
// music-lib's panel connected: its search, playlists (a list and one opened) and equalizer sections.
import { mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';

const require = createRequire(new URL('../package.json', import.meta.url));
const { chromium } = require('@playwright/test');

const base = process.env.SR_E2E_BASE_URL ?? 'http://127.0.0.1:5173';
const out = process.argv[2] ?? 'C:\\Users\\braxt\\Desktop\\SIDEROOM\\screenshots\\p30-player\\v3';
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
      })),
    ),
  [TITLE, ARTIST],
);
await page.waitForFunction(() => window.__srPlayer?.audio.currentTime > 2);

const shot = async (skin, file, full = false) => {
  await page.evaluate((s) => window.__srPlayerActions.setSkin(s), skin);
  await page.waitForSelector(`.sr-ply[data-skin="${skin}"]`);
  await page.waitForTimeout(400);
  if (full) await page.screenshot({ path: join(out, file) });
  else await page.locator('.sr-ply').screenshot({ path: join(out, file) });
  console.log('saved', join(out, file));
};

await shot('retro', 'player-retro.png');
await shot('retro', 'player-retro-fullviewport-1280x800.png', true);
await shot('pill', 'player-pill.png');
await shot('library', 'player-library.png');
// The Full Library Manager with music-lib's panel in its Library tab.
const panel = async (tab, file, after) => {
  await page.evaluate(() => window.__srPlayerActions.setSkin('library'));
  await page.waitForSelector('.sr-mlib');
  // The panel's own tabs (its "Library" search tab shares a name with the skin's Library tab).
  await page.locator('.sr-mlib-tabs').getByRole('tab', { name: tab, exact: true }).click();
  if (after) await after();
  await page.waitForTimeout(500);
  await page.locator('.sr-ply').screenshot({ path: join(out, file) });
  console.log('saved', join(out, file));
};
await panel('Library', 'library-panel-search.png', () =>
  page.getByText('Harbour Lights').first().waitFor(),
);
await panel('Playlists', 'library-panel-playlists.png', () =>
  page.getByText('Late drive').waitFor(),
);
await panel('Playlists', 'library-panel-playlist-open.png', async () => {
  await page.getByRole('button', { name: /Late drive/ }).click();
  await page.getByText('Tail Lights').first().waitFor();
});
await panel('Equalizer', 'library-panel-equalizer.png', async () => {
  await page.locator('.sr-mlib select').first().selectOption('rock');
});
await page.screenshot({ path: join(out, 'library-panel-fullviewport-1280x800.png') });
console.log('saved', join(out, 'library-panel-fullviewport-1280x800.png'));

await page.setViewportSize({ width: 360, height: 740 });
await shot('retro', 'player-retro-360.png');
await page.screenshot({ path: join(out, 'player-retro-fullviewport-360x740.png') });
console.log('saved', join(out, 'player-retro-fullviewport-360x740.png'));
await browser.close();
