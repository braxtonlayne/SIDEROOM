import { appendFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, apiPut, loginApi, registerUser, test } from '../support/fixtures';

/**
 * SEC-AUD-397 (D-907) and SEC-AUD-398 (D-909), wave 91 lounge. Item 1: on a desktop "Member List" in a voice and a
 * stage channel toggles the members column and shows its true state (aria-pressed, the tooltip, the stored flag, the
 * column); on a phone wave 90's cases still pass (More, "Member List" shows the pane, the stored flag is not
 * written). Item 2: on a phone the group DM members panel has the one-to-one card's inline gutter. Needs the sweep's
 * seeded world (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`'s "seed the sweep world"; this spec never
 * edits that spec). The Owner's account. Local only. A stage channel and a group DM the world does not hold are made
 * through the API in this run (D-863). The stack needs SR_VOICE__ENABLED=true. Every variable starts with
 * SR_P91_LOUNGE_.
 *
 * The commander's command (from `e2e/`, after the world is seeded on the same database; also set SR_CLI_BIN,
 * SR_DATABASE__URL and DATABASE_URL of the stack). Two tests: "voice" (item 1) and "dm" (item 2); `-g voice` or
 * `-g dm` runs one:
 *   SR_E2E_BASE_URL=http://localhost:5692 SR_P91_LOUNGE_ON=1 \
 *   SR_P91_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-lounge \
 *   npx playwright test tests/p91-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * The base case adds SR_P91_LOUNGE_BASE=1 (run it with the change switched off): files named "base-...", measures
 * and writes lines (no assertions). The after case reads `base-measure.json` in the same folder and compares the
 * 1280 group panel's boxes with it (0 px).
 *
 * 1280x800 Editorial: the button's name, aria-pressed and tooltip, the column and the stored flag, at the start and
 * after each click; then a text channel. 375x812 (touch): wave 90's voice and stage cases; the group DM's members
 * pane (header More, Member List) in Lounge Light and Dark: the start and end x of every piece, against the
 * one-to-one DM card's, and every button and the invite field against the hit test (D-856).
 */
const ON = process.env.SR_P91_LOUNGE_ON === '1';
const BASE = process.env.SR_P91_LOUNGE_BASE === '1';
const PRE = BASE ? 'base-' : '';
const OUT =
  process.env.SR_P91_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-lounge';
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
  console.log(`REPORT ${JSON.stringify(row).slice(0, 1800)}`);
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

/** The pane, the stored flag, the members column's box. */
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
    return {
      pane: document.querySelector('.lng-root')?.getAttribute('data-pane') ?? null,
      membersPane: box(document.querySelector('.lng-right')),
      stored,
      doc: document.documentElement.scrollWidth - vw,
      theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    };
  });

type Edges = [number, number, number, number];
interface Measure {
  missing?: boolean;
  card?: Edges;
  cardStyle?: { margin: string; padding: string };
  cardKids?: unknown[];
  panel?: Edges;
  head?: Edges;
  heading?: Edges;
  members?: Edges;
  row?: Edges;
  rowAvatar?: Edges;
  actions?: Edges;
  inviteHeading?: Edges;
  invite?: Edges;
  buttons?: [string, number, number, number, number, boolean][];
  doc?: number;
  cut?: number;
  vw?: number;
}

/** Start and end x (then top and height) of the panel's pieces, and the hit test of its controls (D-856). */
const panelMeasure = (page: Page) =>
  page.evaluate((): Measure & Record<string, unknown> => {
    const vw = document.documentElement.clientWidth;
    const edges = (e: Element): Edges => {
      const r = e.getBoundingClientRect();
      return [
        Math.round(r.left * 10) / 10,
        Math.round(r.right * 10) / 10,
        Math.round(r.top),
        Math.round(r.height),
      ];
    };
    const hit = (e: Element) => {
      const r = e.getBoundingClientRect();
      const cx = r.left + r.width / 2;
      const cy = r.top + r.height / 2;
      const okAt = (y: number) => {
        const h = document.elementFromPoint(cx, y);
        const lab = h?.closest('label');
        return !!h && (e === h || e.contains(h) || (!!lab && lab.contains(e)));
      };
      return okAt(cy - 21) && okAt(cy + 21);
    };
    const out: Measure & Record<string, unknown> = {};
    const root = document.querySelector('.lng-right .lng-profile');
    if (!root) return { missing: true, vw };
    out.panel = edges(root);
    const card = root.querySelector('.lng-profile-card');
    if (card) {
      const cs = getComputedStyle(card);
      out.card = edges(card);
      out.cardStyle = { margin: cs.margin, padding: cs.padding };
      out.cardKids = Array.from(card.children).map((c) => [
        `${c.tagName}.${c.className}`,
        ...edges(c),
      ]);
    }
    const pick = (name: string, sel: string) => {
      const e = root.querySelector(sel);
      if (!e) return;
      out[name] = edges(e);
      const cs = getComputedStyle(e);
      out[`${name}Style`] = { margin: cs.margin, padding: cs.padding };
    };
    pick('head', '.prf-group-head');
    pick('heading', '.prf-group-head h2');
    pick('members', '.prf-members');
    pick('row', '.prf-member');
    pick('rowMain', '.prf-member-main');
    pick('rowAvatar', '.prf-member-main > :first-child');
    pick('actions', '.prf-group-actions');
    pick('inviteHeading', '.lng-pop-h');
    pick('invite', '.prf-invite');
    out.buttons = Array.from(root.querySelectorAll('button, input')).map((b) => [
      (b.getAttribute('aria-label') ?? b.textContent ?? '').trim().slice(0, 24),
      ...edges(b),
      hit(b),
    ]) as Measure['buttons'];
    out.vw = vw;
    out.doc = document.documentElement.scrollWidth - vw;
    out.cut = Array.from(root.querySelectorAll('*')).filter((e) => {
      const r = e.getBoundingClientRect();
      return r.width > 0 && (r.left < -0.5 || r.right > vw + 0.5);
    }).length;
    return out;
  });

async function tap(page: Page, name: string) {
  await page.getByRole('button', { name, exact: true }).first().tap({ timeout: 5000 });
  await page.waitForTimeout(500);
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

/** Wave 90's phone case: More, "Member List": the pane shows and the stored flag is not written. */
async function phoneRound(
  browser: Browser,
  ownerCtx: BrowserContext,
  theme: string,
  path: string,
  id: string,
) {
  const { ctx, page } = await newPage(browser, true, ownerCtx, theme);
  try {
    await open(page, path);
    const before = await state(page);
    log(`${id}-before`, before);
    await tap(page, 'More actions');
    await tap(page, 'Member List');
    const after = await state(page);
    log(`${id}-after-tap`, after);
    ex(after.pane, 'the tap shows the members pane').toBe('members');
    ex(after.membersPane?.inside, 'the pane is inside the viewport').toBe(true);
    ex(after.stored, 'memberListOpen is not written').toBe(before.stored);
    ex(after.doc, 'no sideways scroll').toBe(0);
  } catch (e) {
    log(id, { ok: false, error: String(e).slice(0, 300) });
    throw e;
  } finally {
    await ctx.close().catch(() => undefined);
  }
}

/** The button's name, aria-pressed and tooltip, whether the column is drawn, and the stored flag. */
async function desktopState(page: Page) {
  const btn = page.getByRole('button', { name: 'Member List', exact: true });
  await page.mouse.move(5, 400);
  await btn.hover();
  await page.waitForTimeout(700);
  const tip = await page
    .locator('[role="tooltip"]')
    .first()
    .textContent({ timeout: 1500 })
    .catch(() => null);
  await page.mouse.move(5, 400);
  return {
    ...(await state(page)),
    name: await btn.getAttribute('aria-label'),
    pressed: await btn.getAttribute('aria-pressed'),
    tooltip: tip,
    column: (await page.locator('.lng-right').count()) > 0,
  };
}

test('voice: Member List on a desktop voice and stage channel (SEC-AUD-397)', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  const voiceId = await channelOfType(ownerCtx, w.guildId, 2, 'Lounge Voice');
  const stageId = await channelOfType(ownerCtx, w.guildId, 13, 'Town Hall');
  const voice = `/channels/${w.guildId}/${voiceId}`;
  const stage = `/channels/${w.guildId}/${stageId}`;
  const text = `/channels/${w.guildId}/${w.channelId}`;
  await setTheme(ownerCtx, 'lounge-editorial');
  const { ctx, page } = await newPage(browser, false, ownerCtx, 'lounge-editorial');
  try {
    for (const [kind, path] of [
      ['voice', voice],
      ['stage', stage],
    ] as const) {
      await open(page, path);
      const btn = page.getByRole('button', { name: 'Member List', exact: true });
      const a = await desktopState(page);
      log(`${kind}-desktop-start`, a);
      if (kind === 'voice') await shot(page, 'voice-desktop-editorial-open');
      await btn.click();
      await page.waitForTimeout(500);
      const b = await desktopState(page);
      log(`${kind}-desktop-click1`, b);
      if (!BASE) await shot(page, `${kind}-desktop-editorial-closed`);
      await btn.click();
      await page.waitForTimeout(500);
      const c = await desktopState(page);
      log(`${kind}-desktop-click2`, c);
      ex(a.theme, 'the room is Editorial').toBe('lounge-editorial');
      ex([a.pressed, a.column, a.stored], 'start: pressed, column, stored').toEqual([
        'true',
        true,
        true,
      ]);
      ex(a.tooltip, 'start tooltip').toContain('Hide');
      ex([b.pressed, b.column, b.stored], 'click 1: not pressed, no column, stored false').toEqual([
        'false',
        false,
        false,
      ]);
      ex(b.tooltip, 'click 1 tooltip').toContain('Show');
      ex([c.pressed, c.column, c.stored], 'click 2: pressed, column, stored').toEqual([
        'true',
        true,
        true,
      ]);
      // The voice channel leaves the column open for the stage channel; the stage channel leaves it closed, and the
      // text channel after it must agree with that.
      if (kind === 'stage') {
        await btn.click();
        await page.waitForTimeout(400);
      }
    }
    await page.goto(text, { waitUntil: 'domcontentloaded' });
    await idle(page, 3000);
    await page.locator('.lng-header').waitFor({ timeout: 8000 });
    const t = await desktopState(page);
    log('text-desktop-after-voice', t);
    ex([t.pressed, t.column, t.stored], 'a text channel agrees with the closed column').toEqual([
      'false',
      false,
      false,
    ]);
  } catch (e) {
    log('voice-desktop', { ok: false, error: String(e).slice(0, 300) });
    await shot(page, 'voice-desktop-failed').catch(() => undefined);
    throw e;
  } finally {
    await ctx.close().catch(() => undefined);
  }
  await setTheme(ownerCtx, 'lounge-dark');
  await phoneRound(browser, ownerCtx, 'lounge-dark', voice, 'voice-phone-lounge-dark');
  await phoneRound(browser, ownerCtx, 'lounge-dark', stage, 'stage-phone-lounge-dark');
  await ownerCtx.close();
});

test('dm: the group DM members panel gutter on a phone (SEC-AUD-398)', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
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
    recipients: others.slice(0, 2),
  });
  const oneP = `/channels/@me/${one.id}`;
  const groupP = `/channels/@me/${group.id}`;

  const phone = async (
    theme: string,
    path: string,
    button: string,
    id: string,
    shotName: string | null,
  ) => {
    const { ctx, page } = await newPage(browser, true, ownerCtx, theme);
    try {
      await open(page, path);
      await tap(page, 'More actions');
      await tap(page, button);
      await page.waitForTimeout(500);
      const s = await state(page);
      const m = await panelMeasure(page);
      log(id, { state: s, ...m });
      if (shotName) await shot(page, shotName);
      return { s, m };
    } catch (e) {
      log(id, { ok: false, error: String(e).slice(0, 300) });
      await shot(page, `${id}-failed`).catch(() => undefined);
      throw e;
    } finally {
      await ctx.close().catch(() => undefined);
    }
  };

  for (const [theme, short] of [
    ['lounge-light', 'light'],
    ['lounge-dark', 'dark'],
  ] as const) {
    await setTheme(ownerCtx, theme);
    const g = await phone(
      theme,
      groupP,
      'Member List',
      `dm-group-phone-${short}`,
      `dm-group-phone-lounge-${short}`,
    );
    const o =
      short === 'light'
        ? await phone(
            theme,
            oneP,
            'User Profile',
            'dm-one-phone-light',
            'dm-one-phone-lounge-light',
          )
        : null;
    ex(g.s.theme, 'the room is the chosen theme').toBe(theme);
    ex(g.s.pane).toBe('members');
    ex(g.m.doc, 'no sideways scroll').toBe(0);
    ex(g.m.cut, 'nothing is cut off').toBe(0);
    for (const b of g.m.buttons ?? []) ex(b[5], `hit test: ${b[0]}`).toBe(true);
    if (o?.m.card) {
      // The group panel's blocks start and end where the card's own content does, to the pixel.
      const [cs, ce] = [o.m.card[0] + 12, o.m.card[1] - 12];
      const at = (label: string) => g.m.buttons?.find((x) => x[0].startsWith(label));
      for (const label of ['Add friends', 'Create invite link']) {
        ex(at(label)?.[1], `${label} starts at the card's content`).toBe(cs);
      }
      ex(g.m.rowAvatar?.[0], 'the member avatars start at the same x').toBe(cs);
      ex(g.m.head ? g.m.head[0] + 12 : null, 'the heading block starts at the same x').toBe(cs);
      ex(at('Group settings')?.[2], 'the last button ends inside the card').toBeLessThanOrEqual(ce);
    }
  }
  await setTheme(ownerCtx, 'lounge-editorial');
  const { ctx, page } = await newPage(browser, false, ownerCtx, 'lounge-editorial');
  try {
    await open(page, groupP);
    const s = await state(page);
    const m = await panelMeasure(page);
    log('dm-group-desktop', { state: s, ...m });
    const b = baseRow('dm-group-desktop') as Measure | null;
    if (b) {
      for (const k of ['panel', 'head', 'members', 'actions', 'invite', 'buttons'] as const) {
        ex(m[k], `1280 ${k} equals the base`).toEqual(b[k]);
      }
    }
  } finally {
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
});
