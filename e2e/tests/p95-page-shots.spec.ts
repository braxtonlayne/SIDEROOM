import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * Wave 95, page builder: SEC-AUD-433 (D-936: on a phone, 720 px and under, a full profile page is one column) and
 * SEC-AUD-431 (D-937: the guestbook's Report, Approve and Remove buttons are 44 px tall; the vouch badge and the ring
 * button get 44 px targets). Rules: packages/profile-blocks/src/profile-blocks.css,
 * apps/web/src/profilepage/vouch/vouch.css, apps/web/src/profilepage/profilepage.css.
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by
 *   SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium \
 *     --workers=1 --retries=0 --reporter=line
 * from e2e/; that spec is never edited; DATABASE_URL and SR_CLI_BIN set for the seed). The Owner's page holds the three
 * link blocks, a guestbook note by another member and a pending note from a visitor. The last case approves and
 * removes notes, so it changes the world: run it on a fresh one.
 * Local only: without SR_P95_PAGE_ON=1 it skips itself.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8732, Vite 5732 for this builder; use the
 * commander's own Vite port in SR_E2E_BASE_URL):
 *   SR_E2E_BASE_URL=http://localhost:5732 SR_P95_PAGE_ON=1 \
 *   SR_P95_PAGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p95-page \
 *   npx playwright test tests/p95-page-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * SR_P95_PAGE_BASE=1 runs only the base figures.
 *
 * "Base" is this tree's own CSS with the wave 95 `@media (max-width: 720px)` blocks deleted from the loaded style sheets
 * at run time (CSSOM; the blocks that name `:not([data-compact])`, `.vouch-badge::after` or `.rng-btn::after`), so base
 * and after run in the same round on the same stack. Hit tests: `e2e/support/hit-test.ts`, nothing private.
 *
 * Case 1: 375 touch, Lounge Light, base then after: every `.pp-grid > .pp-block` box, the grid's columns,
 *   `openCollapsed` and `hitTestAll` over `main` (0 fail, 0 not reached after), page height, sideways scroll;
 *   Lounge Dark after.
 * Case 2: after, 375: a tap at the centre and 20 px under the centre of Report opens its dialog and removes nothing;
 *   the vouch badge shows its tip; the ring opens.
 * Case 3: 700 x 812 after: one column, every control passes. 1280 Editorial, base and after: every box under `main` equal.
 * Case 4: the public page signed out at 375 (`/~owner`): blocks and a hit test over the page, base and after.
 * Case 5 (changes the world): Approve approves the pending note; Remove removes one note and the other stays.
 */
const ON = process.env.SR_P95_PAGE_ON === '1';
const BASE_ONLY = process.env.SR_P95_PAGE_BASE === '1';
const OUT =
  process.env.SR_P95_PAGE_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p95-page';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PHONE = { width: 375, height: 812 };
const MID = { width: 700, height: 812 };
const DESK = { width: 1280, height: 800 };

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

type Mode = 'base' | 'after';
const report: Record<string, unknown> = {};
const save = () => {
  mkdirSync(OUT, { recursive: true });
  writeFileSync(resolve(OUT, 'p95-page.json'), JSON.stringify(report, null, 1));
};
const say = (s: string) => console.log(`REPORT ${s}`);

async function setMain(ownerCtx: BrowserContext, themeId: string) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: themeId, zoom: 100 } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  expect(back.settings?.user_prefs?.theme).toBe(themeId);
}

async function newPage(
  browser: Browser,
  ownerCtx: BrowserContext | null,
  themeId: string,
  size: { width: number; height: number } = PHONE,
) {
  const phone = size.width <= 720;
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    ...(ownerCtx ? { storageState: await ownerCtx.storageState() } : {}),
    bypassCSP: true,
    ...(phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((th) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({ state: { prefs: { theme: th, zoom: 100 } }, version: 2 }),
      );
    } catch {
      /* no storage */
    }
  }, themeId);
  const page = await ctx.newPage();
  page.setDefaultTimeout(8000);
  return { ctx, page };
}

const idle = async (page: Page, ms = 1500) => {
  await page.waitForLoadState('networkidle', { timeout: 3000 }).catch(() => undefined);
  await page.waitForTimeout(ms);
};
const go = async (page: Page, path: string) => {
  await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 20_000 });
  await idle(page);
};
const shot = async (page: Page, name: string) => {
  mkdirSync(OUT, { recursive: true });
  await page.screenshot({ path: resolve(OUT, `${name}.png`), timeout: 10_000 });
};

/** Base case: delete the wave 95 media blocks from the loaded sheets (CSSOM). Returns how many were deleted. */
async function strip(page: Page): Promise<number> {
  return page.evaluate(() => {
    let n = 0;
    const mine = (t: string) =>
      t.includes(':not([data-compact])') ||
      t.includes('.vouch-badge::after') ||
      t.includes('.rng-btn::after');
    const walk = (rules: CSSRuleList, owner: CSSStyleSheet | CSSGroupingRule) => {
      for (let i = rules.length - 1; i >= 0; i--) {
        const r = rules[i]!;
        if (r instanceof CSSMediaRule && mine(r.cssText)) {
          owner.deleteRule(i);
          n++;
        } else if ('cssRules' in r && (r as CSSGroupingRule).cssRules.length)
          walk((r as CSSGroupingRule).cssRules, r as CSSGroupingRule);
      }
    };
    for (const s of Array.from(document.styleSheets)) {
      try {
        walk(s.cssRules, s);
      } catch {
        /* cross-origin sheet */
      }
    }
    return n;
  });
}

const envOf = (page: Page) =>
  page.evaluate(() => ({
    theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
    height: document.documentElement.scrollHeight,
  }));

const rowOf = (r: ControlRow) => ({
  key: `${r.index} ${r.tag} "${r.name.slice(0, 30)}"`,
  path: r.path,
  status: r.status,
  box: r.result ? `${r.result.box.width.toFixed(0)} x ${r.result.box.height.toFixed(0)}` : null,
  fails: (r.result?.points ?? [])
    .filter((p) => !p.ok)
    .map((p) => `${p.at}: ${p.taker?.tag} "${p.taker?.name}" (${p.taker?.path})`),
});

/** Every block of the grid: type, x, width, y, height; and the grid's computed columns. */
const blocksOf = (page: Page) =>
  page.evaluate(() => {
    const grid = document.querySelector('.pp-root:not([data-compact]) .pp-grid');
    const blocks = Array.from(document.querySelectorAll<HTMLElement>('.pp-grid > .pp-block')).map(
      (e) => {
        const r = e.getBoundingClientRect();
        return {
          type: e.getAttribute('data-type'),
          x: +r.x.toFixed(1),
          w: +r.width.toFixed(1),
          y: +(r.y + scrollY).toFixed(1),
          h: +r.height.toFixed(1),
          cut: e.scrollWidth > e.clientWidth + 1,
          text: (e.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 40),
        };
      },
    );
    return {
      columns: grid ? getComputedStyle(grid).gridTemplateColumns : null,
      blocks,
      oneColumn: new Set(blocks.map((b) => `${b.x}/${b.w}`)).size === 1,
    };
  });

/** The guestbook's direct buttons: name and box (Report, Approve, Remove; the Sign button is not among them). */
const gbButtons = (page: Page) =>
  page.evaluate(() =>
    Array.from(document.querySelectorAll<HTMLElement>('.pp-entries li > button')).map((e) => {
      const r = e.getBoundingClientRect();
      return {
        name: e.getAttribute('aria-label') ?? (e.textContent ?? '').trim(),
        w: +r.width.toFixed(1),
        h: +r.height.toFixed(1),
      };
    }),
  );

async function case1(browser: Browser, owner: BrowserContext, mode: Mode, themeId: string) {
  const tag = `375-${themeId}-${mode}`;
  await setMain(owner, themeId);
  const { ctx, page } = await newPage(browser, owner, themeId);
  await go(page, '/u/owner');
  const stripped = mode === 'base' ? await strip(page) : 0;
  if (mode === 'base') expect(stripped).toBeGreaterThan(0);
  const root = page.locator('main').first();
  expect(await root.count()).toBe(1);
  const opened = (await openCollapsed(page, root)).length;
  const fig: Record<string, unknown> = { mode, themeId, stripped, opened };
  fig['grid'] = await blocksOf(page);
  fig['gbButtons'] = await gbButtons(page);
  fig['env'] = await envOf(page);
  const rows = await hitTestAll(page, root);
  fig['rows'] = rows.map(rowOf);
  fig['notReached'] = rows.filter((r) => r.status === 'not-reached').length;
  fig['fails'] = rows.filter((r) => r.status === 'fail').map(rowOf);
  // Shots: the top, then the guestbook (light only; dark after: the top).
  await page.evaluate(() => window.scrollTo(0, 0));
  await shot(page, `1-profile-page-${tag}-top`);
  if (themeId === 'lounge-light') {
    const gb = page.locator('.pp-entries').first();
    if (await gb.count()) await gb.scrollIntoViewIfNeeded();
    await shot(page, `1-profile-page-${tag}-guestbook`);
  }
  const store = (report['case1'] ??= {}) as Record<string, unknown>;
  store[tag] = fig;
  save();
  const g = fig['grid'] as { columns: string; oneColumn: boolean; blocks: unknown[] };
  say(
    `case1 ${tag}: stripped ${stripped}, ${rows.length} rows, fails ${(fig['fails'] as unknown[]).length}, not reached ${String(fig['notReached'])}, columns ${g.columns}, oneColumn ${String(g.oneColumn)}, blocks ${g.blocks.length}, env ${JSON.stringify(fig['env'])}`,
  );
  say(`case1 ${tag} blocks: ${JSON.stringify(g.blocks)}`);
  say(`case1 ${tag} guestbook buttons: ${JSON.stringify(fig['gbButtons'])}`);
  say(`case1 ${tag} rows: ${JSON.stringify(fig['rows'])}`);
  say(`case1 ${tag} fails: ${JSON.stringify(fig['fails'])}`);
  await ctx.close();
  return { rows: rows.map(rowOf), fig };
}

async function case2(browser: Browser, owner: BrowserContext) {
  await setMain(owner, 'lounge-light');
  const { ctx, page } = await newPage(browser, owner, 'lounge-light');
  await go(page, '/u/owner');
  const out: Record<string, unknown> = {};
  const count = () => page.locator('.pp-entries > li').count();
  const before = await count();
  // Report: a tap at the centre and 20 px under the centre opens its dialog and removes nothing.
  const reports = page.locator('.pp-entries li > button').filter({ hasText: /report/i });
  const nReport = await reports.count();
  const taps: unknown[] = [];
  for (const dy of [0, 20]) {
    const b = reports.first();
    await b.scrollIntoViewIfNeeded();
    const bx = (await b.boundingBox())!;
    await page.touchscreen.tap(bx.x + bx.width / 2, bx.y + bx.height / 2 + dy);
    await page.waitForTimeout(700);
    const dialog = await page.getByRole('dialog').count();
    taps.push({ dy, dialog, entries: await count() });
    await page.keyboard.press('Escape');
    await page.waitForTimeout(400);
  }
  out['reportTaps'] = { nReport, before, taps };
  say(`case2 report taps: ${JSON.stringify(out['reportTaps'])}`);
  // The vouch badge: a tap shows its tip.
  const badge = page.locator('.vouch-badge').first();
  if (await badge.count()) {
    await badge.scrollIntoViewIfNeeded();
    const bx = (await badge.boundingBox())!;
    await page.touchscreen.tap(bx.x + bx.width / 2, bx.y + bx.height / 2 + 15);
    await page.waitForTimeout(600);
    out['vouchTip'] = await page.locator('.vouch-tip').count();
  } else out['vouchTip'] = 'no badge';
  say(`case2 vouch tip shown: ${JSON.stringify(out['vouchTip'])}`);
  // The ring: a tap opens it.
  await go(page, '/u/owner');
  const ring = page.locator('.pp-body .rng-btn').first();
  if (await ring.count()) {
    await ring.scrollIntoViewIfNeeded();
    const bx = (await ring.boundingBox())!;
    await page.touchscreen.tap(bx.x + bx.width / 2, bx.y + bx.height / 2 + 21);
    await page.waitForTimeout(800);
    out['ringOpened'] = await page.getByRole('dialog').count();
  } else out['ringOpened'] = 'no ring';
  say(
    `case2 ring dialogs after a tap 21 px under the centre: ${JSON.stringify(out['ringOpened'])}`,
  );
  report['case2'] = out;
  save();
  await ctx.close();
}

async function case3(browser: Browser, owner: BrowserContext) {
  // 700 x 812 after.
  await setMain(owner, 'lounge-light');
  const m = await newPage(browser, owner, 'lounge-light', MID);
  await go(m.page, '/u/owner');
  const root = m.page.locator('main').first();
  await openCollapsed(m.page, root);
  const rows = await hitTestAll(m.page, root);
  const grid = await blocksOf(m.page);
  report['case3_700'] = {
    grid,
    rows: rows.map(rowOf),
    env: await envOf(m.page),
    notReached: rows.filter((r) => r.status === 'not-reached').length,
    fails: rows.filter((r) => r.status === 'fail').map(rowOf),
  };
  say(
    `case3 700: oneColumn ${String(grid.oneColumn)}, columns ${grid.columns}, ${rows.length} rows, fails ${JSON.stringify(rows.filter((r) => r.status === 'fail').map(rowOf))}, notReached ${rows.filter((r) => r.status === 'not-reached').length}, env ${JSON.stringify(await envOf(m.page))}`,
  );
  await m.page.evaluate(() => window.scrollTo(0, 0));
  await shot(m.page, '3-profile-page-700-after');
  await m.ctx.close();
  // 1280 Editorial, base and after.
  await setMain(owner, 'lounge-editorial');
  const res: Record<Mode, string[]> = { base: [], after: [] };
  for (const mode of ['base', 'after'] as Mode[]) {
    const { ctx, page } = await newPage(browser, owner, 'lounge-editorial', DESK);
    await go(page, '/u/owner');
    if (mode === 'base') await strip(page);
    res[mode] = await page.evaluate(() =>
      Array.from(document.querySelectorAll<HTMLElement>('main, main *')).map((e) => {
        const r = e.getBoundingClientRect();
        return `${e.tagName.toLowerCase()}.${String(e.className).split(' ')[0]} ${r.x.toFixed(1)},${(r.y + scrollY).toFixed(1)},${r.width.toFixed(1)},${r.height.toFixed(1)}`;
      }),
    );
    if (mode === 'after') {
      await page.evaluate(() => window.scrollTo(0, 0));
      await shot(page, '3-profile-page-1280-after');
    }
    await ctx.close();
  }
  const moved: string[] = [];
  res.base.forEach((v, i) => v !== res.after[i] && moved.push(`${v} -> ${res.after[i]}`));
  if (res.base.length !== res.after.length)
    moved.push(`count ${res.base.length} -> ${res.after.length}`);
  report['case3_1280'] = { total: res.base.length, moved };
  say(
    `case3 1280: ${res.base.length} boxes, moved ${moved.length}: ${moved.slice(0, 5).join(' | ')}`,
  );
  save();
  expect(moved).toEqual([]);
}

async function case4(browser: Browser) {
  const out: Record<string, unknown> = {};
  for (const mode of ['after', 'base'] as Mode[]) {
    const { ctx, page } = await newPage(browser, null, 'lounge-light');
    await go(page, '/~owner');
    if ((await page.locator('.pp-linktree a').count()) === 0) {
      report['case4'] = 'not tested: the Owner page is not public in this world';
      say(`case4 not tested: /~owner shows ${new URL(page.url()).pathname}`);
      save();
      await ctx.close();
      return;
    }
    if (mode === 'base') await strip(page);
    const scope = page.locator('.pp-root').first();
    await openCollapsed(page, scope);
    const rows = await hitTestAll(page, scope);
    out[mode] = {
      grid: await blocksOf(page),
      rows: rows.map(rowOf),
      fails: rows.filter((r) => r.status === 'fail').map(rowOf),
      notReached: rows.filter((r) => r.status === 'not-reached').length,
      env: await envOf(page),
    };
    if (mode === 'after') {
      await page.evaluate(() => window.scrollTo(0, 0));
      await shot(page, '4-public-page-375-after');
    }
    await ctx.close();
  }
  report['case4'] = out;
  say(`case4: ${JSON.stringify(out)}`);
  save();
}

async function case5(browser: Browser, owner: BrowserContext) {
  await setMain(owner, 'lounge-light');
  const { ctx, page } = await newPage(browser, owner, 'lounge-light');
  await go(page, '/u/owner');
  const out: Record<string, unknown> = {};
  const count = () => page.locator('.pp-entries > li').count();
  const tapBtn = async (name: RegExp) => {
    const b = page.locator('.pp-entries li > button').filter({ hasText: name }).first();
    await b.scrollIntoViewIfNeeded();
    const bx = (await b.boundingBox())!;
    await page.touchscreen.tap(bx.x + bx.width / 2, bx.y + bx.height / 2);
    await page.waitForTimeout(1200);
  };
  const n0 = await count();
  const approveBefore = await page
    .locator('.pp-entries li > button')
    .filter({ hasText: /approve/i })
    .count();
  if (approveBefore) await tapBtn(/approve/i);
  const approveAfter = await page
    .locator('.pp-entries li > button')
    .filter({ hasText: /approve/i })
    .count();
  out['approve'] = { before: approveBefore, after: approveAfter, entries: await count() };
  const n1 = await count();
  if (n1 >= 2) await tapBtn(/remove/i);
  await idle(page, 500);
  out['remove'] = { entriesBefore: n1, entriesAfter: await count(), initial: n0 };
  report['case5'] = out;
  say(`case5: ${JSON.stringify(out)}`);
  save();
  await ctx.close();
}

test('p95 profile page: one column on a phone, the last controls', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const errors: string[] = [];
  const run = async (id: string, fn: () => Promise<void>) => {
    const t0 = Date.now();
    try {
      await fn();
    } catch (e) {
      errors.push(`case ${id}: ${String(e).slice(0, 600)}`);
      say(`case ${id} FAILED ${String(e).slice(0, 600)}`);
    }
    say(`case ${id} took ${Math.round((Date.now() - t0) / 1000)} s`);
  };
  await run('1', async () => {
    const base = await case1(browser, owner, 'base', 'lounge-light');
    if (BASE_ONLY) return;
    const after = await case1(browser, owner, 'after', 'lounge-light');
    const regress = base.rows
      .map((b, i) => ({ b, a: after.rows[i] }))
      .filter(({ b, a }) => b.status === 'pass' && a && a.status !== 'pass')
      .map(({ b, a }) => `${b.key}: ${a!.status} ${JSON.stringify(a!.fails)}`);
    report['regressions'] = regress;
    say(`case1 regressions (passed on base, not after): ${JSON.stringify(regress)}`);
    const baseH = (base.fig['env'] as { height: number }).height;
    const afterH = (after.fig['env'] as { height: number }).height;
    say(`case1 page height base ${baseH}, after ${afterH}`);
    save();
    const dark = await case1(browser, owner, 'after', 'lounge-dark');
    expect(regress).toEqual([]);
    expect(after.fig['notReached']).toBe(0);
    expect(after.fig['fails']).toEqual([]);
    expect(dark.fig['fails']).toEqual([]);
    expect((after.fig['grid'] as { oneColumn: boolean }).oneColumn).toBe(true);
  });
  if (!BASE_ONLY) {
    await run('2', () => case2(browser, owner));
    await run('3', () => case3(browser, owner));
    await run('4', () => case4(browser));
    await run('5', () => case5(browser, owner));
  }
  await owner.close();
  report['errors'] = errors;
  save();
  if (errors.length) throw new Error(errors.join('\n'));
});
