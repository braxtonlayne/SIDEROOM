import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, loginApi, registerUser, test } from '../support/fixtures';
import { type ControlRow, hitTestAll, openCollapsed } from '../support/hit-test';

/**
 * SEC-AUD-426 (D-932), wave 94 lounge: on a phone the guild popout's "Add role" button and a role's remove button
 * pass the 44 px hit test (D-856). Needs the sweep's seeded world (`e2e/.state/sweep/world.json`, made by
 * `page-sweep.spec.ts`'s "seed the sweep world"; this spec never edits that spec). The Owner's account. Local only.
 * A second guild member (`dan`) and three roles are made through the API in the first run (D-863) and kept in
 * `ids.json` in the shots folder: two roles (14 characters each) are given to dan, a third is left to add. The roles
 * and dan stay in the database (the spec has no delete helper). Every variable starts with SR_P94_LOUNGE_.
 *
 * The commander's command (from `e2e/`, after the world is seeded on the same database; also set SR_CLI_BIN,
 * SR_DATABASE__URL and DATABASE_URL of the stack):
 *   SR_E2E_BASE_URL=http://localhost:5721 SR_P94_LOUNGE_ON=1 \
 *   SR_P94_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-lounge \
 *   npx playwright test tests/p94-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * Delete `ids.json` in the shots folder on a fresh database. The base case is taken in the same run: the page's
 * loaded sheet has the SEC-AUD-426 media rule switched off (`mediaText = 'not all'`), the same tree. Setting
 * SR_P94_LOUNGE_BASE=1 makes the after case skip its assertions too (figures only).
 *
 * 375x812 (touch, Lounge Light base and after, Lounge Dark after): dan's popout from the channel's member list:
 * every control by the shared hit test; the chips' lines and the gap; the popout's box; sideways scroll. After:
 * "Add role" opens its menu (menu items are listed, never ruled) and choosing a role adds it; a tap at the centre of
 * the first chip's remove button removes that role and the second stays. 1280 Editorial: every box equal to the base.
 */
const ON = process.env.SR_P94_LOUNGE_ON === '1';
const NOASSERT = process.env.SR_P94_LOUNGE_BASE === '1';
const OUT =
  process.env.SR_P94_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-lounge';
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

/** The base case: the same tree with the SEC-AUD-426 media rule off (found by its selector in the loaded sheets). */
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
          rule.cssText.includes('.lng-pop .lng-pop-role-add') &&
          rule.media.mediaText.includes('768')
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

/** Every control of a root by the shared tool: box, status, taker. A not-reached row is a spec error. */
async function hit(page: Page, rootSel: string) {
  const root = page.locator(rootSel).first();
  const opened = await openCollapsed(page, root);
  const rows: ControlRow[] = await hitTestAll(page, root);
  const out = rows.map((r) => ({
    name: r.name,
    status: r.status,
    box: r.result ? [r1(r.result.box.width), r1(r.result.box.height)] : null,
    takers: (r.result?.points ?? [])
      .filter((p) => !p.ok)
      .map(
        (p) => `${p.at}: ${p.taker?.tag}.${p.taker?.path.split(' < ')[0] ?? ''} "${p.taker?.name}"`,
      ),
    reason: r.reason ?? null,
  }));
  const notReached = out.filter((r) => r.status === 'not-reached');
  if (notReached.length)
    throw new Error(`probe error (not reached): ${JSON.stringify(notReached)}`);
  return { opened: opened.length, rows: out };
}
type Hit = Awaited<ReturnType<typeof hit>>;
const fails = (h: Hit) => h.rows.filter((r) => r.status === 'fail');
const boxes = (h: Hit) => h.rows.map((r) => [r.name, r.box, r.status]);

const popState = (page: Page) =>
  page.evaluate(() => {
    const p = document.querySelector('.lng-pop') as HTMLElement | null;
    if (!p) return null;
    const r = p.getBoundingClientRect();
    const chips = Array.from(p.querySelectorAll('.lng-pop-roles > li')).map((li) => {
      const b = li.getBoundingClientRect();
      return { text: li.textContent, top: b.top, bottom: b.bottom };
    });
    const lines = Array.from(new Set(chips.map((c) => Math.round(c.top))));
    const gaps: number[] = [];
    for (let i = 1; i < chips.length; i++)
      if (Math.round(chips[i]!.top) !== Math.round(chips[i - 1]!.top))
        gaps.push(Math.round((chips[i]!.top - chips[i - 1]!.bottom) * 10) / 10);
    return {
      theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
      box: [r.left, r.top, r.width, r.height].map((n) => Math.round(n * 10) / 10),
      viewport: [window.innerWidth, window.innerHeight],
      bottomInside: r.bottom <= window.innerHeight + 0.5,
      scrolls: p.scrollHeight > p.clientHeight + 1,
      doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      chips: chips.map((c) => c.text),
      chipLines: lines.length,
      lineGaps: gaps,
      removeButtons: p.querySelectorAll('.lng-pop-role-x').length,
      addButtons: p.querySelectorAll('.lng-pop-role-add').length,
    };
  });

/** Taps member rows until the popout that opens is dan's (it has role chips with remove buttons). */
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

async function enterPopout(page: Page, phone: boolean, base: boolean) {
  const world = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;
  await open(page, `/channels/${world.guildId}/${world.channelId}`);
  const off = base ? await switchRuleOff(page) : 0;
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
  return off;
}

async function round(
  browser: Browser,
  ownerCtx: BrowserContext,
  theme: string,
  phone: boolean,
  base: boolean,
  id: string,
  shotName: string,
) {
  const { ctx, page } = await newPage(browser, phone, ownerCtx, theme);
  try {
    const off = await enterPopout(page, phone, base);
    const pop = await popState(page);
    const h = await hit(page, '.lng-pop');
    log(id, { rulesSwitchedOff: off, pop, hit: h });
    await shot(page, shotName);
    return { pop, h };
  } catch (e) {
    log(id, { ok: false, error: String(e).slice(0, 400) });
    await shot(page, `${id}-failed`).catch(() => undefined);
    throw e;
  } finally {
    await ctx.close().catch(() => undefined);
  }
}

test('role chips: the guild popout on a phone (SEC-AUD-426)', async ({ browser }) => {
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const ids = await makeState(browser, ownerCtx);
  await resetRoles(ownerCtx, ids);

  // Base (rule off) and after, 375 touch, Lounge Light.
  await setTheme(ownerCtx, 'lounge-light');
  const base = await round(
    browser,
    ownerCtx,
    'lounge-light',
    true,
    true,
    'base-phone-light',
    'base-popout-phone-lounge-light',
  );
  const after = await round(
    browser,
    ownerCtx,
    'lounge-light',
    true,
    false,
    'after-phone-light',
    'popout-phone-lounge-light',
  );
  log('height-diff-light', {
    px: (after.pop?.box[3] ?? 0) - (base.pop?.box[3] ?? 0),
    scrollsAfter: after.pop?.scrolls,
  });
  ex(after.pop?.theme, 'the room is Lounge Light').toBe('lounge-light');
  ex(after.pop?.removeButtons, 'two remove buttons').toBe(2);
  ex(after.pop?.addButtons, 'one Add role').toBe(1);
  ex(after.pop?.doc, 'no sideways scroll').toBe(0);
  ex(after.pop?.bottomInside, 'the popout stays inside the viewport').toBe(true);
  ex(fails(after.h), 'every control of the popout passes the hit test').toEqual([]);

  // Lounge Dark after.
  await setTheme(ownerCtx, 'lounge-dark');
  const dark = await round(
    browser,
    ownerCtx,
    'lounge-dark',
    true,
    false,
    'after-phone-dark',
    'popout-phone-lounge-dark',
  );
  ex(dark.pop?.theme, 'the room is Lounge Dark').toBe('lounge-dark');
  ex(dark.pop?.doc, 'no sideways scroll').toBe(0);
  ex(fails(dark.h), 'every control passes (dark)').toEqual([]);

  // The flow, 375 Light after: the menu, adding a role, removing the first chip's role.
  await setTheme(ownerCtx, 'lounge-light');
  const f = await newPage(browser, true, ownerCtx, 'lounge-light');
  try {
    await enterPopout(f.page, true, false);
    const add = f.page.locator('.lng-pop .lng-pop-role-add');
    await add.scrollIntoViewIfNeeded();
    await add.tap();
    await f.page.waitForTimeout(600);
    const menu = f.page.getByRole('menu').first();
    await menu.waitFor({ timeout: 5000 });
    // The kit's menu items are listed, never ruled (and not asserted).
    const menuHit = await hit(f.page, '[role="menu"]');
    log('add-role-menu-items', { kit: true, hit: menuHit });
    await shot(f.page, 'add-role-menu-phone-lounge-light');
    const items = f.page.getByRole('menuitem');
    const pick = items.filter({ hasText: 'Spare Keepers' }).first();
    const label = (await pick.textContent())?.trim();
    log('menu-item-boxes', {
      kit: true,
      boxes: await items.evaluateAll((els) =>
        els.map((e) => [e.textContent, Math.round(e.getBoundingClientRect().height * 10) / 10]),
      ),
    });
    await pick.tap();
    await f.page.waitForTimeout(900);
    const afterAdd = await heldRoles(ownerCtx, ids);
    log('flow-add', { label, held: afterAdd.length, has3rd: afterAdd.includes(ids.roles[2]) });
    ex(afterAdd.includes(ids.roles[2]), 'choosing the role adds it').toBe(true);

    // The first chip's remove button, tapped at its box centre (a real touch).
    const x = f.page.locator('.lng-pop .lng-pop-role-x').first();
    await x.scrollIntoViewIfNeeded();
    const box = await x.boundingBox();
    if (!box) throw new Error('probe error: no box for the first remove button');
    await f.page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
    await f.page.waitForTimeout(900);
    const afterRemove = await heldRoles(ownerCtx, ids);
    log('flow-remove', {
      held: afterRemove.length,
      firstGone: !afterRemove.includes(ids.roles[0]),
      secondStays: afterRemove.includes(ids.roles[1]),
    });
    ex(afterRemove.includes(ids.roles[0]), 'the first chip role is removed').toBe(false);
    ex(afterRemove.includes(ids.roles[1]), 'the second chip role stays').toBe(true);
  } finally {
    await f.ctx.close().catch(() => undefined);
    await resetRoles(ownerCtx, ids);
  }

  // 1280 Editorial: every box equal to the base (the rule is a phone rule).
  await setTheme(ownerCtx, 'lounge-editorial');
  const dBase = await round(
    browser,
    ownerCtx,
    'lounge-editorial',
    false,
    true,
    'base-desktop-editorial',
    'base-popout-desktop-lounge-editorial',
  );
  const dAfter = await round(
    browser,
    ownerCtx,
    'lounge-editorial',
    false,
    false,
    'after-desktop-editorial',
    'popout-desktop-lounge-editorial',
  );
  ex(boxes(dAfter.h), '1280 control boxes equal the base').toEqual(boxes(dBase.h));
  ex(dAfter.pop?.box, '1280 popout box equals the base').toEqual(dBase.pop?.box);
  await ownerCtx.close();
});
