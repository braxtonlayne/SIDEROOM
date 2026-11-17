import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, apiPut, loginApi, registerUser, test } from '../support/fixtures';
import { type ControlRow, hitTestAll, openCollapsed } from '../support/hit-test';

/**
 * SEC-AUD-408 (D-924), wave 93 lounge: on a phone the profile popout's Note field, Message field and
 * "View Full Profile" pass the 44 px hit test (D-856). Needs the sweep's seeded world
 * (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`'s "seed the sweep world"; this spec never edits that
 * spec). The Owner's account. Local only. A friend, a one-to-one DM, a group DM and a second guild member are made
 * through the API in the first run (D-863) and kept in `ids.json` in the shots folder, so the base case and the after
 * case measure the same conversations. Every variable starts with SR_P93_LOUNGE_.
 *
 * The commander's command (from `e2e/`, after the world is seeded on the same database; also set SR_CLI_BIN,
 * SR_DATABASE__URL and DATABASE_URL of the stack):
 *   SR_E2E_BASE_URL=http://localhost:5713 SR_P93_LOUNGE_ON=1 \
 *   SR_P93_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-lounge \
 *   npx playwright test tests/p93-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * The base case adds SR_P93_LOUNGE_BASE=1 (run it with `profile-p11.css` restored from the base): files named
 * "base-...", measures and writes lines (no assertions). The after case reads `base-measure.json` in the same folder.
 * `ids.json` in the shots folder keeps the made conversations between runs: delete it on a fresh database.
 *
 * 375x812 (touch, Lounge Light and Dark): the popout opened from a member's name in the group DM panel and from a
 * guild channel's member list (role chips are drawn there): every control by the shared hit test (`openCollapsed`,
 * `hitTestAll`), the popout's box, its inner scroll, sideways scroll; a note typed, a message typed, "View Full
 * Profile" opens the full profile. 1280 Editorial: every box equal to the base. Wave 92's one-to-one card and group
 * panel at 375: still pass, boxes equal to the base.
 */
const ON = process.env.SR_P93_LOUNGE_ON === '1';
const BASE = process.env.SR_P93_LOUNGE_BASE === '1';
const PRE = BASE ? 'base-' : '';
const OUT =
  process.env.SR_P93_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-lounge';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const IDS_FILE = resolve(OUT, 'ids.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

// The base case only measures and shoots: no assertions.
const ex: typeof test.expect = BASE
  ? ((() => {
      const m: unknown = new Proxy(() => m, { get: () => m });
      return m;
    }) as unknown as typeof test.expect)
  : test.expect;

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

const log = (id: string, line: Record<string, unknown>) => {
  mkdirSync(OUT, { recursive: true });
  const row = { id: `${PRE}${id}`, ...line };
  appendFileSync(resolve(OUT, `${PRE}measure.json`), `${JSON.stringify(row)}\n`);
  console.log(`REPORT ${JSON.stringify(row).slice(0, 3000)}`);
};

const baseRow = (id: string): Record<string, unknown> | null => {
  const f = resolve(OUT, 'base-measure.json');
  if (BASE || !existsSync(f)) return null;
  const rows = readFileSync(f, 'utf8')
    .split('\n')
    .filter(Boolean)
    .map((l) => JSON.parse(l) as Record<string, unknown>)
    .filter((r) => r.id === `base-${id}`);
  return rows.at(-1) ?? null;
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

async function tap(page: Page, name: string) {
  await page.getByRole('button', { name, exact: true }).first().tap({ timeout: 5000 });
  await page.waitForTimeout(500);
}

/** A finger swipe (touch events through the DevTools protocol) moves a phone to the member list pane. */
async function swipeToMembers(page: Page) {
  const button = page.getByRole('button', { name: 'Member List' });
  if ((await button.count()) && (await button.getAttribute('aria-pressed')) !== 'true')
    await button.click();
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

interface World {
  guildId: string;
  channelId: string;
}
interface Ids {
  one: string;
  group: string;
  guildMember: string;
}

/** The friends, the one-to-one DM, the group DM and one more guild member: made once, kept for the after case. */
async function conversations(browser: Browser, ownerCtx: BrowserContext): Promise<Ids> {
  mkdirSync(OUT, { recursive: true });
  if (existsSync(IDS_FILE)) return JSON.parse(readFileSync(IDS_FILE, 'utf8')) as Ids;
  const world = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;
  const me = (await (await ownerCtx.request.get('/api/v1/users/@me')).json()) as { id: string };
  const others: string[] = [];
  for (const label of ['bob', 'cat']) {
    const c = await browser.newContext({ baseURL: ORIGIN });
    const u = await registerUser(c.request, label);
    await loginApi(c.request, u);
    await apiPut(ownerCtx.request, `/users/@me/relationships/${u.id}`, { type: 1 });
    await apiPut(c.request, `/users/@me/relationships/${me.id}`, { type: 1 });
    others.push(u.id);
    await c.close();
  }
  const one = await apiPost<{ id: string }>(ownerCtx.request, '/users/@me/channels', {
    recipient_id: others[0],
  });
  const group = await apiPost<{ id: string }>(ownerCtx.request, '/users/@me/channels', {
    recipients: others,
  });
  // A guild member who is not the Owner: the popout from the channel's member list has Note, Message and role chips.
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
  const ids: Ids = { one: one.id, group: group.id, guildMember: du.id };
  writeFileSync(IDS_FILE, JSON.stringify(ids));
  return ids;
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
// Role chips (Add role, a role's remove button) are not this item: they are listed, never fixed here.
const fails = (h: Hit) => h.rows.filter((r) => r.status === 'fail' && !/role/i.test(r.name));
const boxes = (h: Hit) => h.rows.map((r) => [r.name, r.box, r.status]);

const popState = (page: Page) =>
  page.evaluate(() => {
    const p = document.querySelector('.lng-pop') as HTMLElement | null;
    if (!p) return null;
    const r = p.getBoundingClientRect();
    return {
      theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
      box: [r.left, r.top, r.width, r.height].map((n) => Math.round(n * 10) / 10),
      viewport: [window.innerWidth, window.innerHeight],
      bottomInside: r.bottom <= window.innerHeight + 0.5,
      scrolls: p.scrollHeight > p.clientHeight + 1,
      scrollH: p.scrollHeight,
      clientH: p.clientHeight,
      doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      roles: p.querySelectorAll('.lng-pop-role-x, .lng-pop-role-add').length,
    };
  });

/** Taps a member row until the popout that opens has the Note field (a member who is not the Owner). */
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
    if (await page.locator('.lng-pop .lng-pop-input').count()) return i;
    if (phone) await page.getByRole('button', { name: 'Close', exact: true }).first().tap();
    else await page.keyboard.press('Escape');
    await page.waitForTimeout(400);
  }
  throw new Error('probe error: no member row opened a popout with a Note field');
}

async function popoutRound(
  browser: Browser,
  ownerCtx: BrowserContext,
  ids: Ids,
  theme: string,
  phone: boolean,
  from: 'group' | 'guild',
  id: string,
  shotName: string | null,
  flow: boolean,
) {
  const world = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;
  const { ctx, page } = await newPage(browser, phone, ownerCtx, theme);
  try {
    if (from === 'group') {
      await open(page, `/channels/@me/${ids.group}`);
      if (phone) {
        await tap(page, 'More actions');
        await tap(page, 'Member List');
      }
    } else {
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
    }
    const idx = await openPopout(page, phone);
    const pop = await popState(page);
    const h = await hit(page, '.lng-pop');
    log(id, { row: idx, pop, hit: h });
    if (shotName) await shot(page, shotName);
    if (flow) {
      const note = page.locator('.lng-pop .lng-pop-input').first();
      await note.scrollIntoViewIfNeeded();
      await note.tap();
      await note.fill('a note from wave 93');
      await page.waitForTimeout(800);
      const msg = page.locator('.lng-pop .lng-pop-input').nth(1);
      await msg.scrollIntoViewIfNeeded();
      await msg.tap();
      await msg.fill('hello from the popout');
      const typed = [await note.inputValue(), await msg.inputValue()];
      const full = page.locator('.lng-pop .lng-pop-full');
      await full.scrollIntoViewIfNeeded();
      await full.tap();
      await page.waitForTimeout(900);
      const dialogs = await page.getByRole('dialog').count();
      log(`${id}-flow`, { typed, dialogs });
      ex(typed, 'note and message can be typed').toEqual([
        'a note from wave 93',
        'hello from the popout',
      ]);
      ex(dialogs, 'View Full Profile opens the full profile').toBeGreaterThan(0);
    }
    return { pop, h };
  } catch (e) {
    log(id, { ok: false, error: String(e).slice(0, 400) });
    await shot(page, `${id}-failed`).catch(() => undefined);
    throw e;
  } finally {
    await ctx.close().catch(() => undefined);
  }
}

test('popout: the profile popout on a phone (SEC-AUD-408)', async ({ browser }) => {
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const ids = await conversations(browser, ownerCtx);

  for (const [theme, short] of [
    ['lounge-light', 'light'],
    ['lounge-dark', 'dark'],
  ] as const) {
    await setTheme(ownerCtx, theme);
    for (const from of ['group', 'guild'] as const) {
      const r = await popoutRound(
        browser,
        ownerCtx,
        ids,
        theme,
        true,
        from,
        `popout-${from}-phone-${short}`,
        from === 'group' ? `popout-phone-lounge-${short}` : null,
        short === 'light' && from === 'group',
      );
      ex(r.pop?.theme, 'the room is the chosen theme').toBe(theme);
      ex(r.pop?.doc, 'no sideways scroll').toBe(0);
      ex(r.pop?.bottomInside, 'the popout stays inside the viewport').toBe(true);
      ex(fails(r.h), 'every control of the popout passes the hit test').toEqual([]);
      const b = baseRow(`popout-${from}-phone-${short}`) as {
        pop?: { box: number[] };
      } | null;
      if (b?.pop && r.pop)
        log(`popout-${from}-phone-${short}-height-diff`, {
          px: (r.pop.box[3] ?? 0) - (b.pop.box[3] ?? 0),
          scrolls: r.pop.scrolls,
        });
    }
  }

  // 1280 Editorial: every box equal to the base.
  await setTheme(ownerCtx, 'lounge-editorial');
  const d = await popoutRound(
    browser,
    ownerCtx,
    ids,
    'lounge-editorial',
    false,
    'guild',
    'popout-guild-desktop-editorial',
    'popout-desktop-lounge-editorial',
    false,
  );
  const b = baseRow('popout-guild-desktop-editorial') as {
    pop?: { box: number[] };
    hit?: Hit;
  } | null;
  if (b?.hit) {
    ex(boxes(d.h), '1280 control boxes equal the base').toEqual(boxes(b.hit));
    ex(d.pop?.box, '1280 popout box equals the base').toEqual(b.pop?.box);
  }
  await ownerCtx.close();
});

test('regression: wave 92 one-to-one card and group panel at 375 (SEC-AUD-402, 403)', async ({
  browser,
}) => {
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const ids = await conversations(browser, ownerCtx);
  await setTheme(ownerCtx, 'lounge-light');

  const one = await newPage(browser, true, ownerCtx, 'lounge-light');
  try {
    await open(one.page, `/channels/@me/${ids.one}`);
    await tap(one.page, 'More actions');
    await tap(one.page, 'User Profile');
    await one.page.locator('.prf-dm-panel .lng-profile-card').waitFor({ timeout: 8000 });
    await one.page.waitForTimeout(600);
    const h = await hit(one.page, '.lng-right .prf-dm-panel .lng-profile-card');
    log('card-phone-light', { hit: h });
    await shot(one.page, 'card-phone-lounge-light');
    ex(fails(h), 'the card passes').toEqual([]);
    const b = baseRow('card-phone-light') as { hit?: Hit } | null;
    if (b?.hit) ex(boxes(h), 'the card boxes equal the base').toEqual(boxes(b.hit));
  } finally {
    await one.ctx.close().catch(() => undefined);
  }

  const grp = await newPage(browser, true, ownerCtx, 'lounge-light');
  try {
    await open(grp.page, `/channels/@me/${ids.group}`);
    await tap(grp.page, 'More actions');
    await tap(grp.page, 'Member List');
    await grp.page.locator('.lng-right .lng-profile > section').waitFor({ timeout: 8000 });
    await grp.page.waitForTimeout(500);
    const h = await hit(grp.page, '.lng-right .lng-profile');
    log('group-phone-light', { hit: h });
    ex(fails(h), 'the group panel passes').toEqual([]);
    const b = baseRow('group-phone-light') as { hit?: Hit } | null;
    if (b?.hit) ex(boxes(h), 'the group panel boxes equal the base').toEqual(boxes(b.hit));
  } finally {
    await grp.ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
});
