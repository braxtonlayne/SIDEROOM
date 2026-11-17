import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * Wave 94, profile builder: SEC-AUD-427 (D-933: on a phone a profile page's link block is at least 44 px tall and the
 * guestbook author link has a 44 px target; the rules are in packages/profile-blocks/src/profile-blocks.css).
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by
 *   SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium \
 *     --workers=1 --retries=0 --reporter=line
 * from e2e/; that spec is never edited; DATABASE_URL and SR_CLI_BIN set for the seed). The Owner's account. The world's
 * Owner page holds the three link blocks (Bitcoin, Ko-fi, Blog) and a guestbook entry by another member.
 * Local only: without SR_P94_PROFILE_ON=1 it skips itself.
 *
 * The commander's command, from e2e/, on the merged tree's stack with the sweep's world (API 8722, Vite 5722; use the
 * commander's own Vite port in SR_E2E_BASE_URL):
 *   SR_E2E_BASE_URL=http://localhost:5722 SR_P94_PROFILE_ON=1 \
 *   SR_P94_PROFILE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-profile \
 *   npx playwright test tests/p94-profile-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * SR_P94_PROFILE_BASE=1 runs only the base figures (the D-933 rule deleted from the loaded sheets).
 *
 * "Base" is this tree's own CSS with the D-933 `@media (max-width: 720px)` block deleted from the loaded style sheets
 * at run time (CSSOM), so base and after run in the same round on the same stack. The hit tests are
 * `e2e/support/hit-test.ts`; nothing private.
 *
 * Case 1: 375 touch, Lounge Light, base then after: `openCollapsed`, `hitTestAll` over the page's `main`: every
 *   control, box, status, takers; the link blocks and the author link by name; page height; sideways scroll.
 *   After: no control that passed on the base fails now. Lounge Dark after: the same figures.
 * Case 2: after, 375 Lounge Light: a tap 20 px above and below each link block's centre reaches the block (a click
 *   handler that stops the navigation records it; the `href` is read), a tap 20 px above and below the author link
 *   opens /u/<author>.
 * Case 3: 1280 Editorial, base and after: the box of every element under `main` equal.
 * Case 4: the public page signed out at 375 (`/~owner`), when the Owner's page is published in the world; otherwise
 *   it says "not tested".
 */
const ON = process.env.SR_P94_PROFILE_ON === '1';
const BASE_ONLY = process.env.SR_P94_PROFILE_BASE === '1';
const OUT =
  process.env.SR_P94_PROFILE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-profile';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PHONE = { width: 375, height: 812 };
const DESK = { width: 1280, height: 800 };

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

type Mode = 'base' | 'after';
type Tapped = { __tap: string[] };
const report: Record<string, unknown> = {};
const save = () => {
  mkdirSync(OUT, { recursive: true });
  writeFileSync(resolve(OUT, 'p94-profile.json'), JSON.stringify(report, null, 1));
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
  phone = true,
) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: phone ? PHONE : DESK,
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

/** Base case: delete the D-933 media block from the loaded sheets (CSSOM). Returns how many were deleted. */
async function strip(page: Page): Promise<number> {
  return page.evaluate(() => {
    let n = 0;
    const walk = (rules: CSSRuleList, owner: CSSStyleSheet | CSSGroupingRule) => {
      for (let i = rules.length - 1; i >= 0; i--) {
        const r = rules[i]!;
        if (
          r instanceof CSSMediaRule &&
          r.cssText.includes('.pp-linktree a') &&
          r.cssText.includes('min-block-size')
        ) {
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

/** The blocks this item moves: link blocks and guestbook author links, with box and href. */
const movers = (page: Page) =>
  page.evaluate(() =>
    Array.from(
      document.querySelectorAll<HTMLElement>('.pp-linktree a, .pp-entries .pp-muted > a.pp-link'),
    ).map((e) => {
      const r = e.getBoundingClientRect();
      return {
        sel: e.closest('.pp-linktree') ? 'linktree' : 'author',
        text: (e.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 30),
        href: e.getAttribute('href'),
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
  const mv = await movers(page);
  fig['movers'] = mv;
  fig['env'] = await envOf(page);
  const rows = await hitTestAll(page, root);
  fig['rows'] = rows.map(rowOf);
  fig['notReached'] = rows.filter((r) => r.status === 'not-reached').length;
  fig['fails'] = rows.filter((r) => r.status === 'fail').map(rowOf);
  // Bring the link blocks and the guestbook into view for the shot.
  const gb = page.locator('.pp-entries').first();
  if (await gb.count()) await gb.scrollIntoViewIfNeeded();
  await shot(page, `1-profile-page-${tag}`);
  const store = (report['case1'] ??= {}) as Record<string, unknown>;
  store[tag] = fig;
  save();
  say(
    `case1 ${tag}: stripped ${stripped}, ${rows.length} rows, fails ${(fig['fails'] as unknown[]).length}, not reached ${String(fig['notReached'])}, movers ${JSON.stringify(mv)}, env ${JSON.stringify(fig['env'])}`,
  );
  say(`case1 ${tag} fails: ${JSON.stringify(fig['fails'])}`);
  await ctx.close();
  return rows.map(rowOf);
}

async function case2(browser: Browser, owner: BrowserContext) {
  await setMain(owner, 'lounge-light');
  const { ctx, page } = await newPage(browser, owner, 'lounge-light');
  await go(page, '/u/owner');
  const out: Record<string, unknown> = {};
  // Link blocks: a tap 20 px above and below the centre reaches the block (the navigation is stopped here).
  await page.evaluate(() => {
    (window as unknown as Tapped).__tap = [];
    document.querySelectorAll<HTMLAnchorElement>('.pp-linktree a').forEach((a) =>
      a.addEventListener('click', (e) => {
        e.preventDefault();
        (window as unknown as Tapped).__tap.push(a.getAttribute('href') ?? '');
      }),
    );
  });
  const blocks = page.locator('.pp-linktree a');
  const n = await blocks.count();
  const taps: unknown[] = [];
  for (let i = 0; i < n; i++) {
    const b = blocks.nth(i);
    await b.scrollIntoViewIfNeeded();
    const bx = (await b.boundingBox())!;
    const href = await b.getAttribute('href');
    for (const dy of [-20, 20]) {
      await page.evaluate(() => ((window as unknown as Tapped).__tap = []));
      await page.touchscreen.tap(bx.x + bx.width / 2, bx.y + bx.height / 2 + dy);
      await page.waitForTimeout(150);
      const got = await page.evaluate(() => (window as unknown as Tapped).__tap);
      taps.push({ i, href, dy, got });
    }
  }
  out['linkBlockTaps'] = taps;
  say(`case2 link block taps: ${JSON.stringify(taps)}`);
  // The author link: tap above and below its box.
  if (await page.locator('.pp-entries .pp-muted > a.pp-link').count()) {
    const results: unknown[] = [];
    let href: string | null = null;
    for (const dy of [-20, 20]) {
      await go(page, '/u/owner');
      const a = page.locator('.pp-entries .pp-muted > a.pp-link').first();
      href = await a.getAttribute('href');
      await a.scrollIntoViewIfNeeded();
      const bx = (await a.boundingBox())!;
      await page.touchscreen.tap(bx.x + bx.width / 2, bx.y + bx.height / 2 + dy);
      await page.waitForTimeout(1500);
      results.push({ dy, url: new URL(page.url()).pathname });
    }
    out['authorTaps'] = { href, results };
    say(`case2 author taps: href ${String(href)} ${JSON.stringify(results)}`);
  } else out['authorTaps'] = 'no author link on the page';
  report['case2'] = out;
  save();
  await ctx.close();
}

async function case3(browser: Browser, owner: BrowserContext) {
  await setMain(owner, 'lounge-editorial');
  const res: Record<Mode, string[]> = { base: [], after: [] };
  for (const mode of ['base', 'after'] as Mode[]) {
    const { ctx, page } = await newPage(browser, owner, 'lounge-editorial', false);
    await go(page, '/u/owner');
    if (mode === 'base') await strip(page);
    res[mode] = await page.evaluate(() =>
      Array.from(document.querySelectorAll<HTMLElement>('main, main *')).map((e) => {
        const r = e.getBoundingClientRect();
        return `${e.tagName.toLowerCase()}.${String(e.className).split(' ')[0]} ${r.x.toFixed(1)},${(r.y + scrollY).toFixed(1)},${r.width.toFixed(1)},${r.height.toFixed(1)}`;
      }),
    );
    if (mode === 'after') await shot(page, '3-profile-page-1280-after');
    await ctx.close();
  }
  const moved: string[] = [];
  res.base.forEach((v, i) => v !== res.after[i] && moved.push(`${v} -> ${res.after[i]}`));
  if (res.base.length !== res.after.length)
    moved.push(`count ${res.base.length} -> ${res.after.length}`);
  report['case3'] = { total: res.base.length, moved };
  say(
    `case3 1280: ${res.base.length} boxes, moved ${moved.length}: ${moved.slice(0, 5).join(' | ')}`,
  );
  save();
  expect(moved).toEqual([]);
}

async function case4(browser: Browser) {
  const { ctx, page } = await newPage(browser, null, 'lounge-light');
  await go(page, '/~owner');
  const blocks = await page.locator('.pp-linktree a').count();
  if (blocks === 0) {
    report['case4'] = 'not tested: the Owner page is not public in this world';
    say(`case4 not tested: /~owner shows ${new URL(page.url()).pathname}, no link blocks`);
    save();
    await ctx.close();
    return;
  }
  const out: Record<string, unknown> = {};
  for (const mode of ['after', 'base'] as Mode[]) {
    if (mode === 'base') {
      await go(page, '/~owner');
      await strip(page);
    }
    const rows = await hitTestAll(page, page.locator('.pp-linktree').first());
    out[mode] = { movers: await movers(page), rows: rows.map(rowOf), env: await envOf(page) };
    if (mode === 'after') {
      await page.locator('.pp-linktree').first().scrollIntoViewIfNeeded();
      await shot(page, '4-public-page-375-after');
    }
  }
  report['case4'] = out;
  say(`case4: ${JSON.stringify(out)}`);
  save();
  await ctx.close();
}

test('p94 profile: link blocks and the guestbook author link', async ({ browser }) => {
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
    const regress = base
      .map((b, i) => ({ b, a: after[i] }))
      .filter(({ b, a }) => b.status === 'pass' && a && a.status !== 'pass')
      .map(({ b, a }) => `${b.key}: ${a!.status} ${JSON.stringify(a!.fails)}`);
    report['regressions'] = regress;
    say(`case1 regressions (passed on base, not after): ${JSON.stringify(regress)}`);
    save();
    await case1(browser, owner, 'after', 'lounge-dark');
    expect(regress).toEqual([]);
  });
  if (!BASE_ONLY) {
    await run('2', () => case2(browser, owner));
    await run('3', () => case3(browser, owner));
    await run('4', () => case4(browser));
  }
  await owner.close();
  report['errors'] = errors;
  save();
  if (errors.length) throw new Error(errors.join('\n'));
});
