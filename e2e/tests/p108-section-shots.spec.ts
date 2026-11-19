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
import { hitTest, hitTestAll, MENU_ITEM_SELECTOR, openCollapsed } from '../support/hit-test';
import { over, parseColor, ratio, type Rgba } from '../support/contrast';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * Wave 108, shots builder sectionshots (D-1022 the 44 px target; pod sections: chips, library, cinema).
 * Built from wave 107's spec p107-section-shots.spec.ts, with the Library and Cinema reads of p107-staff-shots.spec.ts
 * (the same chips on the pages staffshots measured). Two phases (SR_P108_SECTION_PHASE=base|after), both built mode.
 * Per page:
 *  - at 375 x 812 touch: openCollapsed + hitTestAll (default reach) over the census roots and overlays, every row whole;
 *    the box of every control the three builders name (and of the row each sits in) into the figures;
 *  - chip diagnostics (figures "chipDiag"): for every FilterChips chip, dropdown trigger and Drawer close: the drawn box,
 *    its border widths, its ::after (position, top, bottom, height, transform), and each ancestor whose overflow is not
 *    visible, so a fail names its cause (border, a section rule, clipping);
 *  - at 1024 x 768 (pointer): the same boxes (after must equal base), and the hit test with reach "box" on each control;
 *  - the field under cure B kept from wave 107 (.lib-search-input on Library Browse), at 1024 in three themes;
 *  - scrollWidth against clientWidth on every page.
 * After compares with figures-base-part<N>.json (when it is in OUT) and writes every broken limit as a problem line.
 * Parts: 1 Library (Browse with its filters drawer, sort ascending, an empty library, My library, series, reader, list,
 * review); 2 the Study (with an empty folder) and Cinema (Browse with filters and the search dialog, title, player,
 * party). Files: figures-<phase>-part<N>.json, shots <page>-375.png / <page>-1024-<theme>.png (base twins: base-).
 * It never seeds. What the world lacks it makes through the API under p108-sec-<stamp> names (D-942) and removes in a
 * finally. Study storage and the empty library are faked in the browser.
 *
 * SR_P108_SECTION_ON=1; SR_P108_SECTION_PHASE=base|after; SR_P108_SECTION_PART=1|2; SR_P108_SECTION_OUT=<folder>.
 * The commander's command, whole (part 1; the same for 2):
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p108-integ/apps/web/dist --env SR_P108_SECTION_ON=1 --env SR_P108_SECTION_PHASE=after --env SR_P108_SECTION_PART=1 --env SR_P108_SECTION_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-sectionshots -- tests/p108-section-shots.spec.ts
 */
const ON = process.env.SR_P108_SECTION_ON === '1';
const PHASE = process.env.SR_P108_SECTION_PHASE === 'after' ? 'after' : 'base';
const PART = Number(process.env.SR_P108_SECTION_PART ?? '1');
const OUT =
  process.env.SR_P108_SECTION_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-sectionshots';
const PFX = PHASE === 'base' ? 'base-' : '';
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
interface Named {
  page: string;
  state: string;
  size: number;
  sel: string;
  i: number;
  name: string;
  box: { x: number; y: number; w: number; h: number };
  parent: { tag: string; cls: string; h: number; w: number };
  hit?: { status: string; takers: string };
}
interface FieldFig {
  page: string;
  mode: string;
  sel: string;
  box: { w: number; h: number };
  dataTheme: string | null;
  rest: string;
  hover: string;
  focus: { style: string; width: string; color: string; offset: string };
  placeholder: { color: string; opacity: string; attr: string };
  want: { rest: string; faint: string; focus: string };
  ground: string;
  outside: string;
  ratios: { restOutside: number; restGround: number; ring: number; placeholder: number };
}
const named: Named[] = [];
const fieldFigs: FieldFig[] = [];

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

type Mode = 'shipped' | 'light' | 'dark';
const mainTheme = (mode: Mode) => (mode === 'dark' ? 'lounge-dark' : 'lounge-light');
const sections = (mode: Mode) =>
  mode === 'shipped'
    ? {
        library: 'sideroom',
        cinema: 'sideroom',
        'live-tv': 'sideroom-broadcast',
        arcade: 'lounge-editorial',
      }
    : {
        library: mainTheme(mode),
        cinema: mainTheme(mode),
        'live-tv': mainTheme(mode),
        arcade: mainTheme(mode),
      };

async function setPrefs(ctx: BrowserContext, mode: Mode) {
  const sec = sections(mode);
  await apiPatch(ctx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: mainTheme(mode), zoom: 100, sectionThemes: sec } },
  });
  const back = (await (await ctx.request.get(`${API}/users/@me/settings`)).json()) as {
    settings?: { user_prefs?: { theme?: string; sectionThemes?: Record<string, string> } };
  };
  expect(back.settings?.user_prefs?.theme).toBe(mainTheme(mode));
  expect(back.settings?.user_prefs?.sectionThemes?.['arcade']).toBe(sec.arcade);
  expect(back.settings?.user_prefs?.sectionThemes?.['library']).toBe(sec.library);
}

type Size = 'phone' | 'desk';
async function newPage(browser: Browser, owner: BrowserContext, mode: Mode, size: Size = 'phone') {
  const sec = sections(mode);
  const phone = size === 'phone';
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: phone ? { width: 375, height: 812 } : { width: 1024, height: 768 },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript(
    (p) => {
      try {
        localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
      } catch {
        /* no storage */
      }
    },
    { theme: mainTheme(mode), zoom: 100, sectionThemes: sec },
  );
  await ctx.route('**/cdn/p108-*.png', (r) =>
    r.fulfill({ status: 200, contentType: 'image/png', body: Buffer.from(PIXEL, 'base64') }),
  );
  await ctx.route(/\/api\/v1\/library\/chapters\/\d+\/pages(\?.*)?$/, async (r) => {
    const res = await r.fetch();
    if (!res.ok()) return r.fulfill({ response: res });
    const body = (await res.json()) as { pages: { url: string }[] };
    body.pages.forEach((p, i) => (p.url = `${ORIGIN}/cdn/p108-p${i}.png`));
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
    json(r, { shelves: [{ shelf_id: '9501', label: 'p108 Home', via: 'own', object_count: 4 }] }),
  );
  await page.route(`**${API}/storage/mounts/*/objects*`, (r) =>
    json(r, {
      folders: ['Docs/', 'Notes/'],
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
  if (sw[0]! > sw[1]!) problems.push(`${pageKey}: the page scrolls sideways (${sw[0]} > ${sw[1]})`);
  await page.screenshot({ path: resolve(OUT, `${PFX}${pageKey}-375.png`), timeout: 10_000 });
}

/** Selectors that are rows or lists (boxes read, never hit-tested one by one). */
const ROW_SELS = new Set([
  '.stu-rail',
  '.stu-crumbs',
  '.tv-chips',
  '.tv-nav',
  '.wp-controls',
  '.cin-post-actions',
  '.arc-actions',
  '.lib-actions',
  '.lib-chip-list',
  '.lib-genre-chips',
]);

/** The box of every named control and row on the page now (document coordinates), and of the row each sits in. */
async function measureNamed(page: Page, key: string, state: string, size: number, sels: string[]) {
  for (const sel of sels) {
    const found = await page
      .locator(sel)
      .evaluateAll((els) =>
        els
          .filter((e) => e.getClientRects().length > 0)
          .slice(0, 14)
          .map((e, i) => {
            const r = e.getBoundingClientRect();
            const p = e.parentElement!;
            const pr = p.getBoundingClientRect();
            const cls = typeof p.className === 'string' ? (p.className.split(/\s+/)[0] ?? '') : '';
            const rd = (n: number) => Math.round(n * 10) / 10;
            return {
              i,
              name: (
                e.getAttribute('aria-label') ||
                e.getAttribute('placeholder') ||
                e.textContent ||
                ''
              )
                .trim()
                .replace(/\s+/g, ' ')
                .slice(0, 30),
              box: {
                x: rd(r.left + window.scrollX),
                y: rd(r.top + window.scrollY),
                w: rd(r.width),
                h: rd(r.height),
              },
              parent: { tag: p.tagName.toLowerCase(), cls, h: rd(pr.height), w: rd(pr.width) },
            };
          }),
      )
      .catch(() => []);
    for (const f of found) named.push({ page: key, state, size, sel, ...f });
    if (found.length === 0) notCovered.push({ page: key, state, note: `no "${sel}" shown` });
  }
}

/** At pointer widths: the hit test with reach "box" on each named control (not rows). */
async function hitNamed(page: Page, key: string, state: string, sels: string[]) {
  for (const sel of sels) {
    if (ROW_SELS.has(sel)) continue;
    const loc = page.locator(sel);
    const n = Math.min(await loc.count(), 14);
    let k = 0;
    for (let j = 0; j < n; j++) {
      const l = loc.nth(j);
      if (!(await l.evaluate((e) => e.getClientRects().length > 0).catch(() => false))) continue;
      const rec = named.find(
        (x) => x.page === key && x.state === state && x.size === 1024 && x.sel === sel && x.i === k,
      );
      k++;
      try {
        const res = await hitTest(page, l, { reach: 'box' });
        if (rec)
          rec.hit = {
            status: res.pass ? 'pass' : 'fail',
            takers: res.points
              .filter((p) => !p.ok)
              .map((p) => `${p.at}:${p.taker?.tag ?? 'none'}`)
              .join(','),
          };
      } catch (e) {
        if (rec) rec.hit = { status: 'not-reached', takers: String(e).slice(0, 60) };
      }
    }
  }
}

const hexOf = (c: Rgba) =>
  '#' +
  [c.r, c.g, c.b].map((v) => Math.round(v).toString(16).padStart(2, '0')).join('') +
  (c.a < 1 ? `/${Math.round(c.a * 100) / 100}` : '');
const r2 = (n: number) => Math.round(n * 100) / 100;

/** The field's boundary at rest, on hover and with keyboard focus; its placeholder; against the kit field's manners (D-1024). */
async function readField(page: Page, sel: string, key: string, mode: Mode, focusShot: boolean) {
  const loc = page.locator(sel).first();
  if (!(await loc.isVisible({ timeout: 5000 }).catch(() => false))) {
    notCovered.push({ page: key, state: `field ${sel} ${mode}`, note: 'the field is not shown' });
    return;
  }
  await loc.scrollIntoViewIfNeeded();
  await loc.evaluate((e) => (e as HTMLElement).blur());
  await page.mouse.move(2, 2);
  const read = () =>
    loc.evaluate((el) => {
      const cs = getComputedStyle(el);
      const ph = getComputedStyle(el, '::placeholder');
      const probe = (css: string) => {
        const sp = document.createElement('span');
        sp.style.color = css;
        sp.style.display = 'none';
        el.parentElement!.appendChild(sp);
        const c = getComputedStyle(sp).color;
        sp.remove();
        return c;
      };
      const stack: string[] = [];
      for (let a = el.parentElement; a; a = a.parentElement) {
        const c = getComputedStyle(a).backgroundColor;
        if (c !== 'rgba(0, 0, 0, 0)' && c !== 'transparent') {
          stack.push(c);
          if (/^rgb\(/.test(c) || /, 1\)$/.test(c)) break;
        }
      }
      const r = el.getBoundingClientRect();
      return {
        border: (() => {
          // the side that draws the boundary (an underline field draws only its bottom line)
          const sides = ['Top', 'Bottom', 'Left', 'Right'] as const;
          const sd = sides.find(
            (x) => cs[`border${x}Style`] !== 'none' && parseFloat(cs[`border${x}Width`]) > 0,
          );
          return cs[`border${sd ?? 'Bottom'}Color`];
        })(),
        bg: cs.backgroundColor,
        ol: {
          style: cs.outlineStyle,
          width: cs.outlineWidth,
          color: cs.outlineColor,
          offset: cs.outlineOffset,
        },
        ph: { color: ph.color, opacity: ph.opacity, attr: el.getAttribute('placeholder') ?? '' },
        stack,
        w: r.width,
        h: r.height,
        faint: probe('var(--ui-text-faint)'),
        focusColor: probe('var(--ui-focus)'),
        mix: probe('color-mix(in srgb, var(--ui-border), var(--ui-text-faint) 65%)'),
        theme: el.closest('[data-theme]')?.getAttribute('data-theme') ?? null,
      };
    });
  const rest = await read();
  await loc.hover({ timeout: 3000, force: true });
  const hov = await read();
  await page.mouse.move(2, 2);
  await page.keyboard.press('Shift');
  await loc.focus({ timeout: 2000 });
  const foc = await read();
  if (focusShot) {
    await top(page);
    await page.screenshot({
      path: resolve(OUT, `${PFX}${key}-1024-${mode}.png`),
      timeout: 10_000,
    });
  }
  await loc.blur();
  let outside: Rgba = { r: 255, g: 255, b: 255, a: 1 };
  for (const c of [...rest.stack].reverse()) outside = over(parseColor(c), outside);
  const ground = over(parseColor(rest.bg), outside);
  const borderVis = over(parseColor(rest.border), ground);
  const ringVis = over(parseColor(foc.ol.color), outside);
  fieldFigs.push({
    page: key,
    mode,
    sel,
    box: { w: r2(rest.w), h: r2(rest.h) },
    dataTheme: rest.theme,
    rest: rest.border,
    hover: hov.border,
    focus: foc.ol,
    placeholder: rest.ph,
    want: { rest: rest.mix, faint: rest.faint, focus: rest.focusColor },
    ground: hexOf(ground),
    outside: hexOf(outside),
    ratios: {
      restOutside: r2(ratio(borderVis, outside)),
      restGround: r2(ratio(borderVis, ground)),
      ring: foc.ol.style === 'none' ? 0 : r2(ratio(ringVis, outside)),
      placeholder: r2(ratio(over(parseColor(rest.ph.color), ground), ground)),
    },
  });
  say(`field ${key} ${sel} ${mode}: ${JSON.stringify(fieldFigs[fieldFigs.length - 1])}`);
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
  /** Selectors of the named controls and rows to measure, by state (`page` or an overlay key). */
  named?: Record<string, string[]>;
  /** Fields under cure B on this page (read at 1024 in three themes). */
  fields?: string[];
  /** Routes the page answers differently (an empty library, an empty folder), after the Study fakes. */
  mock?: (page: Page) => Promise<void>;
}

interface ChipDiag {
  page: string;
  state: string;
  kind: string;
  i: number;
  name: string;
  box: { x: number; y: number; w: number; h: number };
  border: string;
  after: {
    content: string;
    position: string;
    top: string;
    bottom: string;
    height: string;
    transform: string;
    pointerEvents: string;
  };
  /** Ancestors whose overflow is not visible (tag.class: overflow, its box). */
  clips: string[];
  /** The element's own and its ancestors' pointer-events/opacity that would matter, and the row it sits in. */
  row: string;
}
const chipDiag: ChipDiag[] = [];

/** Every FilterChips chip, dropdown trigger and Drawer close shown now: box, border, ::after, clipping ancestors. */
async function diagChips(page: Page, key: string, state: string) {
  const found = await page
    .evaluate(() => {
      const rd = (n: number) => Math.round(n * 10) / 10;
      const out: unknown[] = [];
      const clsOf = (e: Element) =>
        typeof (e as HTMLElement).className === 'string'
          ? ((e as HTMLElement).className.split(/\s+/)[0] ?? '')
          : '';
      const describe = (el: HTMLElement, kind: string, i: number) => {
        const r = el.getBoundingClientRect();
        const cs = getComputedStyle(el);
        const af = getComputedStyle(el, '::after');
        const clips: string[] = [];
        for (let a = el.parentElement; a && a !== document.documentElement; a = a.parentElement) {
          const ac = getComputedStyle(a);
          if (ac.overflowX !== 'visible' || ac.overflowY !== 'visible') {
            const b = a.getBoundingClientRect();
            clips.push(
              `${a.tagName.toLowerCase()}.${clsOf(a)}: overflow ${ac.overflowX}/${ac.overflowY} box ${rd(b.left)},${rd(b.top)} ${rd(b.width)}x${rd(b.height)}`,
            );
          }
        }
        const p = el.parentElement;
        const pr = p?.getBoundingClientRect();
        out.push({
          kind,
          i,
          name: (el.getAttribute('aria-label') || el.textContent || '')
            .trim()
            .replace(/\s+/g, ' ')
            .slice(0, 30),
          box: { x: rd(r.left + scrollX), y: rd(r.top + scrollY), w: rd(r.width), h: rd(r.height) },
          border: `${cs.borderTopWidth}/${cs.borderRightWidth}/${cs.borderBottomWidth}/${cs.borderLeftWidth}`,
          after: {
            content: af.content,
            position: af.position,
            top: af.top,
            bottom: af.bottom,
            height: af.height,
            transform: af.transform,
            pointerEvents: af.pointerEvents,
          },
          clips,
          row:
            p && pr
              ? `${p.tagName.toLowerCase()}.${clsOf(p)} ${rd(pr.width)}x${rd(pr.height)}`
              : '',
        });
      };
      const vis = (e: Element) => e.getClientRects().length > 0;
      let n = 0;
      document
        .querySelectorAll<HTMLElement>('[role="group"] > button[aria-pressed]')
        .forEach((e) => vis(e) && n++ < 14 && describe(e, 'chip', n));
      n = 0;
      document
        .querySelectorAll<HTMLElement>('[class*="dropdown"] button')
        .forEach((e) => vis(e) && n++ < 6 && describe(e, 'dropdown-button', n));
      n = 0;
      document
        .querySelectorAll<HTMLElement>('[role="dialog"] button[class*="close"]')
        .forEach((e) => vis(e) && n++ < 3 && describe(e, 'drawer-close', n));
      return out;
    })
    .catch(() => []);
  for (const f of found as Omit<ChipDiag, 'page' | 'state'>[])
    chipDiag.push({ page: key, state, ...f });
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
  return (await mainOf(page).count()) > 0 ? mainOf(page) : page.locator('body');
};

async function measurePage(browser: Browser, owner: BrowserContext, def: PageDef) {
  const key = def.key;
  const { ctx, page } = await newPage(browser, owner, 'shipped');
  try {
    await fakeStudy(page);
    if (def.mock) await def.mock(page);
    const ok = await def.open(page);
    if (ok === false) {
      notCovered.push({ page: key, state: 'page', note: 'the seeded world cannot show this page' });
      return;
    }
    await settle(page);
    if (def.before) await def.before(page);
    await shot(page, key);
    await measureNamed(page, key, 'page', 375, def.named?.['page'] ?? []);
    await diagChips(page, key, 'page');
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
        await page.screenshot({
          path: resolve(OUT, `${PFX}${key}-${ov.key}-375.png`),
          timeout: 10_000,
        });
        await measureNamed(page, key, ov.key, 375, def.named?.[ov.key] ?? []);
        await diagChips(page, key, ov.key);
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

/** 1024 x 768, pointer: the boxes of the named controls and rows (after must equal base), the box-reach hit test, the fields. */
async function measureDesk(
  browser: Browser,
  owner: BrowserContext,
  def: PageDef,
  mode: Mode,
  boxes: boolean,
) {
  const key = def.key;
  const { ctx, page } = await newPage(browser, owner, mode, 'desk');
  try {
    await fakeStudy(page);
    if (def.mock) await def.mock(page);
    const ok = await def.open(page);
    if (ok === false) return;
    await settle(page);
    if (def.before) await def.before(page);
    await top(page);
    const sw = await page.evaluate(() => [
      document.documentElement.scrollWidth,
      document.documentElement.clientWidth,
    ]);
    if (sw[0]! > sw[1]!) problems.push(`${key} 1024: scrolls sideways (${sw[0]} > ${sw[1]})`);
    if (boxes) {
      await measureNamed(page, key, 'page', 1024, def.named?.['page'] ?? []);
      await hitNamed(page, key, 'page', def.named?.['page'] ?? []);
    }
    for (const f of def.fields ?? []) await readField(page, f, key, mode, mode !== 'dark');
    if (boxes)
      for (const ov of def.overlays ?? []) {
        if (!def.named?.[ov.key]) continue;
        try {
          await def.open(page);
          await settle(page, 2500);
          const r = await ov.open(page);
          if (!r) continue;
          await page.waitForTimeout(500);
          await measureNamed(page, key, ov.key, 1024, def.named[ov.key] ?? []);
          await hitNamed(page, key, ov.key, def.named[ov.key] ?? []);
        } catch (e) {
          notCovered.push({
            page: key,
            state: `${ov.key} 1024`,
            note: `error: ${String(e).slice(0, 160)}`,
          });
        }
      }
  } catch (e) {
    notCovered.push({
      page: key,
      state: `1024 ${mode}`,
      note: `error: ${String(e).slice(0, 300)}`,
    });
    say(`${key} 1024 ${mode}: ERROR ${String(e).slice(0, 300)}`);
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
      named: {
        page: ['.lib-topbar', '.lib-browse-tools', '.lib-icon-btn', '.lib-search-input'],
        filters: ['.lib-genre-chips', '.lib-genre-chip'],
      },
      fields: ['.lib-search-input'],
    },
    {
      // The sort direction in its other state ("Sorted ascending").
      key: 'lib-browse-asc',
      named: { page: ['.lib-browse-tools', '.lib-icon-btn'] },
      open: go('/library/browse?order=asc', (p) => p.waitForTimeout(1500)),
    },
    {
      // An empty library: the shelf's empty state with its two links (the list endpoint answers no series).
      key: 'lib-browse-empty',
      named: { page: ['.shv-empty-links', '.shv-empty-links a'] },
      mock: async (p) => {
        await p.route(/\/api\/v1\/library\/series(\?.*)?$/, (r) =>
          json(r, { items: [], total: 0, limit: 24, offset: 0 }),
        );
      },
      open: go('/library/browse', (p) => p.waitForTimeout(1500)),
    },
    {
      key: 'lib-series',
      named: {
        page: ['.lib-actions', '.lib-btn', '.lib-chip-list', '.lib-chip-list a', '.lib-linklike'],
      },
      open: hasSeries
        ? go(series, (p) => p.getByRole('tab', { name: 'Reviews' }).first().waitFor())
        : none,
      overlays: [{ key: 'reviews-tab', open: tapTab(/^Reviews/) }],
    },
    {
      key: 'lib-reader',
      named: { page: ['.lib-icon-btn', '.lib-btn'] },
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
    {
      key: 'lib-me',
      named: { page: ['.lib-btn', '.lib-icon-btn'] },
      open: go('/library/me', (p) => p.waitForTimeout(1500)),
    },
    {
      key: 'lib-list',
      named: { page: ['.lib-btn', '.shv-empty-links a'] },
      open: hasList ? go(`/library/lists/${ids.listId}`, (p) => p.waitForTimeout(1500)) : none,
      overlays: [{ key: 'edit-list', open: tapButton(/Edit list/) }],
    },
    {
      key: 'lib-review',
      named: { page: ['.lib-btn', '.lib-chip-list a'] },
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

function part2Defs(w: World): PageDef[] {
  return [
    {
      key: 'study',
      named: {
        page: ['.stu-rail', '.stu-crumbs', '.stu-crumbs button', '.stu-folder', '.stu-open'],
      },
      chrome: true,
      open: go('/study', (p) =>
        p
          .getByRole('button', { name: /^Edit details of / })
          .first()
          .waitFor({ timeout: 40_000 }),
      ),
      overlays: [{ key: 'edit-dialog', open: tapButton(/^Edit details of /) }],
    },
    {
      // A folder with nothing in it: the Study's empty state.
      key: 'study-empty',
      named: { page: ['.stu-rail', '.stu-crumbs', '.stu-crumbs button'] },
      mock: async (p) => {
        await p.route(`**${API}/storage/mounts/*/objects*`, (r) =>
          json(r, {
            folders: [],
            objects: [],
            next_cursor: null,
            can_upload: false,
            can_manage: true,
          }),
        );
        await p.route(/\/api\/v1\/shelves\/\d+\/objects(\?.*)?$/, (r) =>
          json(r, { objects: [], next_cursor: null, can_edit: true }),
        );
      },
      open: go('/study', (p) => p.waitForTimeout(6000)),
    },
    {
      // The Cinema home: rows of cards with the quick actions.
      key: 'cin-home',
      chrome: true,
      named: { page: ['.cin-qa'] },
      open: go('/cinema', (p) => p.waitForTimeout(2500)),
    },
    {
      key: 'cin-browse',
      chrome: true,
      named: { page: ['.cin-qa'], search: ['.cin-link-btn'] },
      open: go('/cinema/movies', (p) => p.waitForTimeout(1500)),
      overlays: [
        { key: 'filters', open: tapButton(/^Filters/) },
        {
          // The search dialog with text typed, so "See all results" is enabled.
          key: 'search',
          open: async (p) => {
            const b = p.getByRole('button', { name: /^Search the cinema/ }).first();
            if (!(await b.isVisible({ timeout: 6000 }).catch(() => false))) return null;
            await b.click();
            const input = p.getByRole('combobox', { name: /Search the cinema/ }).first();
            await input.waitFor({ state: 'visible', timeout: 5000 });
            await input.fill('ab');
            await p.waitForTimeout(900);
            const dlg = p.getByRole('dialog').last();
            return (await dlg.isVisible().catch(() => false)) ? dlg : null;
          },
        },
      ],
    },
    {
      key: 'cin-title',
      named: { page: ['.cin-link-btn'], 'comments-tab': ['.cin-post-actions', '.cin-link-btn'] },
      open: go(`/cinema/t/${w.titleId}`, (p) => p.waitForTimeout(1500)),
      overlays: [
        { key: 'comments-tab', open: tapTab(/^Comments/) },
        { key: 'watch-together', open: tapButton(/Watch together/i) },
        { key: 'share', open: tapButton(/^Share/) },
      ],
    },
    {
      key: 'cin-player',
      named: { page: ['.cpl-btn'] },
      root: (p) => p.locator('body'),
      // The seeded title has no video, so the page shows its "back to the title" empty state. The play target and the
      // playback grant are answered here (no media behind the grant) so the player draws its bars and control bar.
      open: async (p: Page) => {
        await p.route(/\/api\/v1\/cinema\/titles\/[^/]+\/play(\?.*)?$/, (r) =>
          json(r, {
            title_id: w.titleId,
            asset_id: 'p107asset',
            title: 'p108 census title',
            runtime_min: 100,
            age_restricted: false,
            markers: [],
          }),
        );
        await p.route(/\/api\/v1\/cinema\/assets\/[^/]+\/playback(\?.*)?$/, (r) =>
          json(r, {
            master_url: `${ORIGIN}/cdn/p108-master.m3u8`,
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
      named: { page: ['.wp-controls', '.wp-btn', '.wp-vol'] },
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

/* ------------------------------------------------------------------------------------------------ the test */

test(`p108 sections: part ${PART} ${PHASE}`, async ({ browser }) => {
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
    await setPrefs(owner, 'shipped');
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
            'e2e p108 sectionshots',
          ]);
      } catch (e) {
        say(`staff appoint: ${String(e).slice(0, 140)}`);
      }
      await call(o, 'POST', '/sr/age-gate', { over_18: true }).catch(() => undefined);
      try {
        const series = await call<{ id: string }>(o, 'POST', '/admin/library/series', {
          kind: 'manga',
          title: `p108-sec-${stamp}`,
          synopsis: 'A series made by the wave 108 sectionshots census spec.',
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
          [1, 'p108-sec first'],
          [2, 'p108-sec second chapter with a long title that is wider than the picker'],
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
          name: `p108-sec-list-${stamp}`,
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
    if (PART === 2) defs = part2Defs(w);
    for (const d of defs) {
      await measurePage(browser, owner, d);
      await measureDesk(browser, owner, d, 'shipped', true);
    }
    for (const mode of ['light', 'dark'] as const) {
      if (!defs.some((d) => d.fields?.length)) break;
      await setPrefs(owner, mode);
      for (const d of defs) if (d.fields?.length) await measureDesk(browser, owner, d, mode, false);
    }
    await setPrefs(owner, 'shipped');
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
    compare();
    writeFileSync(
      resolve(OUT, `figures-${PHASE}-part${PART}.json`),
      JSON.stringify(
        {
          part: PART,
          phase: PHASE,
          pageInfo,
          notCovered,
          rows,
          named,
          chipDiag,
          fieldFigs,
          problems,
          notThisPod,
        },
        null,
        1,
      ),
    );
    await owner.close();
  }
  say(`notCovered: ${JSON.stringify(notCovered)}`);
  say(`problems: ${JSON.stringify(problems)}`);
  expect(problems).toEqual([]);
});

const notThisPod: string[] = [];

/** What a control or row may change on a phone (D-990), from the two task files and the commander's notes; default: nothing (0.5 px). */
const ALLOW: Record<string, { cw?: number; ch?: number; ph?: number; min44?: boolean }> = {
  // library (D-1022): the Study's file rows to 44 px on a phone and the list longer by that
  '.stu-folder': { ch: 44, ph: 44 },
  '.stu-open': { ch: 44, ph: 44 },
  '.lib-search-input': { ch: 4.5, ph: 4.5, min44: true },
  '.lib-topbar': { ch: 4.5 },
  '.lib-browse-tools': { ch: 4.5 },
  '.cin-qa': { cw: 14.5, ch: 14.5, ph: 14.5, min44: true },
  // cinema (D-1022): a text button grows with its line by 15 px at most
  '.cin-link-btn': { ch: 15.5, ph: 15.5 },
  '.cin-post-actions': { ch: 15.5 },
  '.wp-controls': { ch: 12 },
  '.wp-btn': { ph: 12 },
  '.wp-vol': { ch: 28.5, ph: 12 },
  '.cpl-btn': { ph: 4.5 },
  '.lib-chip-list': { ch: 0.5 },
  '.shv-empty-links a': { ch: 30, ph: 30 },
  '.shv-empty-links': { ch: 30 },
  '.lib-btn': { ph: 4.5 },
  '.lib-icon-btn': { ph: 4.5 },
};
const ROWS_UNCHECKED_PARENT = new Set([...ROW_SELS]);
const NAMED_RE =
  /(lib-genre-chip|lib-btn|lib-icon-btn|lib-chip-list|lib-linklike|shv-empty-links|lib-search-input|stu-rail|stu-crumbs|stu-folder|stu-open|cin-qa|cin-link-btn|cin-post-actions|wp-btn|wp-vol|wp-controls|cpl-btn)/;
/** The kit's chips, the chip dropdown's two buttons and the Drawer close (builder chips): the path has the stripped module class. */
const CHIP_RE = /\._(chip|chips|dropdown|select|close)( |$)/;
const CONTROL_TAGS = new Set(['button', 'a', 'input', 'select', 'textarea', 'label', 'summary']);

/** After phase: compare with figures-base-part<N>.json (when present) and write every broken limit as a problem line. */
function compare() {
  // Fields (cure B), in any phase they are read: the lines are problems only in the after phase.
  if (PHASE === 'after') {
    for (const f of fieldFigs) {
      const id = `${f.page} ${f.sel} [${f.mode}, data-theme ${f.dataTheme}]`;
      if (f.rest !== f.want.rest)
        problems.push(
          `${id}: rest border-color ${f.rest}, the contract wants color-mix 65 = ${f.want.rest}`,
        );
      if (f.hover !== f.want.faint)
        problems.push(`${id}: hover border ${f.hover}, wants --ui-text-faint ${f.want.faint}`);
      if (
        f.focus.style !== 'solid' ||
        f.focus.width !== '2px' ||
        f.focus.offset !== '1px' ||
        f.focus.color !== f.want.focus
      )
        problems.push(
          `${id}: focus-visible outline ${f.focus.style} ${f.focus.width} ${f.focus.color} offset ${f.focus.offset}, wants solid 2px ${f.want.focus} offset 1px`,
        );
      if (
        f.placeholder.attr &&
        (f.placeholder.color !== f.want.faint || f.placeholder.opacity !== '1')
      )
        problems.push(
          `${id}: placeholder ${f.placeholder.color} opacity ${f.placeholder.opacity}, wants ${f.want.faint} opacity 1`,
        );
      if (f.ratios.restOutside < 3 || f.ratios.restGround < 3)
        problems.push(
          `${id}: rest border ratio ${f.ratios.restOutside} outside / ${f.ratios.restGround} on the field (3.0 or more)`,
        );
      if (f.ratios.ring < 3) problems.push(`${id}: ring ratio ${f.ratios.ring} (3.0 or more)`);
      if (f.placeholder.attr && f.ratios.placeholder < 4.5)
        problems.push(`${id}: placeholder ratio ${f.ratios.placeholder} (4.5 or more)`);
    }
  }
  if (PHASE !== 'after') return;
  let base: {
    rows: Row[];
    named: Named[];
    fieldFigs: FieldFig[];
    pageInfo: Record<string, { scrollWidth: number; clientWidth: number }>;
  };
  try {
    base = JSON.parse(readFileSync(resolve(OUT, `figures-base-part${PART}.json`), 'utf8'));
  } catch {
    say('no figures-base-part file in OUT: nothing compared');
    return;
  }
  const keyOf = (r: Row, seen: Map<string, number>) => {
    const k = `${r.page}|${r.state}|${r.tag}|${r.name}`;
    const n = seen.get(k) ?? 0;
    seen.set(k, n + 1);
    return `${k}#${n}`;
  };
  const bm = new Map<string, Row>();
  const seenB = new Map<string, number>();
  for (const r of base.rows) bm.set(keyOf(r, seenB), r);
  const seenA = new Map<string, number>();
  const fails = { base: 0, after: 0, named: 0, chips: 0 };
  for (const r of rows) {
    const b = bm.get(keyOf(r, seenA));
    if (r.status === 'fail') fails.after++;
    if (b?.status === 'fail') fails.base++;
    const isNamed = NAMED_RE.test(r.path);
    const takers = r.takers
      .map((t) => `${t.at}:${t.tag}${t.name ? ` "${t.name}"` : ''}`)
      .join(', ');
    const id = `${r.page} [${r.state}] ${r.tag} "${r.name}" ${r.box ? `${r.box.w}x${r.box.h}` : ''}`;
    if (r.status === 'fail' && /lib-chip-list/.test(r.path)) {
      // accepted by the library builder (16 px tag in a text-line row, 8 px gap): listed, not a problem
      notThisPod.push(`ACCEPTED by library: ${id} (${takers})`);
    } else if (r.status === 'fail' && CHIP_RE.test(r.path)) {
      fails.chips++;
      problems.push(
        `${id}: a chip, dropdown or drawer close still fails at 375 (${takers}) <- ${r.path}`,
      );
    } else if (r.status === 'fail' && isNamed) {
      fails.named++;
      problems.push(`${id}: a named control still fails at 375 (${takers}) <- ${r.path}`);
    } else if (r.status === 'fail' && b?.status === 'pass')
      problems.push(`${id}: passed in the base, fails after (${takers}) <- ${r.path}`);
    else if (r.status === 'fail' && (!b || b.status === 'fail'))
      notThisPod.push(`${id} (${takers})`);
    if (
      r.status === 'fail' &&
      r.takers.some((t) => CONTROL_TAGS.has(t.tag)) &&
      !(b?.takers ?? []).some((t) => CONTROL_TAGS.has(t.tag))
    )
      problems.push(`${id}: a tap area takes another control's point (${takers})`);
  }
  say(
    `fails at 375: base ${fails.base}, after ${fails.after}, named still failing ${fails.named}, chips still failing ${fails.chips}`,
  );
  const bn = new Map<string, Named>();
  for (const n of base.named) bn.set(`${n.page}|${n.state}|${n.size}|${n.sel}|${n.i}`, n);
  for (const n of named) {
    const b = bn.get(`${n.page}|${n.state}|${n.size}|${n.sel}|${n.i}`);
    const id = `${n.page} [${n.state}] ${n.sel} #${n.i} "${n.name}"`;
    if (!b) {
      problems.push(`${id}: shown after (${n.size}) but not in the base`);
      continue;
    }
    if (n.size === 1024) {
      const d = (k: 'x' | 'y' | 'w' | 'h') => Math.abs(n.box[k] - b.box[k]);
      if (d('x') > 1 || d('y') > 1 || d('w') > 0.6 || d('h') > 0.6)
        problems.push(
          `${id}: moved at 1024: base ${JSON.stringify(b.box)} after ${JSON.stringify(n.box)}`,
        );
      if (b.hit?.status === 'pass' && n.hit && n.hit.status !== 'pass')
        problems.push(
          `${id}: box hit test passed in the base, ${n.hit.status} after (${n.hit.takers})`,
        );
      continue;
    }
    const al = ALLOW[n.sel] ?? {};
    const dw = n.box.w - b.box.w;
    const dh = n.box.h - b.box.h;
    if (dw > (al.cw ?? 0.5) || dw < -0.5)
      problems.push(`${id}: width ${b.box.w} -> ${n.box.w} (allowed +${al.cw ?? 0.5})`);
    if (dh > (al.ch ?? 0.5) || dh < -0.5)
      problems.push(`${id}: height ${b.box.h} -> ${n.box.h} (allowed +${al.ch ?? 0.5})`);
    if (!ROWS_UNCHECKED_PARENT.has(n.sel)) {
      const dp = n.parent.h - b.parent.h;
      if (dp > (al.ph ?? 0.5) || dp < -0.5)
        problems.push(
          `${id}: its row ${n.parent.tag}.${n.parent.cls} grew ${b.parent.h} -> ${n.parent.h} (allowed +${al.ph ?? 0.5})`,
        );
    }
    if (al.min44 && (n.box.h < 43.5 || (n.sel === '.cin-qa' && n.box.w < 43.5)))
      problems.push(`${id}: ${n.box.w}x${n.box.h} on a phone, 44 px wanted`);
  }
  for (const [k, p] of Object.entries(pageInfo)) {
    void k;
    const q = p as { scrollWidth: number; clientWidth: number };
    if (q.scrollWidth > q.clientWidth) problems.push(`${k}: scrolls sideways`);
  }
  // 1024: field size must not change
  for (const f of fieldFigs) {
    const bf = base.fieldFigs.find(
      (x) => x.page === f.page && x.sel === f.sel && x.mode === f.mode,
    );
    if (bf && (Math.abs(bf.box.w - f.box.w) > 0.6 || Math.abs(bf.box.h - f.box.h) > 0.6))
      problems.push(
        `${f.page} ${f.sel} [${f.mode}]: field box at 1024 ${JSON.stringify(bf.box)} -> ${JSON.stringify(f.box)}`,
      );
  }
  say(`not this pod (base had them too, not named): ${notThisPod.length}`);
  for (const l of notThisPod) say(`  not this pod: ${l}`);
}
