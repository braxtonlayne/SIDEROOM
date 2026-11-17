import { appendFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, apiPut, loginApi, registerUser, test } from '../support/fixtures';

/**
 * SEC-AUD-385 and SEC-AUD-386 (wave 90; D-899): on a phone "Member List" in a voice channel's and a stage channel's
 * header shows the members pane, and the identity tap of a group DM shows the pane without writing the stored
 * `memberListOpen`. Needs the sweep's seeded world (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`'s
 * "seed the sweep world"; this spec never edits that spec). The Owner's account. Local only. A stage channel and a
 * group DM the world does not hold are made through the API in this run (D-863). Every variable starts with
 * SR_P90_LOUNGE_.
 *
 * The commander's command (from `e2e/`, after the world is seeded on the same database; also set SR_CLI_BIN,
 * SR_DATABASE__URL and DATABASE_URL of the stack, as for the other specs). Two tests: "voice" (item 1) and "dm"
 * (item 2); `-g voice` or `-g dm` runs one:
 *   SR_E2E_BASE_URL=http://localhost:5682 SR_P90_LOUNGE_ON=1 \
 *   SR_P90_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-lounge \
 *   npx playwright test tests/p90-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * The base case adds SR_P90_LOUNGE_BASE=1 (run it with the change switched off): files named "base-...", Lounge Dark on
 * a phone and Editorial at 1280, measures and writes lines (no assertions). The after case reads `base-measure.json`
 * in the same folder and compares the 1280 boxes with it (0 px).
 *
 * At 375x812 (touch, mobile user agent) in Lounge Dark and Light: open the strip (More), tap Member List (a 1:1 DM's
 * button is "User Profile"); the pane, a member row's box against the viewport, the strip's state, the stored flag
 * before and after; a real touch swipe back to the view with its controls in view. At 1280 in Editorial: the button
 * (or the identity tap) as before, the header's and the view's boxes.
 */
const ON = process.env.SR_P90_LOUNGE_ON === '1';
const BASE = process.env.SR_P90_LOUNGE_BASE === '1';
const PRE = BASE ? 'base-' : '';
const OUT =
  process.env.SR_P90_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-lounge';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const WORLD_FILE = resolve(STATE, 'world.json');
const OWNER_STATE = resolve(STATE, 'owner.json');
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

const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };

const log = (id: string, line: Record<string, unknown>) => {
  mkdirSync(OUT, { recursive: true });
  const row = { id: `${PRE}${id}`, ...line };
  appendFileSync(resolve(OUT, `${PRE}measure.json`), `${JSON.stringify(row)}\n`);
  console.log(`REPORT ${JSON.stringify(row).slice(0, 1400)}`);
};

/** The base case's logged row for `id` (the last one), or null. */
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

/** The pane, the first member row's box against the viewport, the strip, the stored flag, the view's boxes. */
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
    const right = document.querySelector('.lng-right');
    return {
      stripOpen: document.querySelector('.lng-dm-extra')?.getAttribute('data-open') ?? null,
      header: box(document.querySelector('.lng-header')),
      pane: document.querySelector('.lng-root')?.getAttribute('data-pane') ?? null,
      memberRow: box(document.querySelector('.lng-right .lng-member')),
      membersPane: box(right),
      rightLabel: right?.getAttribute('aria-label') ?? null,
      rightText: (right?.textContent ?? '').replace(/\s+/g, ' ').slice(0, 90),
      view: box(document.querySelector('.lng-channel-view')),
      // What the view's controls are: the call toolbar, the composer.
      controls: box(document.querySelector('.vc-view [role="toolbar"], .lng-composer, textarea')),
      joinBtn: box(
        Array.from(document.querySelectorAll('.vc-view button')).find((b) =>
          /^Join/.test(b.textContent ?? ''),
        ) ?? null,
      ),
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
/** Opens `path` with the stored `memberListOpen` set to false (the default is true, which hides a write of true). */
async function openStoredFalse(page: Page, path: string) {
  await open(page, path);
  await page.evaluate(() => {
    const raw = JSON.parse(localStorage.getItem('sr.lounge.ui') ?? 'null') as {
      state?: Record<string, unknown>;
    } | null;
    if (raw?.state) {
      raw.state.memberListOpen = false;
      localStorage.setItem('sr.lounge.ui', JSON.stringify(raw));
    }
  });
  await page.reload({ waitUntil: 'domcontentloaded' });
  await idle(page, 3000);
  await page.locator('.lng-header').waitFor({ timeout: 8000 });
  await page.waitForTimeout(800);
}
const tapRe = async (page: Page, name: RegExp) => {
  await page.getByRole('button', { name }).first().tap({ timeout: 5000 });
  await page.waitForTimeout(500);
};

/** A real swipe to the right (back to the view) through Chromium's touch input. */
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

interface Chan {
  id: string;
  name: string;
  type: number;
}

/** The first channel of `type` in the guild; made through the API when the world has none (D-863). */
async function channelOfType(
  ownerCtx: BrowserContext,
  guildId: string,
  type: number,
  name: string,
) {
  const list = async () =>
    (await (await ownerCtx.request.get(`/api/v1/guilds/${guildId}/channels`)).json()) as Chan[];
  const found = (await list()).find((c) => c.type === type);
  if (found) return found.id;
  const made = await apiPost<{ id: string }>(ownerCtx.request, `/guilds/${guildId}/channels`, {
    name,
    type,
  });
  return made.id;
}

/** One phone round: the header's More, then the named button; the pane, the strip, the stored flag, the swipe back. */
async function phoneRound(
  browser: Browser,
  ownerCtx: BrowserContext,
  theme: string,
  path: string,
  id: string,
  button: string,
  shots: { afterTap?: boolean; back?: boolean },
) {
  const { ctx, page } = await newPage(browser, true, ownerCtx, theme);
  try {
    await open(page, path);
    const before = await state(page);
    log(`${id}-before`, before);
    await tap(page, 'More actions');
    await tap(page, button);
    const after = await state(page);
    log(`${id}-after-tap`, after);
    if (shots.afterTap) await shot(page, `${id}-after-tap`);
    ex(after.pane, 'the tap shows the members pane').toBe('members');
    ex(after.moreExpanded, 'the strip is closed after the tap').toBe('false');
    ex(after.membersPane?.inside, 'the pane is inside the viewport').toBe(true);
    ex(after.stored, 'memberListOpen is not written').toBe(before.stored);
    ex(after.doc, 'no sideways scroll').toBe(0);
    await swipeRight(page);
    const back = await state(page);
    log(`${id}-back`, back);
    if (shots.back) await shot(page, `${id}-back`);
    ex(back.pane).toBe('chat');
    ex(back.controls?.inside ?? back.joinBtn?.inside, "the view's controls are in view").toBe(true);
    ex(back.doc).toBe(0);
  } catch (e) {
    log(id, { ok: false, error: String(e).slice(0, 300) });
    await shot(page, `${id}-failed`).catch(() => undefined);
    throw e;
  } finally {
    await ctx.close().catch(() => undefined);
  }
}

/** The same boxes as the base (0 px): header and view. */
function sameBoxes(id: string, now: Awaited<ReturnType<typeof state>>) {
  const b = baseRow(id) as { header?: { box: number[] }; view?: { box: number[] } } | null;
  if (!b) return;
  ex(now.header?.box, `${id}: header box equals the base`).toEqual(b.header?.box);
  ex(now.view?.box, `${id}: view box equals the base`).toEqual(b.view?.box);
}

test('voice: Member List in a voice channel and a stage channel (SEC-AUD-385)', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  const voiceId = await channelOfType(ownerCtx, w.guildId, 2, 'Lounge Voice');
  const stageId = await channelOfType(ownerCtx, w.guildId, 13, 'Town Hall');
  const voice = `/channels/${w.guildId}/${voiceId}`;
  const stage = `/channels/${w.guildId}/${stageId}`;
  await setTheme(ownerCtx, 'lounge-dark');
  await phoneRound(
    browser,
    ownerCtx,
    'lounge-dark',
    voice,
    'voice-phone-lounge-dark',
    'Member List',
    {
      afterTap: true,
      back: !BASE,
    },
  );
  await phoneRound(
    browser,
    ownerCtx,
    'lounge-dark',
    stage,
    'stage-phone-lounge-dark',
    'Member List',
    {
      afterTap: !BASE,
    },
  );
  if (!BASE) {
    await setTheme(ownerCtx, 'lounge-light');
    await phoneRound(
      browser,
      ownerCtx,
      'lounge-light',
      voice,
      'voice-phone-lounge-light',
      'Member List',
      { afterTap: true },
    );
  }
  await setTheme(ownerCtx, 'lounge-editorial');
  for (const [kind, path] of [
    ['voice', voice],
    ['stage', stage],
  ] as const) {
    const { ctx, page } = await newPage(browser, false, ownerCtx, 'lounge-editorial');
    try {
      await open(page, path);
      const btn = page.getByRole('button', { name: 'Member List', exact: true });
      const before = { ...(await state(page)), pressed: await btn.getAttribute('aria-pressed') };
      log(`${kind}-desktop-before`, before);
      await btn.click();
      await page.waitForTimeout(500);
      const after = { ...(await state(page)), pressed: await btn.getAttribute('aria-pressed') };
      log(`${kind}-desktop-after-click`, after);
      if (kind === 'voice' && !BASE) await shot(page, 'voice-desktop-editorial');
      sameBoxes(`${kind}-desktop-before`, before);
      sameBoxes(`${kind}-desktop-after-click`, after);
      ex(after.stored, 'the desktop button does what it did').toBe(before.stored);
    } finally {
      await ctx.close().catch(() => undefined);
    }
  }
  await ownerCtx.close();
});

test('dm: the DM identity tap and the DM header button on a phone (SEC-AUD-386)', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  // The world has one other member and a stranger's DM is a message request: two new accounts, made friends of the
  // Owner, give a 1:1 DM and a group DM that open as conversations (D-863).
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
  log('dm-setup', { others: others.length });
  const one = await apiPost<{ id: string }>(ownerCtx.request, '/users/@me/channels', {
    recipient_id: others[0],
  });
  const group = await apiPost<{ id: string }>(ownerCtx.request, '/users/@me/channels', {
    recipients: others.slice(0, 2),
  });
  const oneP = `/channels/@me/${one.id}`;
  const groupP = `/channels/@me/${group.id}`;

  const identityRound = async (
    theme: string,
    path: string,
    id: string,
    shotName: string | null,
  ) => {
    const { ctx, page } = await newPage(browser, true, ownerCtx, theme);
    try {
      await openStoredFalse(page, path);
      const before = await state(page);
      log(`${id}-before`, before);
      await tapRe(page, /^(Show members of|Open profile of)/);
      const after = await state(page);
      log(`${id}-after-tap`, after);
      if (shotName) await shot(page, shotName);
      if (path === groupP) {
        ex(after.pane, 'the identity tap shows the pane').toBe('members');
        ex(after.membersPane?.inside, 'the pane is inside the viewport').toBe(true);
        ex(after.stored, 'memberListOpen is not written').toBe(before.stored);
        await swipeRight(page);
        const back = await state(page);
        log(`${id}-back`, back);
        ex(back.pane).toBe('chat');
        ex(back.controls?.inside, 'the composer is in view').toBe(true);
      }
    } catch (e) {
      log(id, { ok: false, error: String(e).slice(0, 300) });
      await shot(page, `${id}-failed`).catch(() => undefined);
      throw e;
    } finally {
      await ctx.close().catch(() => undefined);
    }
  };

  await setTheme(ownerCtx, 'lounge-dark');
  await identityRound(
    'lounge-dark',
    groupP,
    'dm-group-phone-lounge-dark-identity',
    'dm-group-phone-lounge-dark-identity-after-tap',
  );
  // The 1:1 identity tap opens the profile pop-out: unchanged, written down.
  await identityRound('lounge-dark', oneP, 'dm-one-phone-lounge-dark-identity', null);
  // The header's button on a phone: what it opens (D-893 kept).
  await phoneRound(
    browser,
    ownerCtx,
    'lounge-dark',
    oneP,
    'dm-one-phone-lounge-dark-header',
    'User Profile',
    { afterTap: true },
  );
  await phoneRound(
    browser,
    ownerCtx,
    'lounge-dark',
    groupP,
    'dm-group-phone-lounge-dark-header',
    'Member List',
    { afterTap: true },
  );
  if (!BASE) {
    await setTheme(ownerCtx, 'lounge-light');
    await identityRound(
      'lounge-light',
      groupP,
      'dm-group-phone-lounge-light-identity',
      'dm-group-phone-lounge-light-identity-after-tap',
    );
  }
  await setTheme(ownerCtx, 'lounge-editorial');
  const { ctx, page } = await newPage(browser, false, ownerCtx, 'lounge-editorial');
  try {
    await openStoredFalse(page, groupP);
    const before = await state(page);
    log('dm-group-desktop-before', before);
    await page.getByRole('button', { name: /^Show members of/ }).click();
    await page.waitForTimeout(500);
    const after = await state(page);
    log('dm-group-desktop-after-tap', after);
    await shot(page, 'dm-group-desktop-editorial-after-tap');
    sameBoxes('dm-group-desktop-before', before);
    ex(after.stored, 'the desktop tap writes the stored flag (as before)').toBe(true);
    ex(after.membersPane, 'the member column opens').not.toBeNull();
  } finally {
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
});
