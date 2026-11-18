import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { BrowserContext, Locator, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { apiDelete } from '../support/world';

/**
 * SEC-AUD-475; D-1015 (wave 106, solo: ring): player.css names a select only for the raw select it still has
 * (`.sr-ply .ss-presets`), so a kit select inside the player shows the kit's own focus ring (offset 1 px) and the kit's font,
 * colour and cursor.
 *
 * Measured (Lounge Dark, 1024 x 768), each select focused by keyboard: outline width/style/offset/colour, box-shadow,
 * font-family, font-size, colour, cursor, and its box. Selects: the music panel's Playlists "Show", Equalizer Preset, a track
 * row's "Add to playlist", the Stack skin's presets (raw), and the forum's Sort (kit, outside the player: the reference). Also
 * the ring of one panel button and one input (must not change) and the document's scroll width.
 *
 * Own data (D-942), named `p106r ...`, removed in a `finally`: an own playlist and a forum channel. Faked in the browser
 * (page.route): `GET /music/library` (three tracks) and the POST that would add a track to a playlist. Nothing is saved.
 * The Stack skin is reached the way a member does: the skin button ("Switch to the ... skin") clicked until it shows.
 *
 * Phases: `SR_P106_RING_PHASE=base` writes `base-` shots and `figures-base.json`; `after` writes shots and `figures-after.json`
 * and, when `figures-base.json` is in the folder, lists every broken limit as a problem line.
 *
 * Commander's command: bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p106-integ/apps/web/dist --env SR_P106_RING_ON=1 --env SR_P106_RING_PHASE=after --env SR_P106_RING_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-ring -- tests/p106-ring-shots.spec.ts
 */
const ON = process.env.SR_P106_RING_ON === '1';
const PHASE = process.env.SR_P106_RING_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P106_RING_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-ring';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const STAMP = Date.now().toString(36);

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Ring {
  width: string;
  style: string;
  offset: string;
  color: string;
  shadow: string;
  fontFamily: string;
  fontSize: string;
  textColor: string;
  cursor: string;
  box: Box;
  focusVisible: boolean;
}
interface Figures {
  rings: Record<string, Ring>;
  scroll: { sw: number; cw: number };
}

async function ring(page: Page, l: Locator): Promise<Ring> {
  await l.scrollIntoViewIfNeeded().catch(() => undefined);
  await page.keyboard.press('Shift'); // the last input is the keyboard: focus() matches :focus-visible
  await l.focus();
  await page.waitForTimeout(150);
  return l.evaluate((el) => {
    const s = getComputedStyle(el);
    const r = el.getBoundingClientRect();
    return {
      width: s.outlineWidth,
      style: s.outlineStyle,
      offset: s.outlineOffset,
      color: s.outlineColor,
      shadow: s.boxShadow,
      fontFamily: s.fontFamily,
      fontSize: s.fontSize,
      textColor: s.color,
      cursor: s.cursor,
      box: { x: r.x, y: r.y, w: r.width, h: r.height },
      focusVisible: el.matches(':focus-visible'),
    };
  });
}

const TRACKS = [1, 2, 3].map((n) => ({
  id: `p106r-t${n}`,
  mount_id: '1',
  title: n === 1 ? 'Midnight Drive With The Windows Down' : `Tone ${n}`,
  artist: 'E2E',
  album: 'Fixtures',
  duration_ms: 90_000,
  genres: [],
  tags: [],
}));

async function shot(page: Page, root: Locator, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(400);
  const vp = page.viewportSize() ?? { width: 1024, height: 768 };
  const b = await root.boundingBox().catch(() => null);
  let clip: { x: number; y: number; width: number; height: number } | undefined;
  if (b) {
    const x = Math.max(0, b.x);
    const y = Math.max(0, b.y);
    const width = Math.min(vp.width, b.x + b.width) - x;
    const height = Math.min(vp.height, b.y + b.height) - y;
    if (width >= 200 && height >= 60) clip = { x, y, width, height };
  }
  await page.screenshot({
    path: resolve(OUT, `${shotName(name)}.png`),
    timeout: 10_000,
    ...(clip ? { clip } : {}),
  });
}

const near = (a: number, b: number, tol = 0.6) => Math.abs(a - b) <= tol;
const RING_FIELDS = ['width', 'style', 'offset', 'color', 'shadow'] as const;
const LOOK_FIELDS = ['fontFamily', 'fontSize', 'textColor', 'cursor'] as const;

test(`p106 ring: focus ring of selects inside the player, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const w = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
  const owner: BrowserContext = await browser.newContext({
    baseURL: ORIGIN,
    storageState: OWNER_STATE,
  });
  const problems: string[] = [];
  const probe: string[] = [];
  const figs: Figures = { rings: {}, scroll: { sw: 0, cw: 0 } };
  let playlistId: string | null = null;
  let forumCh: string | null = null;
  try {
    const p = await apiPost<{ id: string }>(owner.request, '/users/@me/music/playlists', {
      name: 'p106r playlist',
    });
    playlistId = p.id;
    const c = await apiPost<{ id: string }>(owner.request, `/guilds/${w.guildId}/channels`, {
      name: `p106r-forum-${STAMP}`,
      type: 15,
    });
    forumCh = c.id;
  } catch (e) {
    problems.push(`own data: ${String(e).slice(0, 200)}`);
  }
  try {
    await apiPatch(owner.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-dark', zoom: 100 } },
    });
    const ctx = await browser.newContext({
      baseURL: ORIGIN,
      viewport: { width: 1024, height: 768 },
      storageState: await owner.storageState(),
      bypassCSP: true,
    });
    await ctx.addInitScript(
      (pr) => {
        try {
          localStorage.setItem(
            'sr.user.prefs',
            JSON.stringify({ state: { prefs: pr }, version: 2 }),
          );
        } catch {
          /* no storage */
        }
      },
      { theme: 'lounge-dark', zoom: 100 },
    );
    try {
      const page = await ctx.newPage();
      page.setDefaultTimeout(15_000);
      await page.route('**/api/v1/music/library*', (r) =>
        r.request().method() === 'GET'
          ? json(r, { items: TRACKS, total: TRACKS.length })
          : r.fallback(),
      );
      await page.route(/\/api\/v1\/music\/playlists\/[^/]+\/items$/, (r) =>
        r.request().method() === 'POST' ? json(r, {}) : r.fallback(),
      );
      await page.goto('/channels/@me', { waitUntil: 'domcontentloaded', timeout: 30_000 });
      await page.waitForFunction(() => document.readyState === 'complete');
      const rail = page.getByRole('button', { name: 'Music library', exact: true }).first();
      await rail.waitFor({ timeout: 40_000 });
      await rail.click();
      await page.locator('#sr-mlib-tab-playlists').waitFor({ timeout: 25_000 });
      await page.waitForTimeout(1200);
      const theme = await page.evaluate(
        () =>
          document.querySelector('.sr-mlib')?.closest('[data-theme]')?.getAttribute('data-theme') ??
          document.documentElement.getAttribute('data-theme'),
      );
      say(`data-theme ${theme}`);
      if (theme !== 'lounge-dark') problems.push(`data-theme ${theme}, wanted lounge-dark`);
      const root = page.locator('.sr-mlib').first();
      const rings: Record<string, Ring> = {};

      await page.locator('#sr-mlib-tab-playlists').click();
      await page.waitForTimeout(500);
      rings.playlistsSelect = await ring(page, root.locator('select').first());
      await shot(page, root, 'ring-playlists-1024-dark');
      await page.locator('#sr-mlib-tab-eq').click();
      await page.waitForTimeout(500);
      rings.eqSelect = await ring(page, root.locator('select').first());

      await page.locator('#sr-mlib-tab-search').click();
      const more = root.getByRole('button', { name: /^More for / }).first();
      await more.waitFor({ timeout: 25_000 });
      await more.click();
      const group = root.getByRole('group', { name: /^Actions for / }).first();
      const addSel = group.getByRole('combobox', { name: /^Add .* to a playlist/ });
      await addSel.waitFor({ timeout: 25_000 });
      await page.waitForTimeout(400);
      rings.addToPlaylistSelect = await ring(page, addSel);
      await shot(page, group, 'ring-row-1024-dark');
      rings.panelButton = await ring(page, group.getByRole('button').first());
      const input = root.locator('input').first();
      if (await input.count()) rings.panelInput = await ring(page, input);

      // the Stack skin, reached with the skin button as a member does
      const ply = page.locator('.sr-ply').first();
      let skin = await ply.getAttribute('data-skin');
      for (let i = 0; i < 9 && skin !== 'stack'; i++) {
        await ply
          .getByRole('button', { name: /^Switch to the .* skin$/ })
          .first()
          .click();
        await page.waitForTimeout(700);
        skin = await page.locator('.sr-ply').first().getAttribute('data-skin');
      }
      say(`skin ${skin}`);
      if (skin !== 'stack') problems.push(`the Stack skin was not reached (skin ${skin})`);
      else {
        await page.waitForTimeout(1200);
        const presets = page.locator('.sr-ply[data-skin="stack"] .ss-presets');
        if (await presets.count()) {
          rings.stackPresets = await ring(page, presets.first());
          await shot(page, page.locator('.sr-ply[data-skin="stack"]'), 'ring-stack-presets-1024');
        } else probe.push('Stack skin shows no .ss-presets select (probe error)');
      }
      // other selects in the player, whichever skin is up
      say(
        `selects in .sr-ply: ${await page.evaluate(() => document.querySelectorAll('.sr-ply select').length)}`,
      );

      if (forumCh) {
        const p2 = await ctx.newPage();
        p2.setDefaultTimeout(15_000);
        await p2.goto(`/channels/${w.guildId}/${forumCh}`, {
          waitUntil: 'domcontentloaded',
          timeout: 30_000,
        });
        const sort = p2.getByRole('combobox', { name: 'Sort by' });
        await sort.waitFor({ timeout: 40_000 });
        rings.forumSort = await ring(p2, sort);
        await p2.close();
      } else problems.push('no forum channel, no reference ring');

      figs.scroll = await page.evaluate(() => ({
        sw: document.documentElement.scrollWidth,
        cw: document.documentElement.clientWidth,
      }));
      if (figs.scroll.sw > figs.scroll.cw)
        problems.push(`page scrolls sideways ${figs.scroll.sw}>${figs.scroll.cw}`);
      figs.rings = rings;
      for (const [k, r] of Object.entries(rings)) {
        say(`${k} ${JSON.stringify(r)}`);
        if (!r.focusVisible) problems.push(`${k} not :focus-visible (probe error)`);
      }
    } catch (e) {
      problems.push(String(e).split('\n')[0]!.slice(0, 400));
    } finally {
      await ctx.close();
    }
  } finally {
    const del = (p: string) => apiDelete(owner.request, p).catch(() => undefined);
    if (playlistId) await del(`music/playlists/${playlistId}`);
    if (forumCh) await del(`channels/${forumCh}`);
  }
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  const ref = figs.rings.forumSort;
  if (PHASE === 'base' && ref) {
    // what the base rules give the kit selects inside the player against the reference (info, not a limit)
    for (const k of ['playlistsSelect', 'eqSelect', 'addToPlaylistSelect']) {
      const r = figs.rings[k];
      if (!r) continue;
      for (const f of [...RING_FIELDS, ...LOOK_FIELDS])
        if (r[f] !== ref[f]) say(`BASE-DIFF ${k} ${f}: ${r[f]} | reference ${ref[f]}`);
    }
  }
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Figures;
    // may change: the kit selects' offset and, if the first list reached them, font, colour, cursor (reported)
    for (const k of ['playlistsSelect', 'eqSelect', 'addToPlaylistSelect']) {
      const r = figs.rings[k];
      const b = base.rings[k];
      if (!r || !ref) continue;
      for (const f of RING_FIELDS)
        if (r[f] !== ref[f])
          problems.push(`${k} ${f} ${r[f]} differs from the forum Sort ${ref[f]}`);
      for (const f of LOOK_FIELDS)
        if (b && r[f] !== b[f])
          say(`LOOK-CHANGED ${k} ${f}: ${b[f]} -> ${r[f]} (reference ${ref[f]})`);
      // must not change: any box
      if (b && (!near(r.box.w, b.box.w, 4) || !near(r.box.h, b.box.h)))
        problems.push(`${k} box ${b.box.w}x${b.box.h} -> ${r.box.w}x${r.box.h}`);
    }
    // must not change: the Stack presets (every value) and the ring of buttons and inputs
    for (const k of ['stackPresets', 'panelButton', 'panelInput']) {
      const x = figs.rings[k];
      const y = base.rings[k];
      if (!x || !y) {
        if (!!x !== !!y) problems.push(`${k} reached in one phase only`);
        continue;
      }
      if (JSON.stringify(x) !== JSON.stringify(y))
        problems.push(`${k} changed ${JSON.stringify(y)} -> ${JSON.stringify(x)}`);
    }
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  await owner.close();
  for (const p of probe) say(p);
  say(`problems: ${JSON.stringify(problems)}`);
  if (PHASE === 'after') expect(problems).toEqual([]);
  else
    expect(
      problems.filter((p) => /probe error|own data|scrolls|data-theme|no forum/.test(p)),
    ).toEqual([]);
});
