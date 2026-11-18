import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type {
  APIRequestContext,
  Browser,
  BrowserContext,
  Locator,
  Page,
  Route,
} from '@playwright/test';
import { repoRoot, runCli } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, MENU_ITEM_SELECTOR, openCollapsed } from '../support/hit-test';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * Wave 106, census builder touchb (D-1004; the 44 px census): every control that fails the touch hit test at 375 x 812 in
 * the Library, the Cinema, Live TV, the Study and the Arcade. MEASURE ONLY: no app file is changed, every shoot is
 * `shoot base`. For each page: go there as a member does, wait until settled, `openCollapsed`, `hitTestAll` over the
 * page's main region with the default reach (D-856: points 21 px from the centre), then open each thing a member opens
 * with one tap (a filter drawer, a form, a dialog) and test that too. Every row is written whole to
 * `figures-part<n>.json` (page, state, name, role or tag, box, status, takers, the helper's path string), and one
 * viewport shot per page (`<page>-375.png`) is written next to it. The first page of each section also measures the
 * section's chrome (the header and nav outside `main`) as state "chrome".
 *
 * Parts (so that each job runs under 6 minutes): 1 Library, 2 Cinema, 3 Live TV + the Study + the Arcade, 4 the Browse
 * pages, the guide and the Arcade lobby again with every section on `lounge-light`.
 * Themes: the account's main theme is `lounge-light`; sections wear their shipped theme (`sectionThemes`: library and
 * cinema `sideroom`, live-tv `sideroom-broadcast`, arcade `lounge-editorial`), set through `PATCH /users/@me/settings`
 * and read back; `data-theme` of the page is read back on every page and written to the figures.
 * It never seeds. What the world lacks it makes through the API under `p106-tb-<stamp>` names (D-942): one published
 * series (two chapters) and one list for the Library pages, removed in a `finally`. The Study's shelf and files are faked
 * in the browser (no object storage here). The storefront switches are put on for the run and off again.
 *
 * SR_P106_TOUCHB_ON=1; SR_P106_TOUCHB_PART=1|2|3|4; SR_P106_TOUCHB_OUT=<folder>. The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p106-integ/apps/web/dist --env SR_P106_TOUCHB_ON=1 --env SR_P106_TOUCHB_PART=1 --env SR_P106_TOUCHB_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-touchb -- tests/p106-touchb-shots.spec.ts
 */
const ON = process.env.SR_P106_TOUCHB_ON === '1';
const PART = Number(process.env.SR_P106_TOUCHB_PART ?? '1');
const OUT =
  process.env.SR_P106_TOUCHB_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-touchb';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const API = '/api/v1';
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PIXEL =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';

test.skip(!ON, 'local visual check only');
test.setTimeout(9 * 60_000);

interface World {
  titleId: string;
  partyId: string | null;
  tvChannelId: string | null;
}
const say = (s: string) => console.log(`REPORT ${s}`);
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

interface Ids {
  seriesId: string;
  chapterId: string;
  listId: string;
}

interface Row {
  page: string;
  state: string;
  reach: string;
  name: string;
  tag: string;
  role: string | null;
  type: string | null;
  box: { x: number; y: number; w: number; h: number } | null;
  status: string;
  reason: string | null;
  offset: number | null;
  takers: { at: string; tag: string; role: string | null; name: string; path: string }[];
  path: string;
}
interface Cov {
  page: string;
  state: string;
  note: string;
}
const rows: Row[] = [];
const notCovered: Cov[] = [];
const pageInfo: Record<string, unknown> = {};
const problems: string[] = [];

async function call<T = any>(
  request: APIRequestContext,
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE',
  path: string,
  data?: unknown,
  retried = false,
): Promise<T> {
  const headers: Record<string, string> = { Origin: ORIGIN };
  if (method !== 'GET') {
    const csrf = await request.get(`${API}/sr/csrf`, { headers: { Origin: ORIGIN } });
    headers['X-SR-CSRF'] = ((await csrf.json()) as { token: string }).token;
  }
  const res = await request.fetch(`${API}${path}`, {
    method,
    headers,
    ...(data !== undefined ? { data } : {}),
  });
  if (res.status() === 429 && !retried) {
    const wait = Math.min(15, Number(res.headers()['retry-after'] ?? '5'));
    await new Promise((r) => setTimeout(r, wait * 1000));
    return call<T>(request, method, path, data, true);
  }
  if (!res.ok()) throw new Error(`${method} ${path} -> ${res.status()}: ${await res.text()}`);
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

const sections = (light: boolean) =>
  light
    ? {
        library: 'lounge-light',
        cinema: 'lounge-light',
        'live-tv': 'lounge-light',
        arcade: 'lounge-light',
      }
    : {
        library: 'sideroom',
        cinema: 'sideroom',
        'live-tv': 'sideroom-broadcast',
        arcade: 'lounge-editorial',
      };

async function setPrefs(ctx: BrowserContext, light: boolean) {
  const sec = sections(light);
  await apiPatch(ctx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-light', zoom: 100, sectionThemes: sec } },
  });
  const back = (await (await ctx.request.get(`${API}/users/@me/settings`)).json()) as {
    settings?: { user_prefs?: { theme?: string; sectionThemes?: Record<string, string> } };
  };
  expect(back.settings?.user_prefs?.theme).toBe('lounge-light');
  expect(back.settings?.user_prefs?.sectionThemes?.['arcade']).toBe(sec.arcade);
  expect(back.settings?.user_prefs?.sectionThemes?.['library']).toBe(sec.library);
}

async function newPage(browser: Browser, owner: BrowserContext, light: boolean) {
  const sec = sections(light);
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: 375, height: 812 },
    storageState: await owner.storageState(),
    bypassCSP: true,
    userAgent: PHONE_UA,
    isMobile: true,
    hasTouch: true,
  });
  await ctx.addInitScript(
    (p) => {
      try {
        localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
      } catch {
        /* no storage */
      }
    },
    { theme: 'lounge-light', zoom: 100, sectionThemes: sec },
  );
  await ctx.route('**/cdn/p106-*.png', (r) =>
    r.fulfill({ status: 200, contentType: 'image/png', body: Buffer.from(PIXEL, 'base64') }),
  );
  await ctx.route(/\/api\/v1\/library\/chapters\/\d+\/pages(\?.*)?$/, async (r) => {
    const res = await r.fetch();
    if (!res.ok()) return r.fulfill({ response: res });
    const body = (await res.json()) as { pages: { url: string }[] };
    body.pages.forEach((p, i) => (p.url = `${ORIGIN}/cdn/p106-p${i}.png`));
    return r.fulfill({ response: res, json: body });
  });
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  return { ctx, page };
}

const json = (r: Route, body: unknown) =>
  r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
const FILES = [
  { key: 'Films/heat.mp4', size: 1000 },
  { key: 'Music/blue.mp3', size: 2000 },
  { key: 'Covers/one.png', size: 3000 },
  { key: 'Covers/two.png', size: 4000 },
];
async function fakeStudy(page: Page) {
  await page.route(`**${API}/shelves/visible`, (r) =>
    json(r, { shelves: [{ shelf_id: '9501', label: 'p106 Home', via: 'own', object_count: 4 }] }),
  );
  await page.route(`**${API}/storage/mounts/*/objects*`, (r) =>
    json(r, {
      folders: [],
      objects: FILES.map((f) => ({ ...f, last_modified: '2026-10-01T00:00:00Z' })),
      next_cursor: null,
      can_upload: false,
      can_manage: true,
    }),
  );
  await page.route(/\/api\/v1\/shelves\/\d+\/objects(\?.*)?$/, (r) =>
    json(r, {
      objects: FILES.map((f) => ({
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

const settle = async (page: Page, ms = 3500) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(800);
};
const top = (page: Page) =>
  page
    .evaluate(() => {
      window.scrollTo(0, 0);
      for (const e of Array.from(document.querySelectorAll<HTMLElement>('body *')))
        if (e.scrollTop > 0) e.scrollTop = 0;
    })
    .catch(() => undefined);

const mainOf = (page: Page) => page.locator('main').first();

async function themeOf(page: Page) {
  return page.evaluate(() => {
    const m = document.querySelector('main') ?? document.body;
    return (
      m.closest('[data-theme]')?.getAttribute('data-theme') ??
      document.querySelector('[data-theme]')?.getAttribute('data-theme') ??
      null
    );
  });
}

/** Run the helper over one root and keep every row whole. */
async function record(
  page: Page,
  root: Locator,
  pageKey: string,
  state: string,
  opts: { menu?: boolean; reach?: 'touch' | 'box' } = {},
) {
  if ((await root.count()) === 0) {
    notCovered.push({ page: pageKey, state, note: 'the root was not found' });
    return;
  }
  const opened = await openCollapsed(page, root);
  const res = await hitTestAll(page, root, {
    ...(opts.menu ? { also: MENU_ITEM_SELECTOR } : {}),
    ...(opts.reach ? { reach: opts.reach } : {}),
  });
  let nr = 0;
  for (const r of res) {
    if (r.status === 'not-reached') nr++;
    rows.push({
      page: pageKey,
      state,
      reach: opts.reach ?? 'touch',
      name: r.name,
      tag: r.tag,
      role: r.role,
      type: r.type,
      box: r.result
        ? {
            x: Math.round(r.result.box.x),
            y: Math.round(r.result.box.y),
            w: Math.round(r.result.box.width * 10) / 10,
            h: Math.round(r.result.box.height * 10) / 10,
          }
        : null,
      status: r.status,
      reason: r.reason ?? null,
      offset: r.result?.offset ?? null,
      takers: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map((p) => ({
          at: p.at,
          tag: p.taker?.tag ?? 'none',
          role: p.taker?.role ?? null,
          name: p.taker?.name ?? '',
          path: p.taker?.path ?? '',
        })),
      path: r.path,
    });
  }
  const c = (s: string) => res.filter((r) => r.status === s).length;
  say(
    `${pageKey} [${state}]: ${res.length} controls, pass ${c('pass')}, fail ${c('fail')}, not-reached ${nr}, skipped ${c('skipped')}, opened ${opened.length}`,
  );
  for (const r of res.filter((x) => x.status === 'fail' || x.status === 'not-reached'))
    say(
      `  ${r.status} ${r.tag}${r.role ? `[${r.role}]` : ''} "${r.name}" ${r.result ? `${Math.round(r.result.box.width)}x${Math.round(r.result.box.height)}` : (r.reason ?? '')} ${r.path}`,
    );
  if (nr) problems.push(`${pageKey} [${state}]: ${nr} not-reached rows (a probe error)`);
}

async function shot(page: Page, pageKey: string) {
  mkdirSync(OUT, { recursive: true });
  await top(page);
  const sw = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  const theme = await themeOf(page);
  pageInfo[pageKey] = { scrollWidth: sw[0], clientWidth: sw[1], dataTheme: theme };
  say(`${pageKey}: scrollWidth/clientWidth ${sw.join('/')}, data-theme ${theme}`);
  await page.screenshot({ path: resolve(OUT, `${pageKey}-375.png`), timeout: 10_000 });
}

interface Overlay {
  key: string;
  /** Opens it (the page is open); returns a root, or null when it could not be opened. */
  open: (page: Page) => Promise<Locator | null>;
  menu?: boolean;
}
interface PageDef {
  key: string;
  /** Goes there and waits until the page shows what it is for. Returns false when the world cannot show it. */
  open: (page: Page) => Promise<boolean | void>;
  root?: (page: Page) => Locator;
  /** A section's first page also measures the chrome outside `main`. */
  chrome?: boolean;
  overlays?: Overlay[];
  /** Run before each measure (the reader's bars). */
  before?: (page: Page) => Promise<void>;
}

const dialogRoot = async (page: Page, ms = 4000) => {
  const d = page.locator('[role="dialog"], [role="menu"], [role="listbox"]').last();
  await d.waitFor({ state: 'visible', timeout: ms }).catch(() => undefined);
  return (await d.isVisible().catch(() => false)) ? d : null;
};
const tapButton = (name: RegExp) => async (page: Page) => {
  const b = page.getByRole('button', { name }).first();
  if (!(await b.isVisible({ timeout: 6000 }).catch(() => false))) return null;
  await b.scrollIntoViewIfNeeded();
  await b.click();
  await page.waitForTimeout(700);
  return (await dialogRoot(page, 2500)) ?? mainOf(page);
};
const tapTab = (name: RegExp) => async (page: Page) => {
  const b = page.getByRole('tab', { name }).first();
  if (!(await b.isVisible({ timeout: 6000 }).catch(() => false))) return null;
  await b.click();
  await settle(page, 2500);
  return mainOf(page);
};

async function measurePage(browser: Browser, owner: BrowserContext, def: PageDef, light: boolean) {
  const key = `${def.key}${light ? '-light' : ''}`;
  const { ctx, page } = await newPage(browser, owner, light);
  try {
    await fakeStudy(page);
    const ok = await def.open(page);
    if (ok === false) {
      notCovered.push({ page: key, state: 'page', note: 'the seeded world cannot show this page' });
      return;
    }
    await settle(page);
    if (def.before) await def.before(page);
    await shot(page, key);
    const root = def.root ? def.root(page) : mainOf(page);
    const target = (await root.count()) > 0 ? root : page.locator('body');
    await record(page, target, key, 'page');
    if (def.chrome)
      // The section bar sits at the very top of the page: a point 21 px above its centre is off the page, so the touch
      // reach cannot be used there (not-reached). Its rows are measured with the box reach and say so.
      await record(
        page,
        page.locator('header:not(main header), nav:not(main nav)'),
        key,
        'chrome',
        { reach: 'box' },
      );
    for (const ov of def.overlays ?? []) {
      try {
        await def.open(page);
        await settle(page, 2500);
        if (def.before) await def.before(page);
        const r = await ov.open(page);
        if (!r) {
          notCovered.push({ page: key, state: ov.key, note: 'the opener was not found' });
          continue;
        }
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `${key}-${ov.key}-375.png`), timeout: 10_000 });
        await record(page, r, key, ov.key, { menu: ov.menu });
      } catch (e) {
        notCovered.push({ page: key, state: ov.key, note: `error: ${String(e).slice(0, 200)}` });
      }
    }
  } catch (e) {
    notCovered.push({ page: key, state: 'page', note: `error: ${String(e).slice(0, 300)}` });
    say(`${key}: ERROR ${String(e).slice(0, 300)}`);
  } finally {
    await ctx.close();
  }
}

/* ------------------------------------------------------------------------------------------------ definitions */

const go = (path: string, ready?: (p: Page) => Promise<unknown>) => async (page: Page) => {
  await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 30_000 });
  if (ready) await ready(page);
};

async function holdBars(page: Page) {
  await page.evaluate(() => {
    const w = window as unknown as { __holdBars?: number };
    if (w.__holdBars) return;
    const poke = () =>
      document
        .querySelector('header[aria-label]')
        ?.dispatchEvent(new MouseEvent('mousemove', { bubbles: true }));
    poke();
    w.__holdBars = window.setInterval(poke, 400);
  });
}

function libraryDefs(ids: Ids): PageDef[] {
  const series = `/library/s/${ids.seriesId}`;
  const hasSeries = ids.seriesId !== '';
  const hasChapter = ids.chapterId !== '';
  const hasList = ids.listId !== '';
  const none = async () => false as const;
  return [
    {
      key: 'lib-browse',
      chrome: true,
      open: go('/library/browse', (p) => p.waitForTimeout(1500)),
      overlays: [{ key: 'filters', open: tapButton(/^Filters/) }],
    },
    {
      key: 'lib-series',
      open: hasSeries
        ? go(series, (p) => p.getByRole('tab', { name: 'Reviews' }).first().waitFor())
        : none,
      overlays: [{ key: 'reviews-tab', open: tapTab(/^Reviews/) }],
    },
    {
      key: 'lib-reader',
      root: (p) => p.locator('body'),
      open: hasChapter
        ? go(`/library/r/${ids.chapterId}/1`, (p) =>
            p.locator('select').first().waitFor({ state: 'visible', timeout: 30_000 }),
          )
        : none,
      before: async (p) => {
        await holdBars(p);
        await p.waitForTimeout(600);
      },
    },
    { key: 'lib-me', open: go('/library/me', (p) => p.waitForTimeout(1500)) },
    {
      key: 'lib-lists',
      open: go('/library/lists', (p) => p.waitForTimeout(1500)),
      overlays: [{ key: 'new-list', open: tapButton(/New list/) }],
    },
    {
      key: 'lib-list',
      open: hasList ? go(`/library/lists/${ids.listId}`, (p) => p.waitForTimeout(1500)) : none,
      overlays: [{ key: 'edit-list', open: tapButton(/Edit list/) }],
    },
    { key: 'lib-rankings', open: go('/library/rankings', (p) => p.waitForTimeout(1500)) },
    {
      key: 'lib-review',
      open: hasSeries
        ? async (p: Page) => {
            await p.goto(series, { waitUntil: 'domcontentloaded', timeout: 30_000 });
            await p.getByRole('tab', { name: 'Reviews' }).first().click();
            await p
              .getByRole('button', { name: /Write a review|Edit your review/ })
              .first()
              .click();
            await p.getByLabel('Rating with this review').first().waitFor();
          }
        : none,
    },
  ];
}

function cinemaDefs(w: World): PageDef[] {
  return [
    {
      key: 'cin-browse',
      chrome: true,
      open: go('/cinema/movies', (p) => p.waitForTimeout(1500)),
      overlays: [{ key: 'filters', open: tapButton(/^Filters/) }],
    },
    {
      key: 'cin-title',
      open: go(`/cinema/t/${w.titleId}`, (p) => p.waitForTimeout(1500)),
      overlays: [
        { key: 'comments-tab', open: tapTab(/^Comments/) },
        { key: 'watch-together', open: tapButton(/Watch together/i) },
        { key: 'share', open: tapButton(/^Share/) },
      ],
    },
    {
      key: 'cin-player',
      root: (p) => p.locator('body'),
      // The seeded title has no video, so the page shows its "back to the title" empty state. The play target and the
      // playback grant are answered here (no media behind the grant) so the player draws its bars and control bar.
      open: async (p: Page) => {
        await p.route(
          //api/v1/cinema/titles/[^/]+/play(?.*)?$/, (r) =>
          json(r, {
            title_id: w.titleId,
            asset_id: 'p106asset',
            title: 'p106 census title',
            runtime_min: 100,
            age_restricted: false,
            markers: [],
          }),
        );
        await p.route(
          //api/v1/cinema/assets/[^/]+/playback(?.*)?$/, (r) =>
          json(r, {
            master_url: `${ORIGIN}/cdn/p106-master.m3u8`,
            expires_at: Math.floor(Date.now() / 1000) + 3600,
            refresh_at: Math.floor(Date.now() / 1000) + 1800,
            duration_secs: 6000,
            subtitles: [],
          }),
        );
        await p.goto(`/cinema/watch/${w.titleId}`, { waitUntil: 'domcontentloaded' });
        await p.waitForTimeout(3500);
      },
      before: async (p) => {
        await p.mouse.move(180, 400);
        await p.touchscreen.tap(180, 400).catch(() => undefined);
        await p.waitForTimeout(600);
      },
    },
    {
      key: 'cin-party',
      root: (p) => p.locator('body'),
      open: w.partyId
        ? go(`/cinema/party/${w.partyId}`, (p) => p.waitForTimeout(3000))
        : async () => false as const,
      overlays: [
        { key: 'people-tab', open: tapTab(/^People/) },
        { key: 'queue-tab', open: tapTab(/^Queue/) },
      ],
    },
  ];
}

function part3Defs(w: World): PageDef[] {
  return [
    {
      key: 'tv-guide',
      chrome: true,
      open: go('/cinema/tv', (p) => p.waitForTimeout(2500)),
      overlays: [
        {
          key: 'program',
          open: async (p) => {
            const b = p.locator('.tv-agenda button:not([aria-label]):not([aria-pressed])').first();
            if (!(await b.isVisible({ timeout: 5000 }).catch(() => false))) return null;
            await b.scrollIntoViewIfNeeded();
            await b.click();
            await p.waitForTimeout(700);
            return (await dialogRoot(p, 2500)) ?? mainOf(p);
          },
        },
      ],
    },
    {
      key: 'tv-watch',
      root: (p) => p.locator('body'),
      open: w.tvChannelId
        ? async (p: Page) => {
            await p.route(/\/api\/v1\/tv\/channels\/[^/]+\/tune/, (r) =>
              r.fulfill({ status: 503, contentType: 'application/json', body: '{}' }),
            );
            await p.goto(`/cinema/tv/watch/${w.tvChannelId}`, { waitUntil: 'domcontentloaded' });
            await p.waitForTimeout(3000);
          }
        : async () => false as const,
    },
    {
      key: 'study',
      chrome: true,
      open: go('/study', (p) =>
        p
          .getByRole('button', { name: /^Edit details of / })
          .first()
          .waitFor({ timeout: 40_000 }),
      ),
      overlays: [
        { key: 'edit-dialog', open: tapButton(/^Edit details of /) },
        { key: 'file-viewer', open: tapButton(/heat/i) },
      ],
    },
    {
      key: 'arcade-lobby',
      chrome: true,
      open: go('/games', (p) => p.locator('.arc-root').waitFor({ timeout: 30_000 })),
      overlays: [
        {
          key: 'play',
          open: async (p) => {
            const b = p.locator('.arc-actions button').first();
            if (!(await b.isVisible({ timeout: 6000 }).catch(() => false))) return null;
            await b.scrollIntoViewIfNeeded();
            await b.click();
            return dialogRoot(p, 6000);
          },
        },
      ],
    },
    {
      key: 'arcade-emulator',
      chrome: true,
      open: go('/games/emulator', (p) => p.waitForTimeout(2500)),
    },
  ];
}

const part4Defs = (): PageDef[] => [
  {
    key: 'lib-browse',
    chrome: true,
    open: go('/library/browse', (p) => p.waitForTimeout(1500)),
    overlays: [{ key: 'filters', open: tapButton(/^Filters/) }],
  },
  {
    key: 'cin-browse',
    chrome: true,
    open: go('/cinema/movies', (p) => p.waitForTimeout(1500)),
    overlays: [{ key: 'filters', open: tapButton(/^Filters/) }],
  },
  { key: 'tv-guide', chrome: true, open: go('/cinema/tv', (p) => p.waitForTimeout(2500)) },
  {
    key: 'arcade-lobby',
    chrome: true,
    open: go('/games', (p) => p.locator('.arc-root').waitFor({ timeout: 30_000 })),
  },
];

/* ------------------------------------------------------------------------------------------------ the test */

test(`p106 touchb: the 44 px census, part ${PART}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = owner.request;
  const w = world();
  const stamp = String(Date.now()).slice(-6);
  const ids: Ids = { seriesId: '', chapterId: '', listId: '' };
  let made = false;
  await setStudySurfacesWith(o, true).catch((e) =>
    say(`storefront switches: ${String(e).slice(0, 120)}`),
  );
  try {
    await setPrefs(owner, PART === 4);
    if (PART === 1) {
      // Data for the Library, through the API (staff appointed best effort; the sweep owner may already be staff).
      try {
        const me = await call<{ username?: string }>(o, 'GET', '/users/@me');
        if (me.username)
          runCli([
            'staff',
            'appoint',
            me.username,
            '--role',
            'Senior Staff',
            '--reason',
            'e2e p106 touchb census',
          ]);
      } catch (e) {
        say(`staff appoint: ${String(e).slice(0, 140)}`);
      }
      await call(o, 'POST', '/sr/age-gate', { over_18: true }).catch(() => undefined);
      try {
        const series = await call<{ id: string }>(o, 'POST', '/admin/library/series', {
          kind: 'manga',
          title: `p106-tb-${stamp}`,
          synopsis: 'A series made by the wave 106 touchb census spec.',
          genres: ['Drama'],
          authors: ['E2E Author'],
          reading_direction: 'ltr',
        });
        ids.seriesId = series.id;
        made = true;
        const edition = (
          await call<{ id: string }>(o, 'POST', `/admin/library/series/${ids.seriesId}/editions`, {
            language: 'en',
            label: 'Official',
            is_default: true,
          })
        ).id;
        for (const [n, title] of [
          [1, 'p106-tb first'],
          [2, 'p106-tb second chapter with a long title that is wider than the picker'],
        ] as const) {
          const ch = await call<{ id: string }>(
            o,
            'POST',
            `/admin/library/editions/${edition}/chapters`,
            { number: n, title, format: 'images' },
          );
          await call(
            o,
            'PUT',
            `/admin/library/chapters/${ch.id}/pages`,
            [0, 1, 2].map((i) => ({
              key: `library/${ids.seriesId}/${ch.id}/00${i + 1}.webp`,
              width: 200,
              height: 300,
              variants: {},
            })),
          );
          await call(o, 'POST', `/admin/library/chapters/${ch.id}/publish`);
          if (!ids.chapterId) ids.chapterId = ch.id;
        }
        await call(o, 'POST', `/admin/library/series/${ids.seriesId}/publish`);
        const list = await call<{ id: string }>(o, 'POST', '/library/lists', {
          name: `p106-tb-list-${stamp}`,
          visibility: 'private',
        });
        ids.listId = list.id;
        say(`ids ${JSON.stringify(ids)}`);
      } catch (e) {
        say(`library data not made: ${String(e).slice(0, 300)}`);
        if (!made) {
          // fall back to a series the world already has
          try {
            const pg = await call<{ items?: { id: string }[] }>(
              o,
              'GET',
              '/library/series?limit=1',
            );
            const first = pg.items?.[0]?.id;
            if (first) ids.seriesId = first;
          } catch {
            /* none */
          }
        }
      }
    }

    let defs: PageDef[] = [];
    if (PART === 1) defs = libraryDefs(ids);
    if (PART === 2) defs = cinemaDefs(w);
    if (PART === 3) defs = part3Defs(w);
    if (PART === 4) defs = part4Defs();
    for (const d of defs) await measurePage(browser, owner, d, PART === 4);
  } finally {
    if (ids.listId)
      await call(o, 'DELETE', `/library/lists/${ids.listId}`).catch((e) =>
        say(`list delete: ${String(e).slice(0, 160)}`),
      );
    if (made && ids.seriesId) {
      await call(o, 'POST', `/admin/library/series/${ids.seriesId}/unpublish`).catch(
        () => undefined,
      );
      await call(o, 'DELETE', `/admin/library/series/${ids.seriesId}`).catch((e) =>
        say(`series delete: ${String(e).slice(0, 160)}`),
      );
    }
    await setStudySurfacesWith(o, false).catch(() => undefined);
    writeFileSync(
      resolve(OUT, `figures-part${PART}.json`),
      JSON.stringify({ part: PART, pageInfo, notCovered, rows }, null, 1),
    );
    await owner.close();
  }
  say(`notCovered: ${JSON.stringify(notCovered)}`);
  say(`problems: ${JSON.stringify(problems)}`);
  expect(problems).toEqual([]);
});
