import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, loginApi, registerUser, test } from '../support/fixtures';
import { hitTest, ProbeError } from '../support/hit-test';

/**
 * SEC-AUD-430 (D-938), wave 95 menu: on a phone (max-width 720px, the kit's own condition) a kit menu item is at
 * least 44 px tall. Needs the sweep's seeded world (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`
 * "seed the sweep world"; this spec never edits that spec). The Owner's account. Local only. A second guild member
 * (`dan`) and three roles are made through the API in the first run (D-863) and kept in `ids.json` in the shots
 * folder (delete it on a fresh database). Every variable starts with SR_P95_MENU_.
 *
 * The commander's command (from `e2e/`, after the world is seeded on the same database; also set SR_CLI_BIN,
 * SR_DATABASE__URL and DATABASE_URL of the stack):
 *   SR_E2E_BASE_URL=http://localhost:5731 SR_P95_MENU_ON=1 \
 *   SR_P95_MENU_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p95-menu \
 *   npx playwright test tests/p95-menu-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * The base case is taken in the same run: the loaded sheet's SEC-AUD-430 media rule is switched off
 * (`mediaText = 'not all'`), the same tree. SR_P95_MENU_BASE=1 makes the after case skip its assertions (figures only).
 *
 * 375x812 touch: the guild popout's "Add role" menu and a message's "More" menu (the long one): every item's box,
 * the shared `hitTest` on each `[role=menuitem]` (hitTestAll takes buttons, not menu items: wrapping, not a private
 * probe), the menu against the viewport, sideways scroll. Lounge Light base and after, Lounge Dark after. After:
 * choosing a role adds it; a harmless item of the long menu works. 1280 Editorial: item boxes equal to the base.
 */
const ON = process.env.SR_P95_MENU_ON === '1';
const NOASSERT = process.env.SR_P95_MENU_BASE === '1';
const OUT =
  process.env.SR_P95_MENU_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p95-menu';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const IDS_FILE = resolve(OUT, 'ids.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

const ex: typeof test.expect = NOASSERT
  ? ((() => {
      const m: unknown = new Proxy(() => m, { get: () => m });
      return m;
    }) as unknown as typeof test.expect)
  : test.expect;

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

const log = (id: string, line: Record<string, unknown>) => {
  mkdirSync(OUT, { recursive: true });
  const row = { id, ...line };
  appendFileSync(resolve(OUT, 'measure.json'), `${JSON.stringify(row)}\n`);
  console.log(`REPORT ${JSON.stringify(row).slice(0, 3500)}`);
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
  page.screenshot({ path: resolve(OUT, `${name}.png`), timeout: 10_000 });

async function open(page: Page, path: string) {
  await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
  await idle(page, 4000);
  await page.locator('.lng-header').waitFor({ timeout: 8000 });
  await page.waitForTimeout(800);
}

async function tap(page: Page, name: string) {
  await page.getByRole('button', { name, exact: true }).first().tap({ timeout: 5000 });
  await page.waitForTimeout(500);
}

/** A finger swipe (touch events through the DevTools protocol) moves a phone to the member list pane. */
async function swipeToMembers(page: Page) {
  const cdp = await page.context().newCDPSession(page);
  const touch = (type: 'touchStart' | 'touchMove' | 'touchEnd', x: number) =>
    cdp.send('Input.dispatchTouchEvent', {
      type,
      touchPoints: type === 'touchEnd' ? [] : [{ x, y: 400 }],
    });
  await touch('touchStart', 330);
  for (const x of [280, 220, 160, 100, 50]) await touch('touchMove', x);
  await touch('touchEnd', 50);
  await cdp.detach();
  await page.waitForTimeout(600);
}

/** The base case: the same tree with the SEC-AUD-430 media rule off (found in the loaded sheets by its declaration). */
const switchRuleOff = (page: Page) =>
  page.evaluate(() => {
    let off = 0;
    for (const sheet of Array.from(document.styleSheets)) {
      let rules: CSSRuleList;
      try {
        rules = sheet.cssRules;
      } catch {
        continue;
      }
      for (const rule of Array.from(rules)) {
        if (
          rule instanceof CSSMediaRule &&
          rule.media.mediaText.includes('720') &&
          /_item_/.test(rule.cssText) &&
          rule.cssText.includes('min-block-size: 44px')
        ) {
          rule.media.mediaText = 'not all';
          off++;
        }
      }
    }
    return off;
  });

interface World {
  guildId: string;
  channelId: string;
}
interface Ids {
  member: string;
  roles: [string, string, string];
}

/** dan joins the sweep guild; three roles with 14-character names; two go to dan, the third is left to add. */
async function makeState(browser: Browser, ownerCtx: BrowserContext): Promise<Ids> {
  mkdirSync(OUT, { recursive: true });
  if (existsSync(IDS_FILE)) return JSON.parse(readFileSync(IDS_FILE, 'utf8')) as Ids;
  const world = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;
  const invite = await apiPost<{ code: string }>(
    ownerCtx.request,
    `/channels/${world.channelId}/invites`,
    {},
  );
  const dc = await browser.newContext({ baseURL: ORIGIN });
  const du = await registerUser(dc.request, 'dan');
  await loginApi(dc.request, du);
  await apiPost(dc.request, `/invites/${invite.code}`, {});
  await dc.close();
  const roles: string[] = [];
  for (const name of ['Garden Wardens', 'Night Stewards', 'Spare Keepers']) {
    const r = await apiPost<{ id: string }>(ownerCtx.request, `/guilds/${world.guildId}/roles`, {
      name,
    });
    roles.push(r.id);
  }
  await apiPatch(ownerCtx.request, `/guilds/${world.guildId}/members/${du.id}`, {
    roles: [roles[0], roles[1]],
  });
  const ids = { member: du.id, roles } as Ids;
  writeFileSync(IDS_FILE, JSON.stringify(ids));
  return ids;
}

/** Puts dan back to exactly the first two roles (after the flow changed them). */
async function resetRoles(ownerCtx: BrowserContext, ids: Ids) {
  const world = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;
  await apiPatch(ownerCtx.request, `/guilds/${world.guildId}/members/${ids.member}`, {
    roles: [ids.roles[0], ids.roles[1]],
  });
}

async function heldRoles(ownerCtx: BrowserContext, ids: Ids): Promise<string[]> {
  const world = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;
  const res = await ownerCtx.request.get(`/api/v1/guilds/${world.guildId}/members/${ids.member}`);
  const m = (await res.json()) as { roles?: string[]; role_ids?: string[] };
  return m.roles ?? m.role_ids ?? [];
}

const r1 = (n: number) => Math.round(n * 10) / 10;
async function openPopout(page: Page, phone: boolean) {
  const rows = page.locator('.lng-member, .prf-member-main');
  const n = await rows.count();
  for (let i = 0; i < n; i++) {
    const row = rows.nth(i);
    await row.scrollIntoViewIfNeeded();
    if (phone) await row.tap();
    else await row.click();
    await page.locator('.lng-pop').first().waitFor({ timeout: 6000 });
    await page.waitForTimeout(700);
    if (await page.locator('.lng-pop .lng-pop-role-x').count()) return i;
    if (phone) await page.getByRole('button', { name: 'Close', exact: true }).first().tap();
    else await page.keyboard.press('Escape');
    await page.waitForTimeout(400);
  }
  throw new Error('probe error: no member row opened a popout with role chips');
}

async function enterPopout(page: Page, phone: boolean) {
  const world = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;
  await open(page, `/channels/${world.guildId}/${world.channelId}`);
  if (phone) {
    await tap(page, 'More actions');
    await tap(page, 'Member List');
    await page.waitForTimeout(500);
    if (
      !(await page
        .locator('.lng-member')
        .first()
        .isVisible()
        .catch(() => false))
    )
      await swipeToMembers(page);
  }
  await openPopout(page, phone);
}

async function openRoleMenu(page: Page, phone: boolean) {
  await enterPopout(page, phone);
  const add = page.locator('.lng-pop .lng-pop-role-add');
  await add.scrollIntoViewIfNeeded();
  if (phone) await add.tap();
  else await add.click();
  await page.waitForTimeout(600);
  await page.getByRole('menu').first().waitFor({ timeout: 5000 });
}

/** The message toolbar's "More" menu (a dropdown), opened by tap or click. */
async function openLongMenu(page: Page, phone: boolean) {
  const world = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;
  await open(page, `/channels/${world.guildId}/${world.channelId}`);
  const art = page.locator('article.lng-msg:has([role=toolbar])').last();
  await art.scrollIntoViewIfNeeded();
  await art.hover();
  await page.waitForTimeout(400);
  const more = art.locator('button[aria-label="More"]');
  if (phone) await more.tap();
  else await more.click();
  await page.waitForTimeout(600);
  await page.getByRole('menu').first().waitFor({ timeout: 5000 });
}

/** Every menu item: its box, the shared hit test, the taker of a failing point; the menu against the viewport. */
async function menuState(page: Page) {
  const menu = page.getByRole('menu').first();
  const items = menu.getByRole('menuitem');
  const n = await items.count();
  const rows: { label: string; box: number[]; status: string; takers: string[] }[] = [];
  for (let i = 0; i < n; i++) {
    const it = items.nth(i);
    const label = ((await it.textContent()) ?? '').trim().slice(0, 40);
    try {
      const r = await hitTest(page, it);
      rows.push({
        label,
        box: [r1(r.box.width), r1(r.box.height)],
        status: r.pass ? 'pass' : 'fail',
        takers: r.points
          .filter((p) => !p.ok)
          .map(
            (p) =>
              `${p.at}: ${p.taker?.tag}.${p.taker?.path.split(' < ')[0] ?? ''} "${p.taker?.name}"`,
          ),
      });
    } catch (e) {
      if (!(e instanceof ProbeError)) throw e;
      rows.push({ label, box: [], status: 'not-reached', takers: [String(e).slice(0, 120)] });
    }
  }
  await menu.evaluate((m) => m.scrollTo(0, 0));
  const env = await menu.evaluate((m) => {
    const b = m.getBoundingClientRect();
    return {
      theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
      box: [b.left, b.top, b.width, b.height].map((x) => Math.round(x * 10) / 10),
      viewport: [window.innerWidth, window.innerHeight],
      inside:
        b.left >= -0.5 &&
        b.top >= -0.5 &&
        b.right <= window.innerWidth + 0.5 &&
        b.bottom <= window.innerHeight + 0.5,
      scrollsInside: m.scrollHeight > m.clientHeight + 1,
      doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    };
  });
  return { n, rows, ...env };
}
type MS = Awaited<ReturnType<typeof menuState>>;
const bad = (m: MS) => m.rows.filter((r) => r.status !== 'pass');
const short = (m: MS) => m.rows.filter((r) => (r.box[1] ?? 0) < 44);

interface Run {
  theme: string;
  phone: boolean;
  base: boolean;
  id: string;
  shotName?: string;
  long?: boolean;
}

async function run(browser: Browser, ownerCtx: BrowserContext, o: Run) {
  const { ctx, page } = await newPage(browser, o.phone, ownerCtx, o.theme);
  try {
    if (o.long) await openLongMenu(page, o.phone);
    else await openRoleMenu(page, o.phone);
    const off = o.base ? await switchRuleOff(page) : 0;
    await page.waitForTimeout(300);
    const ms = await menuState(page);
    log(o.id, { rulesSwitchedOff: off, ...ms });
    if (o.base) ex(off, 'the rule was found and switched off').toBeGreaterThan(0);
    if (o.shotName) await shot(page, o.shotName);
    return ms;
  } catch (e) {
    log(o.id, { ok: false, error: String(e).slice(0, 400) });
    await shot(page, `${o.id}-failed`).catch(() => undefined);
    throw e;
  } finally {
    await ctx.close().catch(() => undefined);
  }
}

test('kit menu items on a phone (SEC-AUD-430)', async ({ browser }) => {
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const ids = await makeState(browser, ownerCtx);
  await resetRoles(ownerCtx, ids);
  const P = { phone: true, base: false } as const;

  await setTheme(ownerCtx, 'lounge-light');
  const base = await run(browser, ownerCtx, {
    ...P,
    theme: 'lounge-light',
    base: true,
    id: 'base-role-light',
    shotName: 'role-menu-375-light-base',
  });
  const after = await run(browser, ownerCtx, {
    ...P,
    theme: 'lounge-light',
    id: 'after-role-light',
    shotName: 'role-menu-375-light-after',
  });
  const baseLong = await run(browser, ownerCtx, {
    ...P,
    theme: 'lounge-light',
    base: true,
    long: true,
    id: 'base-long-light',
  });
  const afterLong = await run(browser, ownerCtx, {
    ...P,
    theme: 'lounge-light',
    long: true,
    id: 'after-long-light',
    shotName: 'long-menu-375-light-after',
  });
  log('summary-light', {
    roleBase: base.rows.map((r) => r.box[1]),
    roleAfter: after.rows.map((r) => r.box[1]),
    longBase: baseLong.rows.map((r) => r.box[1]),
    longAfter: afterLong.rows.map((r) => r.box[1]),
  });
  for (const [name, m] of [
    ['role', after],
    ['long', afterLong],
  ] as const) {
    ex(m.theme, 'Lounge Light').toBe('lounge-light');
    ex(m.n, `${name} menu has items`).toBeGreaterThan(0);
    ex(short(m), `${name}: every item 44 px or more`).toEqual([]);
    ex(bad(m), `${name}: every item passes the hit test`).toEqual([]);
    ex(m.doc, 'no sideways scroll').toBe(0);
    ex(m.inside || m.scrollsInside, `${name}: inside the viewport or scrolls`).toBe(true);
  }

  await setTheme(ownerCtx, 'lounge-dark');
  const dark = await run(browser, ownerCtx, {
    ...P,
    theme: 'lounge-dark',
    id: 'after-role-dark',
    shotName: 'role-menu-375-dark-after',
  });
  ex(dark.theme, 'Lounge Dark').toBe('lounge-dark');
  ex(short(dark), 'dark: every item 44 px or more').toEqual([]);
  ex(bad(dark), 'dark: every item passes').toEqual([]);
  ex(dark.doc, 'no sideways scroll').toBe(0);

  // 1280 Editorial: item boxes equal to the base (the rule is a phone rule).
  await setTheme(ownerCtx, 'lounge-editorial');
  const dBase = await run(browser, ownerCtx, {
    theme: 'lounge-editorial',
    phone: false,
    base: true,
    id: 'base-role-desktop',
  });
  const dAfter = await run(browser, ownerCtx, {
    theme: 'lounge-editorial',
    phone: false,
    base: false,
    id: 'after-role-desktop',
    shotName: 'role-menu-1280-after',
  });
  ex(
    dAfter.rows.map((r) => r.box),
    '1280 item boxes equal the base',
  ).toEqual(dBase.rows.map((r) => r.box));

  // The flows, 375 Light after: add the role through the menu; one harmless item of the long menu.
  await setTheme(ownerCtx, 'lounge-light');
  const f = await newPage(browser, true, ownerCtx, 'lounge-light');
  try {
    await openRoleMenu(f.page, true);
    const pick = f.page.getByRole('menuitem').filter({ hasText: 'Spare Keepers' }).first();
    await pick.tap();
    await f.page.waitForTimeout(900);
    const held = await heldRoles(ownerCtx, ids);
    log('flow-add', { has3rd: held.includes(ids.roles[2]) });
    ex(held.includes(ids.roles[2]), 'choosing the role adds it').toBe(true);
  } finally {
    await f.ctx.close().catch(() => undefined);
    await resetRoles(ownerCtx, ids);
  }
  const g = await newPage(browser, true, ownerCtx, 'lounge-light');
  try {
    await openLongMenu(g.page, true);
    const items = g.page.getByRole('menu').first().getByRole('menuitem');
    const labels = await items.allTextContents();
    log('long-menu-labels', { labels });
    const harmless = items.filter({ hasText: /^(Copy|Mark Unread)/ }).first();
    if (await harmless.count()) {
      await harmless.tap();
      await g.page.waitForTimeout(600);
      const closed = (await g.page.getByRole('menu').count()) === 0;
      log('flow-long', { closed });
      ex(closed, 'choosing a harmless item closes the menu').toBe(true);
    } else log('flow-long', { note: 'no harmless item by label', labels });
  } finally {
    await g.ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
});
