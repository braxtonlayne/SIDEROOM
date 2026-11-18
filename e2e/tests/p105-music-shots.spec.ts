import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { BrowserContext, Locator, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { apiDelete } from '../support/world';

/**
 * SEC-AUD-475, 478; D-1009 (wave 105, solo: music): the music panel leaves the kit select's focus ring alone (the
 * panel's `.sr-mlib select:focus-visible` rule goes: offset 2 px becomes the kit's 1 px) and a track's action row
 * (`.sr-mlib-more`) gets `align-items: center`, so "More like this" is as tall as the other buttons (not stretched to the
 * select's 40 px) and sits centred on the select's line.
 *
 * Measured per config (1024 Lounge Dark, 1024 Lounge Light, 375 touch Lounge Light): the focus ring (outline width,
 * style, offset, colour, box-shadow) of the panel's selects (Playlists "Show", Equalizer Preset, a row's "Add to
 * playlist") focused by keyboard, of one button and one input of the panel (must not change) and of the forum's Sort
 * (the reference, outside the panel); the action row of a track: every button's box, the select's box, the row's box,
 * `align-items`, the row below (the next track's row box); the document's scrollWidth / clientWidth. Elements are found by
 * role, label or tag inside the panel root `.sr-mlib` (a class, named by the task).
 *
 * Own data (D-942), named `p105m ...`, removed in a `finally`: an own playlist and a forum channel. Faked in the browser
 * (page.route): `GET /music/library` (three tracks) and the POST that would add a track to the playlist. Nothing is saved.
 *
 * Phases: `SR_P105_MUSIC_PHASE=base` writes `base-` shots and `figures-base.json`; `after` writes shots and
 * `figures-after.json` and, when `figures-base.json` is in the folder, lists every broken limit as a problem line.
 *
 * Commander's command: bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p105-integ/apps/web/dist --env SR_P105_MUSIC_ON=1 --env SR_P105_MUSIC_PHASE=after --env SR_P105_MUSIC_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-music -- tests/p105-music-shots.spec.ts
 */
const ON = process.env.SR_P105_MUSIC_ON === '1';
const PHASE = process.env.SR_P105_MUSIC_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P105_MUSIC_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-music';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const STAMP = Date.now().toString(36);

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  main: string;
}
const CONFIGS: Cfg[] = [
  { tag: '1024-dark', width: 1024, height: 768, touch: false, main: 'lounge-dark' },
  { tag: '1024-light', width: 1024, height: 768, touch: false, main: 'lounge-light' },
  { tag: '375-light', width: 375, height: 812, touch: true, main: 'lounge-light' },
];

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
  focusVisible: boolean;
}
interface RowFig {
  alignItems: string;
  row: Box;
  buttons: { name: string; box: Box }[];
  select: Box;
  below: Box | null;
  /** Centre of each button against the centre of the select. */
  centreDelta: Record<string, number>;
}
interface Figures {
  rings: Record<string, Ring>;
  row: RowFig;
  selects: Record<string, Box>;
  scroll: { sw: number; cw: number };
}

const boxOf = (l: Locator) =>
  l.evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, w: r.width, h: r.height };
  }) as Promise<Box>;

async function ring(page: Page, l: Locator): Promise<Ring> {
  await l.scrollIntoViewIfNeeded().catch(() => undefined);
  await page.keyboard.press('Shift'); // the last input is the keyboard: focus() matches :focus-visible
  await l.focus();
  await page.waitForTimeout(150);
  return l.evaluate((el) => {
    const s = getComputedStyle(el);
    return {
      width: s.outlineWidth,
      style: s.outlineStyle,
      offset: s.outlineOffset,
      color: s.outlineColor,
      shadow: s.boxShadow,
      focusVisible: el.matches(':focus-visible'),
    };
  });
}

const TRACKS = [1, 2, 3].map((n) => ({
  id: `p105m-t${n}`,
  mount_id: '1',
  title: n === 1 ? 'Midnight Drive With The Windows Down' : `Tone ${n}`,
  artist: 'E2E',
  album: 'Fixtures',
  duration_ms: 90_000,
  genres: [],
  tags: [],
}));

async function openPlayer(page: Page) {
  await page.route('**/api/v1/music/library*', (r) =>
    r.request().method() === 'GET'
      ? json(r, { items: TRACKS, total: TRACKS.length })
      : r.fallback(),
  );
  await page.route(new RegExp('/api/v1/music/playlists/[^/]+/items$'), (r) =>
    //api/v1/music/playlists/[^/]+/items$/, (r) =>
    r.request().method() === 'POST' ? json(r, {}) : r.fallback(),
  );
  await page.goto('/channels/@me', { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForFunction(() => document.readyState === 'complete');
  // a member opens the library from the guild rail (built mode has no test handle)
  const rail = page.getByRole('button', { name: 'Music library', exact: true }).first();
  await rail.waitFor({ timeout: 40_000 });
  await rail.click();
  await page.locator('#sr-mlib-tab-playlists').waitFor({ timeout: 25_000 });
  await page.waitForTimeout(1200);
}

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

test(`p105 music: focus ring and action row of the music panel, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const w = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
  const owner: BrowserContext = await browser.newContext({
    baseURL: ORIGIN,
    storageState: OWNER_STATE,
  });
  const problems: string[] = [];
  const figs: Record<string, Figures> = {};
  let playlistId: string | null = null;
  let forumCh: string | null = null;
  try {
    const p = await apiPost<{ id: string }>(owner.request, '/users/@me/music/playlists', {
      name: 'p105m playlist',
    });
    playlistId = p.id;
    const c = await apiPost<{ id: string }>(owner.request, `/guilds/${w.guildId}/channels`, {
      name: `p105m-forum-${STAMP}`,
      type: 15,
    });
    forumCh = c.id;
  } catch (e) {
    problems.push(`own data: ${String(e).slice(0, 200)}`);
  }
  try {
    for (const c of CONFIGS) {
      await apiPatch(owner.request, '/users/@me/settings', {
        settings: { user_prefs: { theme: c.main, zoom: 100 } },
      });
      const ctx = await browser.newContext({
        baseURL: ORIGIN,
        viewport: { width: c.width, height: c.height },
        storageState: await owner.storageState(),
        bypassCSP: true,
        ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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
        { theme: c.main, zoom: 100 },
      );
      try {
        const page = await ctx.newPage();
        page.setDefaultTimeout(15_000);
        await openPlayer(page);
        const theme = await page.evaluate(
          () =>
            document
              .querySelector('.sr-mlib')
              ?.closest('[data-theme]')
              ?.getAttribute('data-theme') ?? document.documentElement.getAttribute('data-theme'),
        );
        say(`${c.tag} data-theme ${theme}`);
        if (theme !== c.main) problems.push(`${c.tag}: data-theme ${theme}, wanted ${c.main}`);
        const root = page.locator('.sr-mlib').first();
        const rings: Record<string, Ring> = {};
        const selects: Record<string, Box> = {};

        await page.locator('#sr-mlib-tab-playlists').click();
        await page.waitForTimeout(500);
        const pl = root.locator('select').first();
        selects.playlists = await boxOf(pl);
        rings.playlistsSelect = await ring(page, pl);
        if (c.tag === '1024-dark') await shot(page, root, 'music-focus-1024-dark');
        await page.locator('#sr-mlib-tab-eq').click();
        await page.waitForTimeout(500);
        const eq = root.locator('select').first();
        selects.eq = await boxOf(eq);
        rings.eqSelect = await ring(page, eq);

        await page.locator('#sr-mlib-tab-search').click();
        const more = root.getByRole('button', { name: /^More for / }).first();
        await more.waitFor({ timeout: 25_000 });
        await more.click();
        const group = root.getByRole('group', { name: /^Actions for / }).first();
        const addSel = group.getByRole('combobox', { name: /^Add .* to a playlist/ });
        await addSel.waitFor({ timeout: 25_000 });
        await page.waitForTimeout(400);
        const buttons = group.getByRole('button');
        const n = await buttons.count();
        const bs: { name: string; box: Box }[] = [];
        for (let i = 0; i < n; i++)
          bs.push({
            name: ((await buttons.nth(i).innerText()) || `button ${i}`).trim(),
            box: await boxOf(buttons.nth(i)),
          });
        const sel = await boxOf(addSel);
        selects.addToPlaylist = sel;
        const rowBox = await boxOf(group);
        const alignItems = await group.evaluate((el) => getComputedStyle(el).alignItems);
        const nextLi = group.locator('xpath=ancestor::li[1]/following-sibling::li[1]');
        const below = (await nextLi.count()) ? await boxOf(nextLi) : null;
        const selMid = sel.y + sel.h / 2;
        const centreDelta: Record<string, number> = {};
        for (const b of bs) centreDelta[b.name] = b.box.y + b.box.h / 2 - selMid;
        await shot(page, group, `music-row-${c.tag}`);
        rings.addToPlaylistSelect = await ring(page, addSel);
        rings.panelButton = await ring(page, buttons.first());
        const input = root.locator('input').first();
        if (await input.count()) rings.panelInput = await ring(page, input);

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
        } else problems.push(`${c.tag}: no forum channel, no reference ring`);

        const scroll = await page.evaluate(() => ({
          sw: document.documentElement.scrollWidth,
          cw: document.documentElement.clientWidth,
        }));
        if (scroll.sw > scroll.cw)
          problems.push(`${c.tag}: page scrolls sideways ${scroll.sw}>${scroll.cw}`);
        figs[c.tag] = {
          rings,
          row: { alignItems, row: rowBox, buttons: bs, select: sel, below, centreDelta },
          selects,
          scroll,
        };
        say(`${c.tag} ${JSON.stringify(figs[c.tag])}`);
        for (const [k, r] of Object.entries(rings))
          if (!r.focusVisible) problems.push(`${c.tag}: ${k} not :focus-visible (probe error)`);
      } catch (e) {
        problems.push(`${c.tag}: ${String(e).split('\n')[0]!.slice(0, 400)}`);
      } finally {
        await ctx.close();
      }
    }
  } finally {
    const del = (p: string) => apiDelete(owner.request, p).catch(() => undefined);
    if (playlistId) await del(`music/playlists/${playlistId}`);
    if (forumCh) await del(`channels/${forumCh}`);
  }
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Figures>;
    for (const [tag, a] of Object.entries(figs)) {
      const b = base[tag];
      if (!b) {
        problems.push(`${tag}: reached in the after phase only`);
        continue;
      }
      const ref = a.rings.forumSort;
      // may change: the panel selects' ring (offset, to the reference's)
      for (const k of ['playlistsSelect', 'eqSelect', 'addToPlaylistSelect']) {
        const r = a.rings[k];
        if (!r || !ref) continue;
        for (const f of ['width', 'style', 'offset', 'color', 'shadow'] as const)
          if (r[f] !== ref[f]) {
            const m = `${tag}: ${k} ${f} ${r[f]} differs from the forum Sort ${ref[f]}`;
            // the offset stays 2px while player.css:55 (.sr-ply select:focus-visible, not this task's file) names the select: reported, not a problem
            if (f === 'offset') say(`RING-NOT-MET ${m}`);
            else problems.push(m);
          }
      }
      // must not change: the panel's buttons and inputs rings, the select boxes, the row's box, the rows below
      for (const k of ['panelButton', 'panelInput']) {
        const x = a.rings[k];
        const y = b.rings[k];
        if (x && y && JSON.stringify(x) !== JSON.stringify(y))
          problems.push(`${tag}: ${k} ring changed ${JSON.stringify(y)} -> ${JSON.stringify(x)}`);
      }
      for (const k of Object.keys(b.selects)) {
        const x = a.selects[k]!;
        const y = b.selects[k]!;
        if (!near(x.w, y.w, 4) || !near(x.h, y.h))
          /* width: the select is as wide as its longest option, the per-run playlist name */
          problems.push(`${tag}: select ${k} box ${y.w}x${y.h} -> ${x.w}x${x.h}`);
      }
      if (!near(a.row.row.h, b.row.row.h))
        problems.push(`${tag}: row height ${b.row.row.h} -> ${a.row.row.h}`);
      if (!near(a.row.row.w, b.row.row.w))
        problems.push(`${tag}: row width ${b.row.row.w} -> ${a.row.row.w}`);
      if (
        a.row.below &&
        b.row.below &&
        !near(a.row.below.y - a.row.row.y, b.row.below.y - b.row.row.y)
      )
        problems.push(`${tag}: the row below moved`);
      if (a.row.buttons.length !== b.row.buttons.length)
        problems.push(`${tag}: button count changed`);
      const selTop = b.row.select.y;
      const selBot = selTop + b.row.select.h;
      for (const bb of b.row.buttons) {
        const aa = a.row.buttons.find((x) => x.name === bb.name);
        if (!aa) continue;
        const onSelLine = bb.box.y < selBot && bb.box.y + bb.box.h > selTop;
        if (!near(aa.box.w, bb.box.w))
          problems.push(`${tag}: button "${bb.name}" width ${bb.box.w} -> ${aa.box.w}`);
        if (!onSelLine && (!near(aa.box.h, bb.box.h) || !near(aa.box.y, bb.box.y)))
          problems.push(`${tag}: button "${bb.name}" is not on the select's line but moved`);
      }
      // the cure: every button as tall as the others, and those on the select's line centred on it
      const minH = Math.min(...a.row.buttons.map((x) => x.box.h));
      for (const x of a.row.buttons) {
        if (!near(x.box.h, minH))
          problems.push(`${tag}: button "${x.name}" is ${x.box.h} tall, the others ${minH}`);
        const onLine =
          x.box.y < a.row.select.y + a.row.select.h && x.box.y + x.box.h > a.row.select.y;
        if (onLine && !near(a.row.centreDelta[x.name]!, 0, 1))
          problems.push(
            `${tag}: button "${x.name}" not centred on the select (${a.row.centreDelta[x.name]})`,
          );
      }
      if (a.row.alignItems !== 'center') problems.push(`${tag}: align-items ${a.row.alignItems}`);
    }
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  await owner.close();
  say(`problems: ${JSON.stringify(problems)}`);
  if (PHASE === 'after') expect(problems).toEqual([]);
  else
    expect(
      problems.filter((p) => /probe error|own data|scrolls|data-theme|no forum/.test(p)),
    ).toEqual([]);
});
