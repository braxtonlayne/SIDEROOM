import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { runCli } from '../support/cli';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * D-958 (wave 101, library): the raw `<select>` elements of the Library's Browse Filters drawer (Status, Rating and
 * Age-restricted) move to the kit's `NativeSelect`. Left raw, with their figures in the handoff: Browse's sort select
 * (toolbar), Series social's "Your rating", the review form's rating and the reader's "Rate this chapter" (a
 * `NativeSelect` there stacks its label above it and moves the neighbours by more than the select's own height
 * difference). A record of every select's box and of the controls around it, base then after, with the exact hit test
 * (`e2e/support/hit-test.ts`; boxes are relative to the measured root).
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; never
 * edited) for its stack and owner. The data it makes goes through the admin API under `p101-lib-<digits>` names
 * (D-942): one published series with one chapter of three pages (the reader's pictures are answered by `page.route`).
 * It is deleted in a `finally`. Ratings and reviews it makes are on that series only and go with it.
 *
 * Phases: `SR_P101_LIBRARY_PHASE=base` records the unedited tree (shots get a `base-` prefix, figures go to
 * `figures-base.json`); the default `after` records the same and, when `figures-base.json` is in the folder, compares
 * (select block size not smaller than the base, at least 44 px at 375 touch for a moved one, inline size not smaller,
 * controls below moved by no more than the summed height difference above them, hit test equal in count and status).
 * Configs: 1280 x 800 and 375 x 812 touch, each in `sectionThemes.library = 'sideroom'` (the shipped theme) and
 * `'lounge-light'`, set by PATCH and localStorage and read back from `.lib-root`'s `data-theme`.
 *
 * Local only (SR_P101_LIBRARY_ON=1). SR_P101_LIBRARY_OUT is the folder of the shots and the figures.
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8786, Vite 5786, metrics 9486,
 * `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and `SR_CLI_BIN` set to the stack's database and sr-cli):
 *   SR_E2E_BASE_URL=http://localhost:5786 SR_P101_LIBRARY_ON=1 \
 *   SR_P101_LIBRARY_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-library \
 *   npx playwright test tests/p101-library-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * (after the merge there is no base: delete `figures-base.json` first if the folder holds one from another tree.)
 */
const ON = process.env.SR_P101_LIBRARY_ON === '1';
const PHASE = process.env.SR_P101_LIBRARY_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P101_LIBRARY_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-library';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const API = '/api/v1';
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PIXEL =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  section: 'sideroom' | 'lounge-light';
  size: string;
  /** Which shots this config takes (names without the phase prefix). */
  browse?: string;
  filters?: string;
  series?: string;
  review?: string;
  focus?: string;
}
const CONFIGS: Cfg[] = [
  {
    tag: '1280-sideroom',
    width: 1280,
    height: 800,
    touch: false,
    section: 'sideroom',
    size: '1280',
    browse: 'browse-1280-sideroom',
    filters: 'browse-filters-1280-sideroom',
    series: 'series-social-1280-sideroom',
    review: 'series-review-1280-sideroom',
    focus: 'browse-1280-focus-sideroom',
  },
  {
    tag: '1280-lounge-light',
    width: 1280,
    height: 800,
    touch: false,
    section: 'lounge-light',
    size: '1280',
    browse: 'browse-1280-lounge-light',
  },
  {
    tag: '375-sideroom',
    width: 375,
    height: 812,
    touch: true,
    section: 'sideroom',
    size: '375',
    browse: 'browse-375-sideroom',
    filters: 'browse-filters-375-sideroom',
    series: 'series-social-375-sideroom',
    review: 'series-review-375-sideroom',
  },
  {
    tag: '375-lounge-light',
    width: 375,
    height: 812,
    touch: true,
    section: 'lounge-light',
    size: '375',
    browse: 'browse-375-lounge-light',
    filters: 'browse-filters-375-lounge-light',
    series: 'series-social-375-lounge-light',
  },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);

async function call<T = any>(
  request: APIRequestContext,
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE',
  path: string,
  data?: unknown,
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
  if (!res.ok()) throw new Error(`${method} ${path} -> ${res.status()}: ${await res.text()}`);
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Measured {
  scroll: number[];
  selects: Record<string, Box | null>;
  controls: { key: string; box: Box }[];
  overlaps: string[];
  extra: Record<string, unknown>;
}
interface HitCount {
  pass: number;
  fail: number;
  notReached: number;
  skipped: number;
}
interface View extends Measured {
  hit: HitCount;
  hitFails: unknown[];
  opened: unknown[];
}
type Figures = Record<string, View | { missing: string }>;

/** Boxes of every select (keyed by its label's first span) and of every control under `root`, scroll offsets added. */
async function measure(page: Page, rootSel: string): Promise<Measured> {
  return page.evaluate((sel) => {
    const root = document.querySelector(sel);
    const none: Measured = { scroll: [0, 0], selects: {}, controls: [], overlaps: [], extra: {} };
    if (!root) return none;
    const abs = (e: Element) => {
      const r = e.getBoundingClientRect();
      let sx = 0;
      let sy = 0;
      for (let p: Element | null = e.parentElement; p; p = p.parentElement) {
        sx += p.scrollLeft;
        sy += p.scrollTop;
      }
      return { x: r.left + sx, y: r.top + sy, w: r.width, h: r.height };
    };
    const o = abs(root);
    // Boxes are relative to the root's top-left corner, so a change above the root does not count as a move.
    const box = (e: Element) => {
      const r = abs(e);
      return {
        x: Math.round((r.x - o.x) * 10) / 10,
        y: Math.round((r.y - o.y) * 10) / 10,
        w: Math.round(r.w * 10) / 10,
        h: Math.round(r.h * 10) / 10,
      };
    };
    const visible = (e: Element) => e.getClientRects().length > 0;
    const selEls = Array.from(root.querySelectorAll<HTMLSelectElement>('select')).filter(visible);
    const selects: Record<string, ReturnType<typeof box>> = {};
    for (const s of selEls) {
      const label = s.closest('label');
      const key = (
        label?.querySelector('span')?.textContent ??
        s.getAttribute('aria-label') ??
        s.id ??
        'select'
      ).trim();
      selects[key] = box(s);
    }
    const els = Array.from(
      root.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
    ).filter(visible);
    const controls = els.map((e, i) => ({
      key: `${i}:${e.tagName.toLowerCase()}#${e.id || ''}:${(e.getAttribute('aria-label') ?? e.textContent ?? '').trim().slice(0, 24)}`,
      box: box(e),
    }));
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
            `${s.id || s.className} x ${o.tagName.toLowerCase()} (${Math.round(ix)}x${Math.round(iy)})`,
          );
      }
    }
    return {
      scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      selects,
      controls,
      overlaps,
      extra: {},
    };
  }, rootSel);
}

const count = (rows: ControlRow[]): HitCount => ({
  pass: rows.filter((r) => r.status === 'pass').length,
  fail: rows.filter((r) => r.status === 'fail').length,
  notReached: rows.filter((r) => r.status === 'not-reached').length,
  skipped: rows.filter((r) => r.status === 'skipped').length,
});

async function view(page: Page, rootSel: string, tag: string, problems: string[]): Promise<View> {
  const m = await measure(page, rootSel);
  if (m.scroll[0]! > m.scroll[1]!)
    problems.push(`${tag}: the document scrolls sideways ${m.scroll.join('/')}`);
  if (m.overlaps.length) problems.push(`${tag}: overlaps ${m.overlaps.join('; ')}`);
  const root = page.locator(rootSel).first();
  const opened = await openCollapsed(page, root);
  const rows = await hitTestAll(page, root);
  const hit = count(rows);
  const hitFails = rows
    .filter((r) => r.status === 'fail' || r.status === 'not-reached')
    .map((r) => ({
      status: r.status,
      control: `${r.tag} "${r.name}"`,
      points: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map((p) => ({ at: p.at, taker: p.taker })),
    }));
  say(`${tag}: selects ${JSON.stringify(m.selects)} hit ${JSON.stringify(hit)}`);
  for (const f of hitFails.filter((h) => h.status === 'not-reached'))
    say(`${tag}: not reached ${JSON.stringify(f)}`);
  if (hit.notReached) problems.push(`${tag}: ${hit.notReached} controls not reached (probe error)`);
  return { ...m, hit, hitFails, opened };
}

async function setPrefs(ctx: BrowserContext, section: string) {
  await apiPatch(ctx.request, '/users/@me/settings', {
    settings: {
      user_prefs: { theme: 'lounge-dark', zoom: 100, sectionThemes: { library: section } },
    },
  });
  const back = (await (await ctx.request.get(`${API}/users/@me/settings`)).json()) as {
    settings?: { user_prefs?: { sectionThemes?: Record<string, string> } };
  };
  expect(back.settings?.user_prefs?.sectionThemes?.['library']).toBe(section);
}

async function newPage(browser: Browser, owner: BrowserContext, c: Cfg) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.width, height: c.height },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((sec) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({
          state: { prefs: { theme: 'lounge-dark', zoom: 100, sectionThemes: { library: sec } } },
          version: 2,
        }),
      );
    } catch {
      /* no storage */
    }
  }, c.section);
  await ctx.route('**/cdn/p101-*.png', (r) =>
    r.fulfill({ status: 200, contentType: 'image/png', body: Buffer.from(PIXEL, 'base64') }),
  );
  await ctx.route(/\/api\/v1\/library\/chapters\/\d+\/pages(\?.*)?$/, async (r) => {
    const res = await r.fetch();
    if (!res.ok()) return r.fulfill({ response: res });
    const body = (await res.json()) as { pages: { url: string }[] };
    body.pages.forEach((p, i) => (p.url = `${ORIGIN}/cdn/p101-p${i}.png`));
    return r.fulfill({ response: res, json: body });
  });
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  return { ctx, page };
}

async function theme(page: Page, c: Cfg, problems: string[]) {
  const dt = await page.locator('.lib-root').first().getAttribute('data-theme');
  say(`${c.tag}: data-theme ${dt}`);
  if (dt !== c.section) problems.push(`${c.tag}: data-theme is ${dt}, wanted ${c.section}`);
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(500);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`), timeout: 10_000 });
}

/** The computed ground and border of a control, for the token probe. */
const probe = (page: Page, sel: string) =>
  page
    .locator(sel)
    .first()
    .evaluate((e) => {
      const cs = getComputedStyle(e);
      return {
        bg: cs.backgroundColor,
        border: `${cs.borderTopWidth} ${cs.borderTopStyle} ${cs.borderTopColor}`,
        color: cs.color,
        padInlineEnd: cs.paddingInlineEnd,
      };
    })
    .catch(() => null);

async function runConfig(
  browser: Browser,
  owner: BrowserContext,
  c: Cfg,
  seriesId: string,
  chapterId: string,
  problems: string[],
  store: Record<string, Figures>,
): Promise<Figures> {
  await setPrefs(owner, c.section);
  const { ctx, page } = await newPage(browser, owner, c);
  const fig: Figures = {};
  store[c.tag] = fig;
  try {
    // ---- Browse: the toolbar (the sort select stays raw) and what a kit-height sort select would do to it.
    await page.goto('/library/browse', { waitUntil: 'domcontentloaded' });
    await page.locator('.lib-browse-bar').waitFor({ timeout: 30_000 });
    await theme(page, c, problems);
    await page.waitForLoadState('networkidle', { timeout: 5000 }).catch(() => undefined);
    await page.waitForTimeout(800);
    const bar = await view(page, '.lib-browse-bar', `${c.tag} browse-bar`, problems);
    bar.extra = await page.evaluate(() => {
      const tools = document.querySelector<HTMLElement>('.lib-browse-tools')!;
      const barEl = document.querySelector<HTMLElement>('.lib-browse-bar')!;
      const lines = (el: HTMLElement) =>
        new Set(Array.from(el.children).map((k) => Math.round(k.getBoundingClientRect().top))).size;
      const sort = tools.querySelector<HTMLSelectElement>('select')!;
      const out: Record<string, unknown> = {
        toolsH: tools.getBoundingClientRect().height,
        barH: barEl.getBoundingClientRect().height,
        toolsLines: lines(tools),
        barLines: lines(barEl),
        children: Array.from(tools.children).map((k) => {
          const r = k.getBoundingClientRect();
          return `${k.tagName.toLowerCase()}.${String(k.className).slice(0, 18)} ${Math.round(r.width)}x${Math.round(r.height)}`;
        }),
      };
      // What a kit-height (40 px, 44 px on touch) sort select would do: set it, read the bar, put it back.
      const want = matchMedia('(pointer: coarse)').matches ? 44 : 40;
      const prev = sort.style.cssText;
      sort.style.minBlockSize = `${want}px`;
      out['whatIf'] = {
        want,
        toolsH: tools.getBoundingClientRect().height,
        barH: barEl.getBoundingClientRect().height,
        toolsLines: lines(tools),
      };
      sort.style.cssText = prev;
      return out;
    });
    say(`${c.tag}: browse bar extra ${JSON.stringify(bar.extra)}`);
    fig['browse-bar'] = bar;
    if (c.tag === '1280-sideroom' || c.tag === '1280-lounge-light') {
      (bar.extra as Record<string, unknown>)['tokens'] = {
        sort: await probe(page, '.lib-browse-tools select'),
        search: await probe(page, '.lib-search input'),
      };
    }
    await page.evaluate(() => window.scrollTo(0, 0));
    if (c.browse) await shot(page, c.browse);

    // ---- Browse: the Filters drawer (Status, Rating, Age-restricted).
    await page.getByRole('button', { name: /^Filters/ }).click();
    const dlg = page.getByRole('dialog');
    await dlg.waitFor({ timeout: 10_000 });
    await page.waitForTimeout(600);
    const drawer = await view(page, '[role="dialog"]', `${c.tag} filters`, problems);
    fig['filters'] = drawer;
    const sample = dlg.locator('select').first();
    (drawer.extra as Record<string, unknown>)['tokens'] = {
      select: await probe(page, '[role="dialog"] select'),
      textField: await probe(page, '[role="dialog"] input'),
    };
    await sample.scrollIntoViewIfNeeded();
    if (c.filters) await shot(page, c.filters);

    // Keyboard (1280 sideroom): focus, ArrowDown changes Status; the draft shows it; put it back (nothing applied).
    if (c.focus) {
      const status = dlg.getByLabel('Status');
      await status.scrollIntoViewIfNeeded();
      await page.keyboard.press('Shift');
      await status.focus();
      const focusVisible = await status.evaluate((e) => e.matches(':focus-visible'));
      const ring = await status.evaluate((e) => {
        const cs = getComputedStyle(e);
        return `${cs.outlineWidth} ${cs.outlineStyle} ${cs.outlineColor} offset ${cs.outlineOffset}`;
      });
      await page.waitForTimeout(300);
      await shot(page, c.focus);
      const before = await status.inputValue();
      await page.keyboard.press('ArrowDown');
      const changed = await status.inputValue();
      await page.keyboard.press('ArrowUp');
      const back = await status.inputValue();
      const k = { focusVisible, ring, before, changed, back };
      (drawer.extra as Record<string, unknown>)['keyboard'] = k;
      say(`${c.tag}: filters keyboard ${JSON.stringify(k)}`);
      if (!focusVisible) problems.push(`${c.tag}: :focus-visible did not match after a key`);
      if (changed === before) problems.push(`${c.tag}: ArrowDown did not change Status`);
      if (back !== before) problems.push(`${c.tag}: ArrowUp did not restore Status`);
    }
    await page.keyboard.press('Escape');

    // ---- The series page: "Your rating".
    await page.goto(`/library/s/${seriesId}`, { waitUntil: 'domcontentloaded' });
    await page.getByLabel('Your rating').first().waitFor({ timeout: 30_000 });
    await page.waitForLoadState('networkidle', { timeout: 5000 }).catch(() => undefined);
    await theme(page, c, problems);
    await page.waitForTimeout(800);
    const ser = await view(page, '.lib-rating', `${c.tag} series`, problems);
    fig['series'] = ser;
    (ser.extra as Record<string, unknown>)['tokens'] = {
      select: await probe(page, '.lib-rate select'),
    };
    await page.getByLabel('Your rating').first().scrollIntoViewIfNeeded();
    if (c.series) await shot(page, c.series);

    // Keyboard (1280 sideroom): ArrowDown rates the series (own data), ArrowUp un-rates.
    if (c.focus) {
      const sel = page.getByLabel('Your rating').first();
      await sel.focus();
      const before = await sel.inputValue();
      await page.keyboard.press('ArrowDown');
      await expect(sel).not.toHaveValue(before);
      const changed = await sel.inputValue();
      const mine = await call<{ mine: number | null }>(
        ctx.request,
        'GET',
        `/library/series/${seriesId}/ratings`,
      )
        .then((r) => r.mine)
        .catch(() => 'n/a');
      await expect(sel).toBeEnabled();
      await sel.focus();
      await page.keyboard.press('ArrowUp');
      await expect(sel).toHaveValue(before);
      await expect(sel).toBeEnabled();
      const k = { before, changed, serverMine: mine };
      (ser.extra as Record<string, unknown>)['keyboard'] = k;
      say(`${c.tag}: rating keyboard ${JSON.stringify(k)}`);
    }

    // ---- The Reviews tab: the form's rating select.
    await page.getByRole('tab', { name: 'Reviews' }).click();
    await page.getByRole('button', { name: /Write a review|Edit your review/ }).click();
    await page.getByLabel('Rating with this review').first().waitFor();
    await page.waitForTimeout(600);
    const rev = await view(page, '.lib-review-form', `${c.tag} review form`, problems);
    fig['review'] = rev;
    await page.getByLabel('Rating with this review').first().scrollIntoViewIfNeeded();
    if (c.review) await shot(page, c.review);

    // ---- The reader's end card: "Rate this chapter" (best effort; not reached is listed).
    await page.goto(`/library/r/${chapterId}/1`, { waitUntil: 'domcontentloaded' });
    let end = false;
    for (let i = 0; i < 8 && !end; i++) {
      end = await page
        .locator('.lib-chapter-rate select')
        .first()
        .isVisible()
        .catch(() => false);
      if (!end) {
        await page
          .locator('.lib-endcard')
          .first()
          .scrollIntoViewIfNeeded({ timeout: 2000 })
          .catch(() => undefined);
        for (let k = 0; k < 3; k++) await page.keyboard.press('ArrowRight');
        await page.waitForTimeout(800);
      }
    }
    if (end) {
      fig['chapter-rate'] = await view(page, '.lib-endcard', `${c.tag} chapter rate`, problems);
    } else {
      fig['chapter-rate'] = { missing: 'the end card did not show a Rate this chapter select' };
      say(`${c.tag}: chapter rate not reached`);
    }
  } finally {
    await ctx.close();
  }
  return fig;
}

function compare(c: Cfg, base: Figures, after: Figures, problems: string[]) {
  const diffs: Record<string, unknown> = {};
  for (const name of Object.keys(after)) {
    const a = after[name]!;
    const b = base[name];
    if (!b || 'missing' in a || 'missing' in b) {
      if (!!b && 'missing' in b !== 'missing' in a)
        problems.push(`${c.tag} ${name}: reached in only one phase`);
      continue;
    }
    const d: Record<string, unknown> = {};
    for (const key of Object.keys(a.selects)) {
      const ab = a.selects[key];
      const bb = b.selects[key];
      if (!ab || !bb) continue;
      d[key] = { base: bb, after: ab };
      const raw = name === 'browse-bar';
      if (ab.h < bb.h - 0.5)
        problems.push(`${c.tag} ${name} "${key}": shorter than base (${ab.h} < ${bb.h})`);
      if (!raw && c.touch && ab.h < 44)
        problems.push(`${c.tag} ${name} "${key}": ${ab.h} px tall at 375 touch`);
      if (ab.w < bb.w - 0.5)
        problems.push(`${c.tag} ${name} "${key}": narrower than base (${ab.w} < ${bb.w})`);
    }
    if (a.controls.length !== b.controls.length) {
      problems.push(`${c.tag} ${name}: control count ${b.controls.length} -> ${a.controls.length}`);
      diffs[name] = d;
      continue;
    }
    const moved: { key: string; moved: number; allowed: number }[] = [];
    b.controls.forEach((bc, i) => {
      const ac = a.controls[i]!;
      // Allowed: the height added by every select wholly above this control, plus (for a control in a select's own
      // row, which is centred or stretched with it) up to the select's own height difference either way.
      let below = 0;
      let beside = 0;
      for (const key of Object.keys(a.selects)) {
        const bs = b.selects[key];
        const as = a.selects[key];
        if (!bs || !as) continue;
        const diff = as.h - bs.h;
        if (bs.y + bs.h <= bc.box.y + 0.5) below += diff;
        else if (bs.y < bc.box.y + bc.box.h) beside += Math.abs(diff);
      }
      const allowed = below + beside;
      const mv = Math.round((ac.box.y - bc.box.y) * 10) / 10;
      moved.push({ key: bc.key, moved: mv, allowed: Math.round(allowed * 10) / 10 });
      if (mv > allowed + 1)
        problems.push(`${c.tag} ${name}: ${bc.key} moved ${mv} px, allowed ${allowed}`);
      if (mv < -(beside + 1)) problems.push(`${c.tag} ${name}: ${bc.key} moved up ${mv} px`);
    });
    d['moved'] = moved.filter((x) => x.moved !== 0 || x.allowed !== 0);
    const sum = (h: HitCount) => h.pass + h.fail + h.notReached + h.skipped;
    if (sum(b.hit) !== sum(a.hit) || a.hit.pass < b.hit.pass || a.hit.fail > b.hit.fail)
      problems.push(
        `${c.tag} ${name}: hit test ${JSON.stringify(b.hit)} -> ${JSON.stringify(a.hit)}`,
      );
    d['hit'] = { base: b.hit, after: a.hit };
    diffs[name] = d;
  }
  return diffs;
}

test(`p101 library: Browse and Series social selects, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true);
  try {
    runCli([
      'staff',
      'appoint',
      owner.username,
      '--role',
      'Senior Staff',
      '--reason',
      'e2e p101 library shots',
    ]);
  } catch (e) {
    say(`staff appoint: ${String(e).slice(0, 120)}`);
  }
  await call(octx.request, 'POST', '/sr/age-gate', { over_18: true }).catch(() => undefined);

  const stamp = String(Date.now()).slice(-6); // digits only: the same width every run
  const problems: string[] = [];
  const figs: Record<string, Figures> = {};
  let seriesId = '';
  try {
    const series = await call<{ id: string }>(octx.request, 'POST', '/admin/library/series', {
      kind: 'manga',
      title: `p101-lib-${stamp}`,
      synopsis: 'A series made by the wave 101 library spec.',
      genres: ['Drama'],
      authors: ['E2E Author'],
      reading_direction: 'ltr',
    });
    seriesId = series.id;
    const edition = await call<{ id: string }>(
      octx.request,
      'POST',
      `/admin/library/series/${seriesId}/editions`,
      { language: 'en', label: 'Official', is_default: true },
    );
    const chapter = await call<{ id: string }>(
      octx.request,
      'POST',
      `/admin/library/editions/${edition.id}/chapters`,
      { number: 1, title: 'p101-library chapter', format: 'images' },
    );
    await call(
      octx.request,
      'PUT',
      `/admin/library/chapters/${chapter.id}/pages`,
      [0, 1, 2].map((i) => ({
        key: `library/${seriesId}/${chapter.id}/00${i + 1}.webp`,
        width: 200,
        height: 300,
        variants: {},
      })),
    );
    await call(octx.request, 'POST', `/admin/library/chapters/${chapter.id}/publish`);
    await call(octx.request, 'POST', `/admin/library/series/${seriesId}/publish`);
    say(`series ${seriesId} chapter ${chapter.id}`);

    for (const c of CONFIGS) {
      try {
        figs[c.tag] = await runConfig(browser, octx, c, seriesId, chapter.id, problems, figs);
      } catch (e) {
        problems.push(`${c.tag}: ${String(e).slice(0, 600)}`);
      }
    }
  } finally {
    if (seriesId) {
      await call(octx.request, 'POST', `/admin/library/series/${seriesId}/unpublish`).catch(
        () => undefined,
      );
      await call(octx.request, 'DELETE', `/admin/library/series/${seriesId}`).catch((e) =>
        say(`series delete: ${String(e).slice(0, 160)}`),
      );
    }
    await octx.close();
  }
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Figures>;
    const cmp: Record<string, unknown> = {};
    for (const c of CONFIGS)
      if (base[c.tag] && figs[c.tag]) cmp[c.tag] = compare(c, base[c.tag]!, figs[c.tag]!, problems);
    writeFileSync(resolve(OUT, 'compare.json'), JSON.stringify(cmp, null, 1));
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  say(`problems: ${JSON.stringify(problems)}`);
  expect(problems).toEqual([]);
});
