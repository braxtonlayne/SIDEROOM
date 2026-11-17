import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, apiPut, loginApi, registerUser, test } from '../support/fixtures';

/**
 * SEC-AUD-402 (D-915) and SEC-AUD-403 (D-916), wave 92 lounge. Item 1: on a phone the one-to-one DM profile card's
 * "More actions" button, mutual servers/friends link and "View Full Profile" link pass the 44 px hit test (D-856).
 * Item 2: the group DM panel's "INVITE LINK" label has 12 px of room above it. Needs the sweep's seeded world
 * (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`'s "seed the sweep world"; this spec never edits that
 * spec). The Owner's account. Local only. A friend, a one-to-one DM and a group DM are made through the API in the
 * first run (D-863) and their ids are kept in `ids.json` in the shots folder, so the base case and the after case
 * measure the same conversations. Every variable starts with SR_P92_LOUNGE_.
 *
 * The commander's command (from `e2e/`, after the world is seeded on the same database; also set SR_CLI_BIN,
 * SR_DATABASE__URL and DATABASE_URL of the stack). Two tests: "card" (item 1) and "group" (item 2); `-g card` or
 * `-g group` runs one:
 *   SR_E2E_BASE_URL=http://localhost:5703 SR_P92_LOUNGE_ON=1 \
 *   SR_P92_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p92-lounge \
 *   npx playwright test tests/p92-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * The base case adds SR_P92_LOUNGE_BASE=1 (run it with `profile-p11.css` restored from the base): files named
 * "base-...", measures and writes lines (no assertions). The after case reads `base-measure.json` in the same folder
 * and compares the 1280 boxes with it (0 px, except the group panel's invite block and what follows it: exactly 12).
 *
 * 375x812 (touch, Lounge Light and Dark): the card's controls against the hit test with the control scrolled into the
 * middle of the viewport first (a null is a probe error, never a fault), the gaps, the card's height, the menu, the
 * full profile; the popout (measured only); the group panel's action row, invite block and invite field.
 */
const ON = process.env.SR_P92_LOUNGE_ON === '1';
const BASE = process.env.SR_P92_LOUNGE_BASE === '1';
const PRE = BASE ? 'base-' : '';
const OUT =
  process.env.SR_P92_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p92-lounge';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
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
  console.log(`REPORT ${JSON.stringify(row).slice(0, 2400)}`);
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

interface Ids {
  one: string;
  group: string;
}

/** The friends, the one-to-one DM and the group DM: made once, kept in ids.json for the after case. */
async function conversations(browser: Browser, ownerCtx: BrowserContext): Promise<Ids> {
  mkdirSync(OUT, { recursive: true });
  if (existsSync(IDS_FILE)) return JSON.parse(readFileSync(IDS_FILE, 'utf8')) as Ids;
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
  const ids: Ids = { one: one.id, group: group.id };
  writeFileSync(IDS_FILE, JSON.stringify(ids));
  return ids;
}

interface Ctl {
  label: string;
  box: number[];
  hit: boolean;
  takers: string[];
  gapAbove: number | null;
  gapBelow: number | null;
}

/**
 * The hit-test rule (wave 91): each control is scrolled into the middle of the viewport, must have a box, and its
 * centre must return an element; a null throws (a probe error, never a fault). The gaps are measured at scrollTop 0
 * between neighbouring controls' boxes. Nothing in these panels is a closed <details> or a collapsed card.
 */
const controls = (page: Page, rootSel: string) =>
  page.evaluate((sel): Ctl[] => {
    const root = document.querySelector(sel);
    if (!root) throw new Error(`probe error: ${sel} not found`);
    const list = Array.from(
      root.querySelectorAll('button:not(:disabled), a[href], input:not([type=hidden]), textarea'),
    ).filter((e) => {
      const r = e.getBoundingClientRect();
      return r.width > 0 && r.height > 0;
    });
    const name = (e: Element) =>
      (e.getAttribute('aria-label') ?? e.textContent ?? '').trim().slice(0, 28) ||
      `${e.tagName}.${e.className}`;
    const who = (h: Element | null) =>
      h
        ? `${h.tagName.toLowerCase()}.${String(h.className).slice(0, 40)} "${(
            h.getAttribute('aria-label') ??
            h.textContent ??
            ''
          )
            .trim()
            .slice(0, 24)}"`
        : 'null';
    const scrollers = () => document.querySelectorAll('.lng-right, .lng-profile, .lng-pop');
    scrollers().forEach((s) => ((s as HTMLElement).scrollTop = 0));
    const tops = list.map((e) => {
      const r = e.getBoundingClientRect();
      return [r.top, r.bottom] as [number, number];
    });
    const out: Ctl[] = [];
    list.forEach((e, i) => {
      e.scrollIntoView({ block: 'center' });
      const r = e.getBoundingClientRect();
      if (r.width <= 0 || r.height <= 0) throw new Error(`probe error: no box for ${name(e)}`);
      const cx = r.left + r.width / 2;
      const cy = r.top + r.height / 2;
      if (cy < 0 || cy > window.innerHeight) throw new Error(`probe error: ${name(e)} off screen`);
      if (!document.elementFromPoint(cx, cy))
        throw new Error(`probe error: centre of ${name(e)} returns null`);
      const takers: string[] = [];
      const okAt = (y: number, tag: string) => {
        const h = document.elementFromPoint(cx, y);
        const lab = h?.closest('label');
        const ok = !!h && (e === h || e.contains(h) || (!!lab && lab.contains(e)));
        if (!ok) takers.push(`${tag}: ${who(h)}`);
        return ok;
      };
      const a = okAt(cy - 21, 'above');
      const b = okAt(cy + 21, 'below');
      const above = tops.filter((t, j) => j !== i && t[1] <= (tops[i] ?? [0, 0])[0] + 0.5);
      const below = tops.filter((t, j) => j !== i && t[0] >= (tops[i] ?? [0, 0])[1] - 0.5);
      out.push({
        label: name(e),
        box: [r.width, r.height].map((n) => Math.round(n * 10) / 10),
        hit: a && b,
        takers,
        gapAbove: above.length
          ? Math.round(((tops[i] ?? [0, 0])[0] - Math.max(...above.map((t) => t[1]))) * 10) / 10
          : null,
        gapBelow: below.length
          ? Math.round((Math.min(...below.map((t) => t[0])) - (tops[i] ?? [0, 0])[1]) * 10) / 10
          : null,
      });
    });
    scrollers().forEach((s) => ((s as HTMLElement).scrollTop = 0));
    return out;
  }, rootSel);

/** Boxes of the card's children at scroll 0, the card's, the page's sideways overflow, and what is cut off. */
const cardBoxes = (page: Page, sel: string) =>
  page.evaluate((s) => {
    const root = document.querySelector(s);
    if (!root)
      return { missing: true, theme: null, card: [0, 0, 0, 0], kids: [], doc: -1, cut: -1 };
    const vw = document.documentElement.clientWidth;
    const rr = (e: Element) => {
      const r = e.getBoundingClientRect();
      return [r.left, r.top, r.width, r.height].map((n) => Math.round(n * 10) / 10);
    };
    const card = root.querySelector('.lng-profile-card') ?? root;
    return {
      missing: false,
      theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
      card: rr(card),
      kids: Array.from(card.children).map((c) => [
        `${c.tagName}.${c.className}`.slice(0, 50),
        ...rr(c),
      ]),
      doc: document.documentElement.scrollWidth - vw,
      cut: Array.from(root.querySelectorAll('*')).filter((e) => {
        const r = e.getBoundingClientRect();
        return r.width > 0 && (r.left < -0.5 || r.right > vw + 0.5);
      }).length,
    };
  }, sel);

async function oneRound(
  browser: Browser,
  ownerCtx: BrowserContext,
  theme: string,
  phone: boolean,
  path: string,
  id: string,
  shotName: string,
) {
  const { ctx, page } = await newPage(browser, phone, ownerCtx, theme);
  try {
    await open(page, path);
    if (phone) {
      await tap(page, 'More actions');
      await tap(page, 'User Profile');
    }
    await page.locator('.prf-dm-panel .lng-profile-card').waitFor({ timeout: 8000 });
    await page.waitForTimeout(600);
    const boxes = await cardBoxes(page, '.lng-right .lng-profile');
    const ctl = await controls(page, '.lng-right .prf-dm-panel .lng-profile-card');
    log(id, { boxes, ctl });
    await shot(page, shotName);
    return { page, ctx, boxes, ctl };
  } catch (e) {
    log(id, { ok: false, error: String(e).slice(0, 300) });
    await shot(page, `${id}-failed`).catch(() => undefined);
    await ctx.close().catch(() => undefined);
    throw e;
  }
}

test('card: the one-to-one profile card on a phone (SEC-AUD-402)', async ({ browser }) => {
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const ids = await conversations(browser, ownerCtx);
  const oneP = `/channels/@me/${ids.one}`;

  for (const [theme, short] of [
    ['lounge-light', 'light'],
    ['lounge-dark', 'dark'],
  ] as const) {
    await setTheme(ownerCtx, theme);
    const r = await oneRound(
      browser,
      ownerCtx,
      theme,
      true,
      oneP,
      `card-phone-${short}`,
      `card-phone-lounge-${short}`,
    );
    try {
      ex(r.boxes.theme, 'the room is the chosen theme').toBe(theme);
      ex(r.boxes.doc, 'no sideways scroll').toBe(0);
      ex(r.boxes.cut, 'nothing is cut off').toBe(0);
      for (const c of r.ctl) ex(c.hit, `hit test: ${c.label} ${c.takers.join('; ')}`).toBe(true);
      const b = baseRow(`card-phone-${short}`) as { boxes?: { card: number[] } } | null;
      if (b?.boxes)
        log(`card-phone-${short}-height-diff`, {
          px: (r.boxes.card[3] ?? 0) - (b.boxes.card[3] ?? 0),
        });
      if (short === 'light') {
        // The menu opens from the card's "More actions", a row can be tapped, it closes.
        const more = r.page.locator('.prf-dm-panel .prf-dm-actions .lng-icon-btn');
        await more.scrollIntoViewIfNeeded();
        await more.tap();
        await r.page.waitForTimeout(500);
        const opened = await r.page.getByRole('menu').count();
        log('card-menu', { opened, items: await r.page.getByRole('menuitem').allTextContents() });
        await shot(r.page, 'card-phone-lounge-light-menu');
        ex(opened, 'the menu opens').toBeGreaterThan(0);
        await r.page.getByRole('menuitem', { name: 'Copy Username' }).first().tap();
        await r.page.waitForTimeout(500);
        const closed = await r.page.getByRole('menu').count();
        log('card-menu-closed', { menus: closed });
        ex(closed, 'the menu closes after a row is tapped').toBe(0);
        // "View Full Profile" and the mutual link each open the full profile.
        for (const label of ['viewFull', 'mutual']) {
          const link =
            label === 'mutual'
              ? r.page.locator('.prf-dm-panel .prf-dm-mutual')
              : r.page.locator(
                  '.prf-dm-panel .lng-profile-card > .lng-link-btn:not(.prf-dm-mutual)',
                );
          if (!(await link.count())) {
            log(`card-${label}`, { drawn: false });
            continue;
          }
          await link.scrollIntoViewIfNeeded();
          await link.tap();
          await r.page.waitForTimeout(900);
          const dialogs = await r.page.getByRole('dialog').count();
          log(`card-${label}`, { drawn: true, dialogs });
          ex(dialogs, `${label} opens the full profile`).toBeGreaterThan(0);
          await r.page.keyboard.press('Escape');
          await r.page.waitForTimeout(500);
        }
      }
    } finally {
      await r.ctx.close().catch(() => undefined);
    }
  }

  // 1280 Editorial: every box of the card equal to the base.
  await setTheme(ownerCtx, 'lounge-editorial');
  const d = await oneRound(
    browser,
    ownerCtx,
    'lounge-editorial',
    false,
    oneP,
    'card-desktop-editorial',
    'card-desktop-lounge-editorial',
  );
  try {
    const b = baseRow('card-desktop-editorial') as {
      boxes?: Record<string, unknown>;
      ctl?: Ctl[];
    } | null;
    if (b?.boxes) {
      const strip = (c: Ctl[]) => c.map((x) => [x.box, x.hit]);
      ex(d.boxes, '1280 card boxes equal the base').toEqual(b.boxes);
      ex(strip(d.ctl), '1280 control boxes equal the base').toEqual(strip(b.ctl ?? []));
    }
  } finally {
    await d.ctx.close().catch(() => undefined);
  }

  // The profile popout on a phone: measured and reported only (a member's name in the group panel opens it).
  await setTheme(ownerCtx, 'lounge-light');
  const { ctx, page } = await newPage(browser, true, ownerCtx, 'lounge-light');
  try {
    await open(page, `/channels/@me/${ids.group}`);
    await tap(page, 'More actions');
    await tap(page, 'Member List');
    await page.locator('.prf-member-main').nth(1).tap();
    await page.locator('.lng-pop').first().waitFor({ timeout: 6000 });
    await page.waitForTimeout(700);
    const ctl = await controls(page, '.lng-pop');
    log('popout-phone', { pop: await page.locator('.lng-pop').first().boundingBox(), ctl });
    await shot(page, 'popout-phone-lounge-light');
  } catch (e) {
    log('popout-phone', { ok: false, error: String(e).slice(0, 300) });
    await shot(page, 'popout-phone-failed').catch(() => undefined);
  } finally {
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
});

/** The group panel's pieces: action row, invite block (heading box and its text), gaps. */
const groupMeasure = (page: Page) =>
  page.evaluate(() => {
    const root = document.querySelector('.lng-right .lng-profile');
    if (!root) return { missing: true };
    const vw = document.documentElement.clientWidth;
    const r1 = (n: number) => Math.round(n * 10) / 10;
    const rr = (e: Element | null): [number, number, number, number] | null => {
      if (!e) return null;
      const r = e.getBoundingClientRect();
      return [r1(r.left), r1(r.top), r1(r.width), r1(r.height)];
    };
    const sec = root.querySelector(':scope > section');
    const act = root.querySelector('.prf-group-actions');
    const members = root.querySelector('.prf-members');
    const head = sec?.querySelector('h3') ?? null;
    let textTop: number | null = null;
    if (head) {
      const rg = document.createRange();
      rg.selectNodeContents(head);
      textTop = r1(rg.getBoundingClientRect().top);
    }
    const cs = (e: Element | null) =>
      e ? { margin: getComputedStyle(e).margin, padding: getComputedStyle(e).padding } : null;
    const a = rr(act);
    const s = rr(sec);
    const h = rr(head);
    const m = rr(members);
    return {
      missing: false,
      theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
      panel: rr(root),
      members: m,
      actions: a,
      section: s,
      heading: h,
      headingTextTop: textTop,
      gapActionsToSection: a && s ? r1(s[1] - (a[1] + a[3])) : null,
      gapActionsToHeading: a && h ? r1(h[1] - (a[1] + a[3])) : null,
      gapActionsToHeadingText: a && textTop !== null ? r1(textTop - (a[1] + a[3])) : null,
      gapAboveActions: a && m ? r1(a[1] - (m[1] + m[3])) : null,
      sectionStyle: cs(sec),
      headingStyle: cs(head),
      doc: document.documentElement.scrollWidth - vw,
      cut: Array.from(root.querySelectorAll('*')).filter((e) => {
        const r = e.getBoundingClientRect();
        return r.width > 0 && (r.left < -0.5 || r.right > vw + 0.5);
      }).length,
    };
  });

type GM = Record<string, number[] | number | string | null | boolean>;

test('group: the group DM panel invite block (SEC-AUD-403)', async ({ browser }) => {
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const ids = await conversations(browser, ownerCtx);
  const groupP = `/channels/@me/${ids.group}`;

  for (const [theme, short] of [
    ['lounge-light', 'light'],
    ['lounge-dark', 'dark'],
  ] as const) {
    await setTheme(ownerCtx, theme);
    const { ctx, page } = await newPage(browser, true, ownerCtx, theme);
    try {
      await open(page, groupP);
      await tap(page, 'More actions');
      await tap(page, 'Member List');
      await page.locator('.lng-right .lng-profile > section').waitFor({ timeout: 8000 });
      await page.waitForTimeout(500);
      const m = (await groupMeasure(page)) as GM;
      const ctl = await controls(page, '.lng-right .lng-profile');
      // "Create invite link" is not clipped by the panel's bottom: scroll the panel to its end.
      const clip = await page.evaluate(() => {
        const p = document.querySelector('.lng-right .lng-profile') as HTMLElement;
        p.scrollTop = p.scrollHeight;
        const b = Array.from(p.querySelectorAll('button')).find((x) =>
          /Create invite link/.test(x.textContent ?? ''),
        );
        const pr = p.getBoundingClientRect();
        const br = b?.getBoundingClientRect();
        p.scrollTop = 0;
        return br
          ? { buttonBottom: Math.round(br.bottom), panelBottom: Math.round(pr.bottom) }
          : null;
      });
      log(`group-phone-${short}`, { m, ctl, clip });
      await shot(page, `group-phone-lounge-${short}`);
      ex(m.theme, 'the room is the chosen theme').toBe(theme);
      ex(m.gapActionsToHeading, 'the label is 12px under the action row').toBe(12);
      ex(m.doc, 'no sideways scroll').toBe(0);
      ex(m.cut, 'nothing is cut off').toBe(0);
      for (const c of ctl) ex(c.hit, `hit test: ${c.label} ${c.takers.join('; ')}`).toBe(true);
      if (clip) {
        ex(clip.buttonBottom, 'Create invite link is not clipped').toBeLessThanOrEqual(
          clip.panelBottom,
        );
      }
      if (short === 'light') {
        // Create the invite link once: the field and its copy button appear, have the gutter, pass the hit test.
        await page.getByRole('button', { name: 'Create invite link' }).first().tap();
        await page.locator('.prf-invite input').waitFor({ timeout: 8000 });
        await page.waitForTimeout(500);
        const ctl2 = await controls(page, '.lng-right .lng-profile');
        const inv = await page.evaluate(() => {
          const i = document.querySelector('.prf-invite input')?.getBoundingClientRect();
          const b = document.querySelector('.prf-invite button')?.getBoundingClientRect();
          return {
            input: i ? [i.left, i.right] : null,
            copy: b ? [b.left, b.right] : null,
          };
        });
        log('group-phone-invite', { ctl: ctl2, inv });
        await shot(page, 'group-phone-lounge-light-invite');
        for (const c of ctl2) {
          ex(c.hit, `hit test (invite): ${c.label} ${c.takers.join('; ')}`).toBe(true);
        }
        ex(inv.input?.[0], 'the field starts at the card content (x = 20)').toBe(20);
      }
    } catch (e) {
      log(`group-phone-${short}`, { ok: false, error: String(e).slice(0, 300) });
      await shot(page, `group-phone-${short}-failed`).catch(() => undefined);
      throw e;
    } finally {
      await ctx.close().catch(() => undefined);
    }
  }

  await setTheme(ownerCtx, 'lounge-editorial');
  const { ctx, page } = await newPage(browser, false, ownerCtx, 'lounge-editorial');
  try {
    await open(page, groupP);
    await page.locator('.lng-right .lng-profile > section').waitFor({ timeout: 8000 });
    const m = (await groupMeasure(page)) as GM;
    log('group-desktop', { m });
    await shot(page, 'group-desktop-lounge-editorial');
    const b = baseRow('group-desktop') as { m?: GM } | null;
    if (b?.m) {
      for (const k of ['members', 'actions']) {
        ex(m[k], `1280 ${k} equals the base`).toEqual(b.m[k]);
      }
      // The panel is as tall as its content: same left, top and width, taller by the gap added.
      const p = m.panel as number[];
      const bp = b.m.panel as number[];
      ex(p.slice(0, 3), '1280 panel left, top, width equal the base').toEqual(bp.slice(0, 3));
      ex((p[3] ?? 0) - (bp[3] ?? 0), '1280 the panel grew by the gap added').toBe(
        12 - (b.m.gapActionsToSection as number),
      );
      const s = m.section as number[];
      const bs = b.m.section as number[];
      ex((s[1] ?? 0) - (bs[1] ?? 0), '1280 the section moved by the gap added').toBe(
        12 - (b.m.gapActionsToSection as number),
      );
    }
    ex(
      m.gapActionsToHeading,
      '1280 the label is at least 12px under the action row',
    ).toBeGreaterThanOrEqual(12);
  } finally {
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
});
