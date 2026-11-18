import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, apiPut, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';
import { serveTone } from '../support/playerAudio';
import { setStudySurfacesWith } from '../support/studySwitches';
import { apiDelete } from '../support/world';

/**
 * D-958 (wave 102, pod media: watch, media, chat): 15 raw `<select>` move to the kit's `NativeSelect` in the watch
 * party (People settings x2, title picker season, start dialog destination, control bar speed), the music panel
 * (Playlists "whose", Equalizer preset, a track row's "add to playlist"), the Study's file-details dialog (cover),
 * Live TV's reminders lead time, the Stack player skin's EQ presets, and the Lounge (Follow dialog, forum sort,
 * search sort, emoji skin tone). A record of every select's box, of every control's box in its section / dialog, the
 * exact hit test, base then after, plus the assertions the comparison needs. Selects are found by TAG inside a root
 * found by role, label or id, never by class. NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by
 * `page-sweep.spec.ts`'s "seed the sweep world"; never edited).
 *
 * Own data (D-942), all named `p102m ...` and removed in a `finally`: a movie and a series (published, a fake asset id)
 * and a party on the movie plus an empty party (Cinema), a Live TV channel, an announcement, a forum and a text
 * channel, an own playlist. The five storefront switches are put on for the run and off again. Faked in the browser
 * (page.route, as `study-shelves.spec.ts` does): the Study's shelf and files, `GET /music/library` (three tracks),
 * the POST that would add a track to the playlist, and the settings PATCH of the reminders lead time. Nothing else
 * is saved: the keyboard cases change a value and put it back.
 *
 * Phases: `SR_P102_MEDIA_PHASE=base` records the unmerged tree (shots get a `base-` prefix, figures go to
 * `figures-base.json`); the default `after` records the same and, when `figures-base.json` is in the folder, compares
 * (block size not under 40, not under 44 at 375 touch, not smaller than the base; inline size not smaller than the
 * base; a control below or beside moved by no more than the summed height difference above it; the select's label
 * not taller than the select's growth; nothing overlapping; the hit test equal in count and no row worse), writes
 * `compare.json` and a side-by-side picture `cmp-<scenario>-<config>.png` of base and after for each shot.
 * Configs: the Lounge's (1024 Lounge Dark, 1024 Lounge Light, 375 touch Lounge Light) for the Lounge, the music
 * panel and the Stack skin; the sections' (1024 shipped, 375 touch shipped, 1024 light) for the Cinema (watch party),
 * Live TV and the Study (`sectionThemes` cinema / live-tv / library set explicitly and read back). `data-theme` is read
 * back from the page before each shot. `SR_P102_MEDIA_ALL=1` adds the scenarios whose selects stay raw (equal in both phases, a check that nothing else moved). `SR_P102_MEDIA_ONLY` is a regex on the scenario key (fix a spec error without
 * running all).
 *
 * Local only (SR_P102_MEDIA_ON=1). SR_P102_MEDIA_OUT is the folder of the shots and the figures.
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8799, Vite 5799, metrics 9499,
 * `SR_ADMIN_REQUIRE_REAUTH=0`, `SR_VOICE__ENABLED` not needed; `DATABASE_URL` and `SR_CLI_BIN` set to the stack's
 * database and sr-cli). Base (the tree before the three pod branches), then, on the merged tree, after:
 *   SR_E2E_BASE_URL=http://localhost:5799 SR_P102_MEDIA_ON=1 SR_P102_MEDIA_PHASE=base \
 *   SR_P102_MEDIA_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-media \
 *   npx playwright test tests/p102-media-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 *   (the same with SR_P102_MEDIA_PHASE=after)
 * After the merge there is no base to compare with: delete `figures-base.json` first if the folder holds one from
 * another tree.
 */
const ON = process.env.SR_P102_MEDIA_ON === '1';
const PHASE = process.env.SR_P102_MEDIA_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P102_MEDIA_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-media';
const ALL = process.env.SR_P102_MEDIA_ALL === '1';
const ONLY = process.env.SR_P102_MEDIA_ONLY ? new RegExp(process.env.SR_P102_MEDIA_ONLY) : null;
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const STAMP = Date.now().toString(36);

type Family = 'lounge' | 'section';
interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  family: Family;
  main: string;
  /** sectionThemes the account gets (all three set explicitly). */
  sections: { cinema: string; library: string; 'live-tv': string };
}
const SHIPPED = { cinema: 'sideroom', library: 'sideroom', 'live-tv': 'sideroom-broadcast' };
const LIGHT = { cinema: 'lounge-light', library: 'lounge-light', 'live-tv': 'lounge-light' };
const CONFIGS: Cfg[] = [
  {
    tag: '1024-lounge-dark',
    width: 1024,
    height: 768,
    touch: false,
    family: 'lounge',
    main: 'lounge-dark',
    sections: SHIPPED,
  },
  {
    tag: '1024-lounge-light',
    width: 1024,
    height: 768,
    touch: false,
    family: 'lounge',
    main: 'lounge-light',
    sections: SHIPPED,
  },
  {
    tag: '375-lounge-light',
    width: 375,
    height: 812,
    touch: true,
    family: 'lounge',
    main: 'lounge-light',
    sections: SHIPPED,
  },
  {
    tag: '1024-shipped',
    width: 1024,
    height: 768,
    touch: false,
    family: 'section',
    main: 'lounge-dark',
    sections: SHIPPED,
  },
  {
    tag: '375-shipped',
    width: 375,
    height: 812,
    touch: true,
    family: 'section',
    main: 'lounge-dark',
    sections: SHIPPED,
  },
  {
    tag: '1024-light',
    width: 1024,
    height: 768,
    touch: false,
    family: 'section',
    main: 'lounge-dark',
    sections: LIGHT,
  },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);

// ------------------------------------------------------------------------------------------------ the data

interface Ids {
  guildId: string;
  textCh: string;
  newsCh: string | null;
  forumCh: string | null;
  movieId: string | null;
  seriesId: string | null;
  seriesName: string;
  partyId: string | null;
  emptyPartyId: string | null;
  tvId: string | null;
  playlistId: string | null;
  playlistName: string;
}

const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

async function makeData(o: BrowserContext['request'], notes: string[]): Promise<Ids> {
  const w = world();
  const ids: Ids = {
    guildId: w.guildId,
    textCh: w.channelId,
    newsCh: null,
    forumCh: null,
    movieId: null,
    seriesId: null,
    seriesName: `p102m series ${STAMP}`,
    partyId: null,
    emptyPartyId: null,
    tvId: null,
    playlistId: null,
    playlistName: `p102m playlist ${STAMP}`,
  };
  const attempt = async <T>(what: string, fn: () => Promise<T>): Promise<T | null> => {
    try {
      return await fn();
    } catch (e) {
      notes.push(`${what}: ${String(e).slice(0, 220)}`);
      return null;
    }
  };
  await attempt('storefront switches', () => setStudySurfacesWith(o, true));
  const title = (kind: 'movie' | 'series', name: string) =>
    attempt(`title ${name}`, async () => {
      const t = await apiPost<{ id: string }>(o, '/admin/cinema/titles', {
        kind,
        title: name,
        year: 2001,
        runtime_min: 118,
        content_rating: 'PG',
        synopsis: 'A fixture for the pod media select check.',
        genres: ['Drama'],
        languages: ['en'],
        subtitle_languages: ['en'],
        cast: [],
        directors: [],
        writers: [],
        studios: [],
        crew: [],
        alt_titles: [],
        tags: [],
        age_restricted: false,
        external_ids: {},
      });
      await apiPut(o, `/admin/cinema/titles/${t.id}/asset`, { asset_id: '4242' });
      await apiPost(o, `/admin/cinema/titles/${t.id}/publish`);
      return t.id;
    });
  ids.movieId = await title('movie', `p102m movie ${STAMP}`);
  ids.seriesId = await title('series', ids.seriesName);
  if (ids.movieId)
    ids.partyId = await attempt('party on the movie', async () => {
      const p = await apiPost<{ id: string }>(o, '/watch-parties', { title_id: ids.movieId });
      return p.id;
    });
  ids.emptyPartyId = await attempt('empty party', async () => {
    const p = await apiPost<{ id: string }>(o, '/watch-parties', {});
    return p.id;
  });
  ids.tvId = await attempt('tv channel', async () => {
    const c = await apiPost<{ id: string }>(o, '/admin/tv/channels', {
      number: 700 + (Date.now() % 250),
      name: `p102m TV ${STAMP}`,
      kind: 'manual',
      timezone: 'UTC',
      enabled: true,
    });
    return c.id;
  });
  const ch = (name: string, type: number) =>
    attempt(`channel ${name}`, async () => {
      const c = await apiPost<{ id: string }>(o, `/guilds/${w.guildId}/channels`, { name, type });
      return c.id;
    });
  ids.newsCh = await ch(`p102m-news-${STAMP}`, 5);
  ids.forumCh = await ch(`p102m-forum-${STAMP}`, 15);
  ids.playlistId = await attempt('playlist', async () => {
    const p = await apiPost<{ id: string }>(o, '/users/@me/music/playlists', {
      name: ids.playlistName,
    });
    return p.id;
  });
  say(`data ${JSON.stringify(ids)} notes ${JSON.stringify(notes)}`);
  return ids;
}

async function removeData(o: BrowserContext['request'], ids: Ids) {
  const del = async (p: string) => apiDelete(o, p).catch(() => undefined);
  if (ids.partyId) await del(`watch-parties/${ids.partyId}`);
  if (ids.emptyPartyId) await del(`watch-parties/${ids.emptyPartyId}`);
  if (ids.tvId) await del(`admin/tv/channels/${ids.tvId}`);
  if (ids.newsCh) await del(`channels/${ids.newsCh}`);
  if (ids.forumCh) await del(`channels/${ids.forumCh}`);
  if (ids.playlistId) await del(`music/playlists/${ids.playlistId}`);
  for (const t of [ids.movieId, ids.seriesId])
    if (t) {
      await apiPost(o, `/admin/cinema/titles/${t}/unpublish`).catch(() => undefined);
      await del(`admin/cinema/titles/${t}`);
    }
  await setStudySurfacesWith(o, false).catch(() => undefined);
}

// ------------------------------------------------------------------------------------------------ the browser

async function setPrefs(ownerCtx: BrowserContext, c: Cfg) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: c.main, zoom: 100, sectionThemes: c.sections } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: {
      user_prefs?: { theme?: string; sectionThemes?: Record<string, string> };
    };
  };
  const p = back.settings?.user_prefs;
  expect(p?.theme).toBe(c.main);
  for (const k of ['cinema', 'library', 'live-tv'] as const)
    expect(p?.sectionThemes?.[k]).toBe(c.sections[k]);
}

async function newContext(browser: Browser, ownerCtx: BrowserContext, c: Cfg) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.width, height: c.height },
    storageState: await ownerCtx.storageState(),
    bypassCSP: true,
    ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript(
    (p) => {
      try {
        localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
      } catch {
        /* no storage */
      }
    },
    { theme: c.main, zoom: 100, sectionThemes: c.sections },
  );
  return ctx;
}

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface SelFig {
  key: string;
  box: Box;
  label: Box | null;
  options: number;
  wrapper: string | null;
}
interface Measured {
  themes: string[];
  scroll: number[];
  rootScroll: number[];
  root: Box | null;
  selects: SelFig[];
  controls: { key: string; box: Box }[];
  overlaps: string[];
}
interface Figures extends Measured {
  hit: { pass: number; fail: number; notReached: number; skipped: number };
  hitFails: { status: string; control: string; takers: string[] }[];
  opened: unknown[];
  keyboard?: Record<string, unknown>;
  shot?: string;
}

/** Boxes in the root's coordinates are page coordinates plus every scrolled ancestor's offset (a scrolled page compares with a top one). */
async function measure(root: Locator): Promise<Measured> {
  return root.evaluate((rootEl) => {
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      let sx = 0;
      let sy = 0;
      for (let p: Element | null = e.parentElement; p; p = p.parentElement) {
        sx += p.scrollLeft;
        sy += p.scrollTop;
      }
      return {
        x: Math.round((r.left + sx) * 10) / 10,
        y: Math.round((r.top + sy) * 10) / 10,
        w: Math.round(r.width * 10) / 10,
        h: Math.round(r.height * 10) / 10,
      };
    };
    const els = Array.from(
      rootEl.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
    ).filter((e) => e.getClientRects().length > 0);
    // The accessible-ish name: aria-label, else the enclosing label's own text with every select taken out, else the text.
    const name = (e: HTMLElement) => {
      const al = e.getAttribute('aria-label');
      if (al) return al.trim().slice(0, 40);
      const lab = e.closest('label');
      if (lab) {
        const c = lab.cloneNode(true) as HTMLElement;
        c.querySelectorAll('select').forEach((s) => s.remove());
        const tx = (c.textContent ?? '').trim();
        if (tx) return tx.slice(0, 40);
      }
      return (e.id || e.textContent || '').trim().slice(0, 40);
    };
    const controls = els.map((e, i) => ({
      key: `${i}:${e.tagName.toLowerCase()}:${name(e)}`,
      box: box(e),
    }));
    const selEls = els.filter((e) => e.tagName === 'SELECT') as HTMLSelectElement[];
    const selects = selEls.map((e) => {
      const lab = e.closest('label');
      const par = e.parentElement;
      return {
        key: `${els.indexOf(e)}:select:${name(e)}`,
        box: box(e),
        label: lab ? box(lab) : null,
        options: e.options.length,
        wrapper: par && par.tagName === 'SPAN' && par !== lab ? par.className || 'span' : null,
      };
    });
    const overlaps: string[] = [];
    for (const s of selEls) {
      const a = s.getBoundingClientRect();
      for (const o of els) {
        if (o === s || s.contains(o) || o.contains(s)) continue;
        const b = o.getBoundingClientRect();
        const ix = Math.min(a.right, b.right) - Math.max(a.left, b.left);
        const iy = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
        if (ix > 1 && iy > 1)
          overlaps.push(
            `select(${name(s)}) x ${o.tagName.toLowerCase()}(${name(o)}) ${Math.round(ix)}x${Math.round(iy)}`,
          );
      }
    }
    const themes = Array.from(
      new Set(
        [
          document.documentElement,
          ...Array.from(document.querySelectorAll('[data-theme]')).slice(0, 6),
        ]
          .map((e) => e.getAttribute('data-theme'))
          .filter((x): x is string => !!x),
      ),
    );
    return {
      themes,
      scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      rootScroll: [(rootEl as HTMLElement).scrollWidth, (rootEl as HTMLElement).clientWidth],
      root: box(rootEl),
      selects,
      controls,
      overlaps,
    };
  });
}

const count = (rows: ControlRow[]) => ({
  pass: rows.filter((r) => r.status === 'pass').length,
  fail: rows.filter((r) => r.status === 'fail').length,
  notReached: rows.filter((r) => r.status === 'not-reached').length,
  skipped: rows.filter((r) => r.status === 'skipped').length,
});

async function hit(page: Page, root: Locator, tag: string, problems: string[]) {
  const opened = await openCollapsed(page, root);
  const rows = await hitTestAll(page, root);
  const h = count(rows);
  const hitFails = rows
    .filter((r) => r.status === 'fail' || r.status === 'not-reached')
    .map((r) => ({
      status: r.status as string,
      control: `${r.tag} "${r.name}"`,
      takers: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map((p) => `${p.at}:${p.taker ? `${p.taker.tag}"${p.taker.name}"` : 'null'}`),
    }));
  say(`${tag}: hit ${JSON.stringify(h)}`);
  if (h.notReached) problems.push(`${tag}: ${h.notReached} controls not reached (probe error)`);
  return { h, hitFails, opened };
}

async function shot(page: Page, root: Locator, firstSelect: Locator | null, name: string) {
  mkdirSync(OUT, { recursive: true });
  if (firstSelect) await firstSelect.scrollIntoViewIfNeeded().catch(() => undefined);
  await page.waitForTimeout(500);
  const vp = page.viewportSize() ?? { width: 1024, height: 768 };
  const b = await root.boundingBox().catch(() => null);
  let clip: { x: number; y: number; width: number; height: number } | undefined;
  if (b) {
    const x = Math.max(0, b.x);
    const y = Math.max(0, b.y);
    const width = Math.min(vp.width, b.x + b.width) - x;
    const height = Math.min(vp.height, b.y + b.height) - y;
    if (width >= 200 && height >= 120) clip = { x, y, width, height };
  }
  const file = resolve(OUT, `${shotName(name)}.png`);
  await page.screenshot({ path: file, timeout: 10_000, ...(clip ? { clip } : {}) });
  return `${shotName(name)}.png`;
}

/** base | after side by side, drawn in a scratch page. */
async function composite(browser: Browser, name: string) {
  const a = resolve(OUT, `base-${name}.png`);
  const b = resolve(OUT, `${name}.png`);
  if (!existsSync(a) || !existsSync(b)) return;
  const ctx = await browser.newContext({ viewport: { width: 1200, height: 800 } });
  try {
    const p = await ctx.newPage();
    const img = (f: string) => `data:image/png;base64,${readFileSync(f).toString('base64')}`;
    await p.setContent(
      `<body style="margin:0;background:#777;font:14px sans-serif;color:#fff"><div style="display:flex;gap:10px;align-items:flex-start;padding:6px">` +
        `<div><div>BASE</div><img src="${img(a)}"></div><div><div>AFTER</div><img src="${img(b)}"></div></div></body>`,
    );
    await p.waitForFunction(() => Array.from(document.images).every((i) => i.complete));
    await p.screenshot({ path: resolve(OUT, `cmp-${name}.png`), fullPage: true });
  } finally {
    await ctx.close();
  }
}

// ------------------------------------------------------------------------------------------------ scenarios

interface Kbd {
  pick: (root: Locator) => Locator;
  /** The select's value does not change by itself (a "pick one to add" select): a request is the effect. */
  noChange?: boolean;
  settleMs?: number;
}
interface Scenario {
  key: string;
  files: string[];
  family: Family;
  /** The section whose theme `data-theme` must show (section family). */
  section?: 'cinema' | 'library' | 'live-tv';
  /** In the default set: its select is moved by a code builder. The others (left raw) run with SR_P102_MEDIA_ALL=1. */
  core: boolean;
  /** Scenarios of one group share one page per config. */
  group: string;
  minSelects: number;
  skip?: (c: Cfg) => string | null;
  open: (page: Page, c: Cfg, ids: Ids, first: boolean, log: string[]) => Promise<Locator>;
  /** The select to scroll to before the shot. */
  focus?: (root: Locator) => Locator;
  kbd?: Kbd;
}

const SIGNAL = { timeout: 25_000 };
const goto = (page: Page, path: string) =>
  page.goto(path, { waitUntil: 'domcontentloaded', timeout: 30_000 });
const settled = async (page: Page) => {
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
};

const MUSIC_TRACKS = [1, 2, 3].map((n) => ({
  id: `p102m-t${n}`,
  mount_id: '1',
  title: n === 1 ? 'Midnight Drive With The Windows Down' : `Tone ${n}`,
  artist: 'E2E',
  album: 'Fixtures',
  duration_ms: 90_000,
  genres: [],
  tags: [],
}));
const PLAY_TRACKS = [1, 2, 3].map((n) => ({
  id: `p102m-p${n}`,
  title: `Tone ${n}`,
  artist: 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));
type PlayerWin = {
  __srPlayerActions: { setSkin(k: string): void; play(t: unknown): void };
  __srPlayer: { audio: HTMLAudioElement };
};

async function openPlayer(page: Page, skin: string, ids: Ids) {
  await serveTone(page);
  await page.route('**/api/v1/music/library*', (r) =>
    r.request().method() === 'GET'
      ? json(r, { items: MUSIC_TRACKS, total: MUSIC_TRACKS.length })
      : r.fallback(),
  );
  await page.route(/\/api\/v1\/music\/playlists\/[^/]+\/items$/, (r) =>
    r.request().method() === 'POST' ? json(r, {}) : r.fallback(),
  );
  void ids;
  await goto(page, '/channels/@me');
  await page.waitForFunction(() => document.readyState === 'complete');
  await page.mouse.click(2, 2);
  await page.waitForFunction(() => (window as unknown as Partial<PlayerWin>).__srPlayerActions, {
    timeout: 30_000,
  });
  await page.evaluate(
    (t) => (window as unknown as PlayerWin).__srPlayerActions.play(t),
    PLAY_TRACKS,
  );
  await page.waitForFunction(() => (window as unknown as Partial<PlayerWin>).__srPlayer);
  await page.evaluate((k) => (window as unknown as PlayerWin).__srPlayerActions.setSkin(k), skin);
  await page.locator(`.sr-ply[data-skin="${skin}"]`).waitFor(SIGNAL);
  await page.waitForTimeout(1200);
}

async function musicTab(page: Page, c: Cfg, ids: Ids, first: boolean, tab: string) {
  if (first) await openPlayer(page, 'library', ids);
  await page.locator(`#sr-mlib-tab-${tab}`).click();
  await page.waitForTimeout(500);
  void c;
  return page.locator('.sr-mlib').first();
}

const STUDY_FILES = [
  { key: 'Films/heat.mp4', size: 1000 },
  { key: 'Music/blue.mp3', size: 2000 },
  { key: 'Covers/one.png', size: 3000 },
  { key: 'Covers/two.png', size: 4000 },
];
async function fakeStudy(page: Page) {
  const API = '**/api/v1';
  await page.route(`${API}/shelves/visible`, (r) =>
    json(r, {
      shelves: [{ shelf_id: '9501', label: 'p102m Home', via: 'own', object_count: 4 }],
    }),
  );
  await page.route(`${API}/storage/mounts/*/objects*`, (r) =>
    json(r, {
      folders: [],
      objects: STUDY_FILES.map((f) => ({ ...f, last_modified: '2026-10-01T00:00:00Z' })),
      next_cursor: null,
      can_upload: false,
      can_manage: true,
    }),
  );
  await page.route(/\/api\/v1\/shelves\/\d+\/objects(\?.*)?$/, (r) =>
    json(r, {
      objects: STUDY_FILES.map((f) => ({
        object_hash: 'a'.repeat(64),
        path: f.key,
        size: f.size,
        class: 'other',
        title: '',
        cover: '',
        note: '',
        updated_at: '2026-10-01T00:00:00Z',
      })),
      next_cursor: null,
      can_edit: true,
    }),
  );
  await page.route('https://bucket.invalid/**', (r) => r.fulfill({ status: 404 }));
}

/** A second visit finds the host out of the party ("You are not in this party any more"): rejoin. */
async function ensureIn(page: Page) {
  const rejoin = page.getByRole('button', { name: 'Rejoin the party' });
  await rejoin.waitFor({ state: 'visible', timeout: 3000 }).catch(() => undefined);
  if (await rejoin.isVisible().catch(() => false)) {
    await rejoin.click();
    await page.waitForTimeout(1500);
  }
}

const dialogOf = (page: Page, name: string | RegExp) => page.getByRole('dialog', { name }).last();

const SCENARIOS: Scenario[] = [
  {
    key: 'watch-theatre',
    core: true,
    files: ['watchparty/PeoplePanel.tsx (x2)', 'watchparty/ControlBar.tsx'],
    family: 'section',
    section: 'cinema',
    group: 'watch-theatre',
    minSelects: 3,
    async open(page, _c, ids) {
      if (!ids.partyId) throw new Error('not reached: no party on the movie could be made');
      await goto(page, `/cinema/party/${ids.partyId}`);
      await page.locator('.wp-root').waitFor({ timeout: 40_000 });
      await ensureIn(page);
      await page.getByRole('tab', { name: 'People' }).click();
      await page.locator('.wp-settings select').first().waitFor(SIGNAL);
      await settled(page);
      return page.locator('.wp-root');
    },
    focus: (root) => root.locator('.wp-settings select').first(),
    kbd: { pick: (root) => root.locator('.wp-settings select').first(), settleMs: 900 },
  },
  {
    key: 'watch-speed',
    core: true,
    files: ['watchparty/ControlBar.tsx'],
    family: 'section',
    section: 'cinema',
    group: 'watch-speed',
    minSelects: 1,
    async open(page, _c, ids) {
      if (!ids.partyId) throw new Error('not reached: no party on the movie could be made');
      await goto(page, `/cinema/party/${ids.partyId}`);
      await page.locator('.wp-root').waitFor({ timeout: 40_000 });
      await ensureIn(page);
      const bar = page.getByRole('combobox', { name: 'Speed' });
      await bar.waitFor(SIGNAL);
      await settled(page);
      return page.locator('.wp-stage-wrap');
    },
    focus: (root) => root.getByRole('combobox', { name: 'Speed' }),
    kbd: { pick: (root) => root.getByRole('combobox', { name: 'Speed' }), settleMs: 900 },
  },
  {
    key: 'watch-picker',
    core: true,
    files: ['watchparty/TitlePicker.tsx'],
    family: 'section',
    section: 'cinema',
    group: 'watch-picker',
    minSelects: 1,
    async open(page, _c, ids) {
      if (!ids.partyId || !ids.seriesId)
        throw new Error('not reached: no party or series could be made');
      await goto(page, `/cinema/party/${ids.partyId}`);
      await page.locator('.wp-root').waitFor({ timeout: 40_000 });
      await ensureIn(page);
      await page.getByRole('tab', { name: /Queue/ }).click();
      await page.getByRole('button', { name: 'Add to queue' }).first().click();
      const box = page.getByRole('textbox', { name: 'Search the cinema' });
      await box.waitFor({ timeout: 40_000 });
      await box.fill(ids.seriesName);
      const ep = page.getByRole('button', { name: /episode/i }).first();
      await ep.waitFor(SIGNAL);
      await ep.click();
      await page.getByRole('combobox', { name: 'Season' }).waitFor(SIGNAL);
      await settled(page);
      return page.locator('.wp-picker').first();
    },
    focus: (root) => root.getByRole('combobox', { name: 'Season' }),
    kbd: { pick: (root) => root.getByRole('combobox', { name: 'Season' }) },
  },
  {
    key: 'watch-start',
    core: true,
    files: ['watchparty/StartPartyDialog.tsx'],
    family: 'section',
    section: 'cinema',
    group: 'watch-start',
    minSelects: 1,
    async open(page, _c, ids) {
      const id = ids.movieId ?? ids.seriesId;
      if (!id) throw new Error('not reached: no title could be made');
      await goto(page, `/cinema/t/${id}`);
      await page.getByRole('button', { name: 'Watch together' }).first().click({ timeout: 40_000 });
      const dlg = dialogOf(page, 'Watch together');
      await dlg.getByRole('combobox', { name: 'Where to watch' }).waitFor(SIGNAL);
      await settled(page);
      return dlg;
    },
    focus: (root) => root.getByRole('combobox', { name: 'Where to watch' }),
    kbd: { pick: (root) => root.getByRole('combobox', { name: 'Where to watch' }) },
  },
  {
    key: 'tv-reminders',
    core: false,
    files: ['cinema/tv/RemindersCard.tsx'],
    family: 'section',
    section: 'live-tv',
    group: 'tv-reminders',
    minSelects: 1,
    async open(page, _c, _ids, _first, log) {
      await page.route('**/api/v1/users/@me/settings', async (r) => {
        if (r.request().method() !== 'PATCH') return r.fallback();
        const body = r.request().postDataJSON() as {
          settings?: { tv?: { reminder_lead_minutes?: number } };
        };
        log.push('reminders PATCH faked');
        return json(r, { version: 9, settings: { tv: body.settings?.tv ?? {} } });
      });
      await goto(page, '/cinema/tv');
      await page.locator('.tv-reminders select').first().waitFor({ timeout: 40_000 });
      await settled(page);
      return page.locator('.tv-reminders').first();
    },
    focus: (root) => root.locator('select').first(),
    kbd: { pick: (root) => root.locator('select').first(), settleMs: 700 },
  },
  {
    key: 'study-edit',
    core: false,
    files: ['study/StudyDialogs.tsx'],
    family: 'section',
    section: 'library',
    group: 'study-edit',
    minSelects: 1,
    async open(page) {
      await fakeStudy(page);
      await goto(page, '/study');
      const edit = page.getByRole('button', { name: /^Edit details of / }).first();
      await edit.waitFor({ timeout: 40_000 });
      await edit.click();
      const dlg = dialogOf(page, 'Edit details');
      await dlg.getByRole('combobox', { name: 'Cover' }).waitFor(SIGNAL);
      await settled(page);
      return dlg;
    },
    focus: (root) => root.getByRole('combobox', { name: 'Cover' }),
    kbd: { pick: (root) => root.getByRole('combobox', { name: 'Cover' }) },
  },
  {
    key: 'music-playlists',
    core: false,
    files: ['music/panel/sections.tsx (Playlists, whose)'],
    family: 'lounge',
    group: 'music',
    minSelects: 1,
    open: (page, c, ids, first) => musicTab(page, c, ids, first, 'playlists'),
    focus: (root) => root.locator('select').first(),
    kbd: { pick: (root) => root.locator('select').first() },
  },
  {
    key: 'music-eq',
    core: false,
    files: ['music/panel/sections.tsx (Equalizer, preset)'],
    family: 'lounge',
    group: 'music',
    minSelects: 1,
    open: (page, c, ids, first) => musicTab(page, c, ids, first, 'eq'),
    focus: (root) => root.locator('select').first(),
    kbd: { pick: (root) => root.locator('select').first() },
  },
  {
    key: 'music-tracks',
    core: true,
    files: ['music/panel/TrackList.tsx'],
    family: 'lounge',
    group: 'music',
    minSelects: 1,
    async open(page, c, ids, first) {
      if (!ids.playlistId) throw new Error('not reached: no own playlist could be made');
      const root = await musicTab(page, c, ids, first, 'search');
      const more = root.getByRole('button', { name: /^More for / }).first();
      await more.waitFor(SIGNAL);
      await more.click();
      await root.getByRole('combobox', { name: /^Add .* to a playlist/ }).waitFor(SIGNAL);
      await page.waitForTimeout(400);
      return root;
    },
    focus: (root) => root.getByRole('combobox', { name: /^Add .* to a playlist/ }).first(),
    kbd: {
      pick: (root) => root.getByRole('combobox', { name: /^Add .* to a playlist/ }).first(),
      noChange: true,
      settleMs: 600,
    },
  },
  {
    key: 'stack-eq',
    core: false,
    files: ['player/skins/backdrop/StackSkin.tsx'],
    family: 'lounge',
    group: 'stack',
    minSelects: 1,
    async open(page, _c, ids) {
      await openPlayer(page, 'stack', ids);
      await page.locator('.sr-ply[data-skin="stack"] select').first().waitFor(SIGNAL);
      return page.locator('.sr-ply[data-skin="stack"]');
    },
    focus: (root) => root.locator('select').first(),
    kbd: { pick: (root) => root.locator('select').first() },
  },
  {
    key: 'lounge-follow',
    core: true,
    files: ['lounge/header/FollowButton.tsx'],
    family: 'lounge',
    group: 'follow',
    minSelects: 1,
    async open(page, _c, ids) {
      if (!ids.newsCh) throw new Error('not reached: no announcement channel could be made');
      await goto(page, `/channels/${ids.guildId}/${ids.newsCh}`);
      const more = page.getByRole('button', { name: /more actions/i }).first();
      await more.waitFor({ state: 'visible', timeout: 6000 }).catch(() => undefined);
      if (await more.isVisible().catch(() => false)) await more.click();
      await page.getByRole('button', { name: 'Follow' }).first().click({ timeout: 40_000 });
      const dlg = page.getByRole('dialog').last();
      await dlg.locator('select').first().waitFor(SIGNAL);
      await settled(page);
      return dlg;
    },
    focus: (root) => root.locator('select').first(),
    kbd: { pick: (root) => root.locator('select').first() },
  },
  {
    key: 'lounge-forum',
    core: false,
    files: ['lounge/forum/ForumView.tsx'],
    family: 'lounge',
    group: 'forum',
    minSelects: 1,
    async open(page, _c, ids) {
      if (!ids.forumCh) throw new Error('not reached: no forum channel could be made');
      await goto(page, `/channels/${ids.guildId}/${ids.forumCh}`);
      await page.getByRole('combobox', { name: 'Sort by' }).waitFor({ timeout: 40_000 });
      await settled(page);
      return page.locator('main').first();
    },
    focus: (root) => root.getByRole('combobox', { name: 'Sort by' }),
    kbd: { pick: (root) => root.getByRole('combobox', { name: 'Sort by' }) },
  },
  {
    key: 'lounge-search',
    core: true,
    files: ['lounge/SearchBox.tsx'],
    family: 'lounge',
    group: 'search',
    minSelects: 1,
    skip: (c) => (c.touch ? 'the header search box is not shown on a phone' : null),
    async open(page, _c, ids) {
      await goto(page, `/channels/${ids.guildId}/${ids.textCh}`);
      const sb = page.getByRole('searchbox').first();
      await sb.waitFor({ timeout: 40_000 });
      await sb.fill(`p102mnone${STAMP}`);
      await sb.press('Enter');
      const panel = page.getByRole('region', { name: 'Search results' });
      await panel.waitFor(SIGNAL);
      await panel.locator('select').first().waitFor(SIGNAL);
      await settled(page);
      return panel;
    },
    focus: (root) => root.locator('select').first(),
    kbd: { pick: (root) => root.locator('select').first(), settleMs: 2200 },
  },
  {
    key: 'lounge-emoji',
    core: false,
    files: ['lounge/components/EmojiPicker.tsx'],
    family: 'lounge',
    group: 'emoji',
    minSelects: 1,
    async open(page, _c, ids) {
      await goto(page, `/channels/${ids.guildId}/${ids.textCh}`);
      await page
        .getByRole('button', { name: 'Open emoji picker' })
        .first()
        .click({ timeout: 40_000 });
      const dlg = dialogOf(page, 'Emoji picker');
      await dlg.getByRole('combobox', { name: 'Skin tone' }).waitFor(SIGNAL);
      await settled(page);
      return dlg;
    },
    focus: (root) => root.getByRole('combobox', { name: 'Skin tone' }),
    kbd: { pick: (root) => root.getByRole('combobox', { name: 'Skin tone' }) },
  },
];

// ------------------------------------------------------------------------------------------------ one scenario

async function keyboard(
  page: Page,
  root: Locator,
  k: Kbd,
  tag: string,
  requests: string[],
  problems: string[],
) {
  const sel = k.pick(root);
  const settle = k.settleMs ?? 350;
  const n = await sel.locator('option').count();
  await page.keyboard.press('Shift');
  await sel.focus();
  const focusVisible = await sel.evaluate((e) => e.matches(':focus-visible'));
  const ring = await sel.evaluate((e) => {
    const cs = getComputedStyle(e);
    return `${cs.outlineWidth} ${cs.outlineStyle} ${cs.outlineColor} off ${cs.outlineOffset}`;
  });
  const before = await sel.inputValue();
  const reqBefore = requests.length;
  await page.keyboard.press('ArrowDown');
  await page.waitForTimeout(settle);
  const changed = await sel.inputValue();
  const reqAfter = requests.length - reqBefore;
  await page.keyboard.press('ArrowUp');
  await page.waitForTimeout(settle);
  const back = await sel.inputValue();
  const out = { options: n, focusVisible, ring, before, changed, back, requests: reqAfter };
  say(`${tag}: keyboard ${JSON.stringify(out)}`);
  if (!focusVisible) problems.push(`${tag}: :focus-visible did not match after a key`);
  if (k.noChange) {
    if (reqAfter < 1) problems.push(`${tag}: ArrowDown made no request (the page did not react)`);
  } else {
    if (n > 1 && changed === before) problems.push(`${tag}: ArrowDown did not change the value`);
    if (back !== before) problems.push(`${tag}: ArrowUp did not restore the value`);
  }
  return out;
}

async function runScenario(
  s: Scenario,
  c: Cfg,
  page: Page,
  ids: Ids,
  first: boolean,
  problems: string[],
  reached: Record<string, string>,
  requests: string[],
): Promise<Figures | null> {
  const tag = `${s.key} ${c.tag}`;
  const skip = s.skip?.(c);
  if (skip) {
    reached[`${s.key}/${c.tag}`] = `skipped: ${skip}`;
    say(`${tag}: skipped (${skip})`);
    return null;
  }
  const log: string[] = [];
  let root: Locator;
  try {
    root = await s.open(page, c, ids, first, log);
  } catch (e) {
    const msg = String(e).split('\n')[0]!.slice(0, 300);
    reached[`${s.key}/${c.tag}`] = `not reached: ${msg}`;
    say(`${tag}: not reached: ${msg}`);
    if (!msg.includes('not reached:')) problems.push(`${tag}: spec could not open it: ${msg}`);
    return null;
  }
  const m = await measure(root);
  say(`${tag}: root ${JSON.stringify(m.root)} selects ${JSON.stringify(m.selects)}`);
  reached[`${s.key}/${c.tag}`] = `reached: ${m.selects.length} selects`;
  if (m.selects.length < s.minSelects)
    problems.push(`${tag}: ${m.selects.length} selects, wanted ${s.minSelects}`);
  // data-theme read back
  const wantTheme = s.family === 'lounge' ? c.main : c.sections[s.section ?? 'cinema'];
  say(`${tag}: data-theme ${JSON.stringify(m.themes)} wanted ${wantTheme}`);
  if (s.section !== 'library' && !m.themes.includes(wantTheme))
    problems.push(`${tag}: data-theme is ${JSON.stringify(m.themes)}, wanted ${wantTheme}`);
  if (m.scroll[0]! > m.scroll[1]!)
    problems.push(`${tag}: the document scrolls sideways ${m.scroll.join('/')}`);
  if (m.overlaps.length) problems.push(`${tag}: overlaps ${m.overlaps.join('; ')}`);
  const h = await hit(page, root, tag, problems);
  const fig: Figures = { ...m, hit: h.h, hitFails: h.hitFails, opened: h.opened };
  // The shot: the select in view, the pointer resting on nothing.
  await page.mouse.move(1, 1);
  fig.shot = await shot(page, root, s.focus ? s.focus(root) : null, `${s.key}-${c.tag}`);
  if (s.kbd && (c.tag === '1024-lounge-dark' || c.tag === '1024-shipped')) {
    try {
      fig.keyboard = await keyboard(page, root, s.kbd, tag, requests, problems);
    } catch (e) {
      problems.push(`${tag}: keyboard step failed: ${String(e).split('\n')[0]!.slice(0, 240)}`);
    }
  }
  if (log.length) say(`${tag}: ${log.join('; ')}`);
  return fig;
}

// ------------------------------------------------------------------------------------------------ compare

function compare(label: string, base: Figures, after: Figures, touch: boolean, problems: string[]) {
  const diffs: Record<string, unknown> = {};
  if (base.selects.length !== after.selects.length) {
    problems.push(`${label}: select count ${base.selects.length} -> ${after.selects.length}`);
    return diffs;
  }
  base.selects.forEach((b, i) => {
    const a = after.selects[i]!;
    diffs[b.key] = {
      base: b.box,
      after: a.box,
      wrapper: a.wrapper,
      labelBase: b.label,
      labelAfter: a.label,
    };
    if (a.box.h < 40 - 0.5) problems.push(`${label}: ${b.key} is ${a.box.h} px tall (under 40)`);
    if (touch && a.box.h < 44 - 0.5)
      problems.push(`${label}: ${b.key} is ${a.box.h} px tall at 375 touch (under 44)`);
    if (a.box.h < b.box.h - 0.5)
      problems.push(`${label}: ${b.key} shorter than base (${a.box.h} < ${b.box.h})`);
    if (a.box.w < b.box.w - 0.5)
      problems.push(`${label}: ${b.key} narrower than base (${a.box.w} < ${b.box.w})`);
    if (b.label && a.label) {
      const grew = a.label.h - b.label.h;
      const selGrew = a.box.h - b.box.h;
      if (grew > Math.max(0, selGrew) + 2)
        problems.push(
          `${label}: ${b.key} label is taller by ${grew.toFixed(1)} px, the select by ${selGrew.toFixed(1)} px (the label now wraps?)`,
        );
    }
  });
  if (base.root && after.root)
    diffs['root'] = { base: base.root, after: after.root, dh: after.root.h - base.root.h };
  if (base.controls.length !== after.controls.length) {
    problems.push(`${label}: control count ${base.controls.length} -> ${after.controls.length}`);
  } else {
    const moved: { key: string; dx: number; dy: number; allowed: number }[] = [];
    base.controls.forEach((bc, i) => {
      const ac = after.controls[i]!;
      let allowed = 0;
      base.selects.forEach((b, j) => {
        const a = after.selects[j]!;
        if (b.box.y + b.box.h <= bc.box.y + 0.5) allowed += a.box.h - b.box.h;
        else if (b.box.y < bc.box.y + bc.box.h && bc.box.y < b.box.y + b.box.h)
          allowed = Math.max(allowed, (a.box.h - b.box.h) / 2 + 1);
      });
      const rootDy = base.root && after.root ? after.root.y - base.root.y : 0;
      const dy = Math.round((ac.box.y - bc.box.y - rootDy) * 10) / 10;
      const dx = Math.round((ac.box.x - bc.box.x) * 10) / 10;
      moved.push({ key: bc.key, dx, dy, allowed: Math.round(allowed * 10) / 10 });
      if (dy > allowed + 1) problems.push(`${label}: ${bc.key} moved ${dy} px, allowed ${allowed}`);
      if (dy < -1 - Math.max(0, base.root && after.root ? (after.root.h - base.root.h) / 2 : 0))
        problems.push(`${label}: ${bc.key} moved up ${dy} px`);
    });
    diffs['moved'] = moved.filter((x) => x.dx !== 0 || x.dy !== 0 || x.allowed !== 0);
  }
  const baseFail = new Set(base.hitFails.map((f) => f.control));
  const worse = after.hitFails.filter((f) => !baseFail.has(f.control));
  if (
    after.hit.fail > base.hit.fail ||
    after.hit.notReached > 0 ||
    after.hit.pass + after.hit.fail !== base.hit.pass + base.hit.fail ||
    worse.length
  )
    problems.push(
      `${label}: hit test ${JSON.stringify(base.hit)} -> ${JSON.stringify(after.hit)}; worse rows ${JSON.stringify(worse)}`,
    );
  return diffs;
}

// ------------------------------------------------------------------------------------------------ the test

test(`p102 media: 15 selects of the watch party, music, Study, Live TV, Stack skin and the Lounge, ${PHASE}`, async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const problems: string[] = [];
  const notes: string[] = [];
  const reached: Record<string, string> = {};
  const figs: Record<string, Figures> = {};
  const ids = await makeData(owner.request, notes);
  try {
    for (const c of CONFIGS) {
      const list = SCENARIOS.filter(
        (s) => s.family === c.family && (ALL || s.core) && (!ONLY || ONLY.test(s.key)),
      );
      if (!list.length) continue;
      await setPrefs(owner, c);
      const ctx = await newContext(browser, owner, c);
      const pages = new Map<string, Page>();
      const requests: string[] = [];
      try {
        for (const s of list) {
          let page = pages.get(s.group);
          const first = !page;
          if (!page) {
            page = await ctx.newPage();
            page.setDefaultTimeout(15_000);
            page.on('request', (r) => {
              if (r.method() === 'POST' && /\/music\/playlists\/[^/]+\/items/.test(r.url()))
                requests.push(r.url());
              if (r.method() === 'PATCH' && /\/users\/@me\/settings/.test(r.url()))
                requests.push(r.url());
              if (r.method() === 'PATCH' && /\/watch-parties\/[^/]+$/.test(r.url()))
                requests.push(r.url());
              if (r.method() === 'POST' && /\/watch-parties\/[^/]+\/playback/.test(r.url()))
                requests.push(r.url());
            });
            pages.set(s.group, page);
          }
          try {
            const f = await runScenario(s, c, page, ids, first, problems, reached, requests);
            if (f) figs[`${s.key}/${c.tag}`] = f;
          } catch (e) {
            problems.push(`${s.key} ${c.tag}: ${String(e).split('\n')[0]!.slice(0, 400)}`);
          }
        }
      } finally {
        await ctx.close();
      }
    }
  } finally {
    await removeData(owner.request, ids).catch(() => undefined);
  }
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  writeFileSync(resolve(OUT, `reach-${PHASE}.json`), JSON.stringify({ reached, notes }, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Figures>;
    const cmp: Record<string, unknown> = {};
    for (const k of Object.keys(figs)) {
      if (base[k]) {
        cmp[k] = compare(k, base[k]!, figs[k]!, k.includes('375'), problems);
        await composite(browser, k.replace('/', '-')).catch(() => undefined);
      } else problems.push(`${k}: reached in the after phase only`);
    }
    for (const k of Object.keys(base))
      if (!figs[k] && !(ONLY && !ONLY.test(k)))
        problems.push(`${k}: reached in the base phase only`);
    writeFileSync(resolve(OUT, 'compare.json'), JSON.stringify(cmp, null, 1));
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  await owner.close();
  say(`reach ${JSON.stringify(reached)}`);
  say(`problems: ${JSON.stringify(problems)}`);
  expect(problems).toEqual([]);
});
