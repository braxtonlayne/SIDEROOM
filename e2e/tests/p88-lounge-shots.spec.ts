import { appendFileSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-380 (wave 88; D-892): "Member List" in a text channel's header strip shows the members pane on a phone.
 * Needs the sweep's seeded world (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`'s "seed the sweep
 * world"; this spec never edits that spec). The Owner's account. Local only. Every variable starts with
 * SR_P88_LOUNGE_.
 *
 * The commander's command (from `e2e/`, after the world is seeded on the same database; also set SR_CLI_BIN,
 * SR_DATABASE__URL and DATABASE_URL of the stack, as for the other specs):
 *   SR_E2E_BASE_URL=http://localhost:5662 SR_P88_LOUNGE_ON=1 \
 *   SR_P88_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p88-lounge \
 *   npx playwright test tests/p88-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * The base case adds SR_P88_LOUNGE_BASE=1: files named "base-...", Lounge Dark on a phone only, measures and writes
 * lines (no assertions).
 *
 * At 375x812 (touch, mobile user agent) in Lounge Dark and Light: open the strip (More), tap Member List; the pane, a
 * member row's box against the viewport, the strip's state, the stored `memberListOpen` before and after; a swipe back
 * to the chat with the composer in view; a reload opens the chat. At 1280 in Editorial: the button toggles the members
 * column, the state survives a reload, the main column's box with the list open (for a 0 px comparison with the base).
 */
const ON = process.env.SR_P88_LOUNGE_ON === '1';
const BASE = process.env.SR_P88_LOUNGE_BASE === '1';
const PRE = BASE ? 'base-' : '';
const OUT =
  process.env.SR_P88_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p88-lounge';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const WORLD_FILE = resolve(STATE, 'world.json');
const OWNER_STATE = resolve(STATE, 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

// The base case only measures and shoots: no assertions.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const ex: typeof test.expect = BASE
  ? ((() => {
      const m: unknown = new Proxy(() => m, { get: () => m });
      return m;
    }) as unknown as typeof test.expect)
  : test.expect;

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };

const log = (id: string, line: Record<string, unknown>) => {
  mkdirSync(OUT, { recursive: true });
  const row = { id: `${PRE}${id}`, ...line };
  appendFileSync(resolve(OUT, `${PRE}measure.json`), `${JSON.stringify(row)}\n`);
  console.log(`REPORT ${JSON.stringify(row).slice(0, 1400)}`);
};

async function setTheme(ownerCtx: BrowserContext, theme: string) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  console.log(`REPORT account theme read back: ${back.settings?.user_prefs?.theme}`);
}

async function newPage(browser: Browser, phone: boolean, base: BrowserContext, theme: string) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: phone ? { width: 375, height: 812 } : { width: 1280, height: 800 },
    storageState: await base.storageState(),
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
  }, theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(6000);
  await page.route(/sweep-preview\.invalid|\/cdn\/external\//, (r: Route) =>
    r.fulfill({ status: 200, contentType: 'image/png', body: Buffer.alloc(0) }),
  );
  return { ctx, page };
}

const idle = async (page: Page, ms = 3000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.waitForTimeout(500);
};
const shot = (page: Page, name: string) =>
  page.screenshot({ path: resolve(OUT, `${PRE}${name}.png`), timeout: 10_000 });

async function open(page: Page, path: string) {
  await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
  await idle(page, 4000);
  await page.locator('.lng-header').waitFor({ timeout: 8000 });
  await page.waitForTimeout(800);
}

/** The pane, the first member row's box against the viewport, the strip, the stored flag. */
const state = (page: Page) =>
  page.evaluate(() => {
    const vw = document.documentElement.clientWidth;
    const vh = document.documentElement.clientHeight;
    const box = (e: Element | null) => {
      if (!e) return null;
      const r = e.getBoundingClientRect();
      return {
        box: [r.left, r.top, r.width, r.height].map(Math.round),
        inside:
          r.width > 0 &&
          r.height > 0 &&
          r.left >= 0 &&
          r.right <= vw &&
          r.top >= 0 &&
          r.bottom <= vh,
      };
    };
    let stored: unknown = null;
    try {
      const raw = JSON.parse(localStorage.getItem('sr.lounge.ui') ?? 'null') as {
        state?: { memberListOpen?: boolean };
      } | null;
      stored = raw?.state?.memberListOpen ?? null;
    } catch {
      stored = 'unreadable';
    }
    const more = document.querySelector('.lng-dm-more');
    return {
      stripOpen: document.querySelector('.lng-dm-extra')?.getAttribute('data-open') ?? null,
      header: box(document.querySelector('.lng-header')),
      pane: document.querySelector('.lng-root')?.getAttribute('data-pane') ?? null,
      memberRow: box(document.querySelector('.lng-right .lng-member')),
      membersPane: box(document.querySelector('.lng-right')),
      composer: box(document.querySelector('.lng-composer, [data-composer], textarea')),
      main: box(document.querySelector('.lng-main')),
      moreExpanded: more?.getAttribute('aria-expanded') ?? null,
      stored,
      doc: document.documentElement.scrollWidth - vw,
      theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    };
  });

async function tap(page: Page, name: string) {
  await page.getByRole('button', { name, exact: true }).first().tap({ timeout: 5000 });
  await page.waitForTimeout(500);
}

/** A real swipe to the right (back to the chat) through Chromium's touch input. */
async function swipeRight(page: Page) {
  const cdp = await page.context().newCDPSession(page);
  const p = (x: number) => [{ x, y: 400, id: 1 }];
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: p(60) });
  for (const x of [120, 200, 280]) {
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: p(x) });
  }
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await page.waitForTimeout(600);
}

async function phone(browser: Browser, ownerCtx: BrowserContext, theme: string, path: string) {
  const id = `phone-${theme}`;
  const { ctx, page } = await newPage(browser, true, ownerCtx, theme);
  try {
    await open(page, path);
    const before = await state(page);
    log(`${id}-before`, before);
    if (theme === 'lounge-dark') await shot(page, `${id}-before`);
    await tap(page, 'More actions');
    log(`${id}-strip-open`, await state(page));
    if (theme === 'lounge-dark') await shot(page, `${id}-strip-open`);
    await tap(page, 'Member List');
    const after = await state(page);
    log(`${id}-after-tap`, after);
    await shot(page, `${id}-after-tap`);
    ex(after.pane, 'the tap shows the members pane').toBe('members');
    ex(after.moreExpanded, 'the strip is closed after the tap').toBe('false');
    ex(after.stripOpen, 'data-open is off after the tap').toBeNull();
    ex(after.memberRow?.inside, 'a member row is inside the viewport').toBe(true);
    ex(after.stored, 'memberListOpen is not written').toBe(before.stored);
    await swipeRight(page);
    const back = await state(page);
    log(`${id}-back`, back);
    await shot(page, `${id}-back`);
    ex(back.pane).toBe('chat');
    ex(back.moreExpanded, 'the strip is closed after the swipe back').toBe('false');
    ex(back.stripOpen).toBeNull();
    ex(back.composer?.inside, 'the composer is in view').toBe(true);
    await page.reload({ waitUntil: 'domcontentloaded' });
    await idle(page, 3000);
    const reloaded = await state(page);
    log(`${id}-reload`, reloaded);
    ex(reloaded.pane, 'a reload opens the chat').toBe('chat');
  } catch (e) {
    log(id, { ok: false, error: String(e).slice(0, 300) });
    await shot(page, `${id}-failed`).catch(() => undefined);
    throw e;
  } finally {
    await ctx.close().catch(() => undefined);
  }
}

async function desktop(browser: Browser, ownerCtx: BrowserContext, path: string) {
  const { ctx, page } = await newPage(browser, false, ownerCtx, 'lounge-editorial');
  try {
    await open(page, path);
    const btn = page.getByRole('button', { name: 'Member List', exact: true });
    const read = async () => ({
      ...(await state(page)),
      pressed: await btn.getAttribute('aria-pressed'),
    });
    const openState = await read();
    log('desktop-open', openState);
    await shot(page, 'desktop-editorial-open');
    await btn.click();
    await page.waitForTimeout(500);
    const closed = await read();
    log('desktop-closed', closed);
    await shot(page, 'desktop-editorial-closed');
    await page.reload({ waitUntil: 'domcontentloaded' });
    await idle(page, 3000);
    const reloaded = await read();
    log('desktop-closed-after-reload', reloaded);
    await btn.click();
    await page.waitForTimeout(500);
    log('desktop-reopened', await read());
    await page.reload({ waitUntil: 'domcontentloaded' });
    await idle(page, 3000);
    const reopenedReload = await read();
    log('desktop-open-after-reload', reopenedReload);
    ex(closed.membersPane).toBeNull();
    ex(closed.stored).toBe(false);
    ex(reloaded.membersPane).toBeNull();
    ex(reopenedReload.membersPane).not.toBeNull();
    ex(reopenedReload.main?.box).toEqual(openState.main?.box);
  } finally {
    await ctx.close().catch(() => undefined);
  }
}

test('SEC-AUD-380: Member List closes the phone header strip', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  const path = `/channels/${w.guildId}/${w.channelId}`;
  await setTheme(ownerCtx, 'lounge-dark');
  await phone(browser, ownerCtx, 'lounge-dark', path);
  if (!BASE) {
    await setTheme(ownerCtx, 'lounge-light');
    await phone(browser, ownerCtx, 'lounge-light', path);
  }
  await setTheme(ownerCtx, 'lounge-editorial');
  await desktop(browser, ownerCtx, path);
  await ownerCtx.close();
});
