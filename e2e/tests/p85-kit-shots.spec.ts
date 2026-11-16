import { appendFileSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';

/**
 * Wave 85, kit builder: SEC-AUD-365 (D-870, the message "More" menu opens by its trigger), SEC-AUD-366 (D-878, a tab
 * strip at load), SEC-AUD-367 (D-871, the invite and template cards' form). NEEDS THE SWEEP'S SEEDED WORLD
 * (`e2e/.state/sweep/world.json`, made by `SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the
 * sweep world"`; never edit that spec). The Owner's account. Local only: SR_P85_KIT = more | tabs | cards | all.
 * SR_P85_BASE=1 names every file and line "base-..." (run with the change held back, or before it is built).
 *  - more: 1280x800 Lounge Dark, a message in the middle of the list: hover, click "More"; boxes of the trigger and the
 *    menu at the click, 300 ms and 1 s, with the pointer moved off the message, Escape and focus, the toolbar's hiding;
 *    the console lines "P85LOG ..." (temporary logs of the base run) are written to console-<n>.txt. Unchanged list:
 *    the emoji picker, a right-click menu, a sub-menu of "More", one menu of the channel header, one kit Select list in
 *    Settings, one tooltip, and the "More" menu at 375.
 *  - tabs: 375x812 Lounge Dark: every tab strip of Admin > Moderation, a server's settings and the Lounge (Saved items):
 *    the selected tab and scrollLeft / scrollWidth / clientWidth at load, 300 ms, 1 s; after: a late tab by a tap.
 *  - cards: the template (made through the API, deleted at the end) and an invite (made through the API) at 375 and
 *    1280, in the app (Editorial, Lounge Light, Lounge Dark) and signed out: the card's content box against the
 *    form's, the field's, the button's boxes, every direct child's box, the sideways scroll; one shot each.
 * Shots and measure.jsonl go to SR_P85_KIT_OUT. No assertions: it measures and writes lines.
 */
const MODE = process.env.SR_P85_KIT ?? '';
const ON = ['more', 'tabs', 'cards', 'all'].includes(MODE);
const BASE = process.env.SR_P85_BASE === '1';
const PRE = BASE ? 'base-' : '';
const OUT =
  process.env.SR_P85_KIT_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p85-kit';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SIZES = {
  phone: { width: 375, height: 812 },
  desktop: { width: 1280, height: 800 },
} as const;
type Size = keyof typeof SIZES;

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

interface World {
  guildId: string;
  channelId: string;
}
const world = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

const log = (id: string, line: Record<string, unknown>) => {
  mkdirSync(OUT, { recursive: true });
  const row = { id: `${PRE}${id}`, ...line };
  appendFileSync(resolve(OUT, `${PRE}measure.jsonl`), `${JSON.stringify(row)}\n`);
  console.log(`REPORT ${JSON.stringify(row).slice(0, 700)}`);
};

async function setMain(ownerCtx: BrowserContext, theme: string) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  console.log(`REPORT account theme read back: ${back.settings?.user_prefs?.theme}`);
}

async function newPage(
  browser: Browser,
  size: Size,
  theme: string,
  signedOut = false,
  consoleOut?: string[],
) {
  const phone = size === 'phone';
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: SIZES[size],
    ...(signedOut ? {} : { storageState: OWNER_STATE }),
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
  if (consoleOut)
    page.on('console', (m) => {
      if (m.text().startsWith('P85LOG')) consoleOut.push(m.text().slice(0, 600));
    });
  await page.route(/sweep-preview\.invalid|\/cdn\/external\//, (r) =>
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

// ---------------------------------------------------------------------------------------------------------------
// Item 1: the "More" menu
// ---------------------------------------------------------------------------------------------------------------

/** Boxes of a message's trigger and of the open floating element (menu / dialog / listbox / tooltip), in the page. */
function floatState(arg: { w: string; pre: number[] | null }) {
  const w = arg.w;
  const r2 = (r: DOMRect) => [
    Math.round(r.left),
    Math.round(r.top),
    Math.round(r.width),
    Math.round(r.height),
  ];
  const msg = document.querySelector('[data-p85]') as HTMLElement | null;
  const tools = msg?.querySelector('.lng-msg-tools') as HTMLElement | null;
  const trig = tools?.querySelector(`button[aria-label="${w}"]`) as HTMLElement | null;
  const menus = Array.from(
    document.querySelectorAll<HTMLElement>('[role="menu"], [role="dialog"], [role="tooltip"]'),
  ).filter((e) => e.getBoundingClientRect().width > 0 && !e.closest('.lng-msg'));
  const menu = menus[menus.length - 1];
  const tr = trig?.getBoundingClientRect();
  const mr = menu?.getBoundingClientRect();
  return {
    trigger: tr ? r2(tr) : null,
    expanded: trig?.getAttribute('aria-expanded') ?? null,
    toolsDisplay: tools ? getComputedStyle(tools).display : null,
    menu: mr ? r2(mr) : null,
    role: menu?.getAttribute('role') ?? null,
    endDelta: tr && mr ? Math.round(mr.right - tr.right) : null,
    /** The menu's inline-end edge minus the trigger's before the click (the toolbar may hide while the menu is open). */
    endDeltaPre: arg.pre && mr ? Math.round(mr.right - (arg.pre[0]! + arg.pre[2]!)) : null,
    pre: arg.pre,
    inViewport: mr
      ? mr.left >= 0 && mr.top >= 0 && mr.right <= innerWidth && mr.bottom <= innerHeight
      : null,
    active: document.activeElement
      ? `${document.activeElement.tagName.toLowerCase()}[${document.activeElement.getAttribute('aria-label') ?? ''}]`
      : null,
  };
}
/** Any open floating element's box, whatever opened it (for the unchanged list). */
function anyFloat() {
  const els = Array.from(
    document.querySelectorAll<HTMLElement>(
      '[role="menu"], [role="listbox"], [role="dialog"], [role="tooltip"]',
    ),
  ).filter((e) => e.getBoundingClientRect().width > 0);
  const e = els[els.length - 1];
  const r = e?.getBoundingClientRect();
  return r
    ? {
        role: e!.getAttribute('role'),
        box: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)],
      }
    : null;
}

async function pickMessage(page: Page) {
  const msgs = page
    .locator('.lng-msg')
    .filter({ has: page.getByRole('button', { name: /^Open profile of / }) });
  const n = await msgs.count();
  const m = msgs.nth(Math.max(0, Math.floor(n / 2)));
  await m.scrollIntoViewIfNeeded();
  await m.evaluate((e) => e.setAttribute('data-p85', '1'));
  return m;
}

async function moreItem(browser: Browser, ownerCtx: BrowserContext) {
  const w = world();
  await setMain(ownerCtx, 'lounge-dark');
  const lines: string[] = [];
  const { ctx, page } = await newPage(browser, 'desktop', 'lounge-dark', false, lines);
  try {
    await page.goto(`/channels/${w.guildId}/${w.channelId}`, {
      waitUntil: 'domcontentloaded',
      timeout: 15_000,
    });
    await idle(page, 4000);
    const m = await pickMessage(page);
    const hover = async () => {
      await page.keyboard.press('Escape');
      await page.mouse.move(5, 5);
      await page.waitForTimeout(500);
      await m.scrollIntoViewIfNeeded();
      const box = await m.boundingBox();
      if (!box) throw new Error('no message box');
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      await page.waitForTimeout(300);
      return box;
    };
    for (const what of ['More', 'Add Reaction']) {
      const tag = what === 'More' ? 'more' : 'emoji';
      await hover();
      const tb = await m
        .locator('.lng-msg-tools')
        .getByRole('button', { name: what, exact: true })
        .boundingBox();
      const pre = tb ? [tb.x, tb.y, tb.width, tb.height].map(Math.round) : null;
      await m.locator('.lng-msg-tools').getByRole('button', { name: what, exact: true }).click();
      await page.waitForTimeout(100);
      log(`${tag}-click`, await page.evaluate(floatState, { w: what, pre }));
      await shot(page, `${tag}-menu-open`);
      await page.waitForTimeout(200);
      log(`${tag}-t300`, await page.evaluate(floatState, { w: what, pre }));
      await page.waitForTimeout(700);
      log(`${tag}-t1000`, await page.evaluate(floatState, { w: what, pre }));
      // The pointer moved off the message.
      await page.mouse.move(5, 300);
      await page.waitForTimeout(500);
      log(`${tag}-pointer-away`, await page.evaluate(floatState, { w: what, pre }));
      await shot(page, `${tag}-menu-pointer-away`);
      await page.keyboard.press('Escape');
      await page.waitForTimeout(400);
      log(`${tag}-escape`, await page.evaluate(floatState, { w: what, pre }));
      await page.mouse.move(5, 5);
      await page.waitForTimeout(500);
      log(`${tag}-hides`, await page.evaluate(floatState, { w: what, pre }));
    }
    // A sub-menu of "More" ("Add Reaction" if the menu has one, else the first sub-menu).
    try {
      await hover();
      await m.locator('.lng-msg-tools').getByRole('button', { name: 'More', exact: true }).click();
      await page.waitForTimeout(400);
      const sub = page.locator('[role="menu"] [aria-haspopup="menu"]').first();
      await sub.hover();
      await page.waitForTimeout(500);
      log('more-submenu', {
        label: await sub.textContent(),
        menus: await page.evaluate(() =>
          Array.from(document.querySelectorAll('[role="menu"]')).map((e) => {
            const r = e.getBoundingClientRect();
            return [
              Math.round(r.left),
              Math.round(r.top),
              Math.round(r.width),
              Math.round(r.height),
            ];
          }),
        ),
      });
      await shot(page, 'more-submenu');
      await page.keyboard.press('Escape');
      await page.keyboard.press('Escape');
    } catch (e) {
      log('more-submenu', { ok: false, error: String(e).slice(0, 200) });
    }
    // A right-click menu on a message: at the pointer.
    try {
      const box = await hover();
      const px = box.x + box.width / 3;
      const py = box.y + box.height / 2;
      await page.mouse.click(px, py, { button: 'right' });
      await page.waitForTimeout(400);
      log('context-menu', {
        pointer: [Math.round(px), Math.round(py)],
        ...(await page.evaluate(anyFloat)),
      });
      await shot(page, 'context-menu');
      await page.keyboard.press('Escape');
    } catch (e) {
      log('context-menu', { ok: false, error: String(e).slice(0, 200) });
    }
    // One other DropdownMenu in the Lounge (the channel header's, default placement) and a tooltip.
    try {
      await page.mouse.move(5, 5);
      const trig = page
        .locator('header [aria-haspopup="menu"], .lng-header [aria-haspopup="menu"]')
        .first();
      const tb = await trig.boundingBox();
      await trig.click();
      await page.waitForTimeout(400);
      log('other-dropdown', { trigger: tb, ...(await page.evaluate(anyFloat)) });
      await shot(page, 'other-dropdown');
      await page.keyboard.press('Escape');
      await page.waitForTimeout(300);
    } catch (e) {
      log('other-dropdown', { ok: false, error: String(e).slice(0, 200) });
    }
    try {
      const b = page.locator('header button[aria-label], .lng-header button[aria-label]').first();
      const bb = await b.boundingBox();
      await b.hover();
      await page.waitForTimeout(900);
      log('tooltip', {
        trigger: bb,
        label: await b.getAttribute('aria-label'),
        ...(await page.evaluate(anyFloat)),
      });
      await page.mouse.move(5, 400);
    } catch (e) {
      log('tooltip', { ok: false, error: String(e).slice(0, 200) });
    }
    // A kit Select's list in the member's Settings.
    try {
      await page.goto('/settings/user/appearance', {
        waitUntil: 'domcontentloaded',
        timeout: 15_000,
      });
      await idle(page, 3000);
      const sel = page.locator('[aria-haspopup="listbox"]').first();
      const sb = await sel.boundingBox();
      await sel.click();
      await page.waitForTimeout(400);
      log('select-list', { trigger: sb, ...(await page.evaluate(anyFloat)) });
      await shot(page, 'select-list');
      await page.keyboard.press('Escape');
    } catch (e) {
      log('select-list', { ok: false, error: String(e).slice(0, 200) });
    }
  } catch (e) {
    log('more', { ok: false, error: String(e).slice(0, 300) });
    await shot(page, 'more-failed').catch(() => undefined);
  }
  await ctx.close().catch(() => undefined);
  mkdirSync(OUT, { recursive: true });
  appendFileSync(resolve(OUT, `${PRE}console.txt`), `${lines.join('\n')}\n`);
  console.log(`REPORT console lines kept: ${lines.length}`);

  // The "More" menu at 375.
  const p2 = await newPage(browser, 'phone', 'lounge-dark');
  try {
    await p2.page.goto(`/channels/${w.guildId}/${w.channelId}`, {
      waitUntil: 'domcontentloaded',
      timeout: 15_000,
    });
    await idle(p2.page, 4000);
    const m = await pickMessage(p2.page);
    await m.tap();
    await p2.page.waitForTimeout(500);
    const trig = m.locator('.lng-msg-tools').getByRole('button', { name: 'More', exact: true });
    if (await trig.count()) {
      await trig.first().tap();
      await p2.page.waitForTimeout(500);
      log('more-phone', await p2.page.evaluate(floatState, { w: 'More', pre: null }));
      await shot(p2.page, 'more-phone');
    } else log('more-phone', { ok: false, error: 'no More button after a tap' });
  } catch (e) {
    log('more-phone', { ok: false, error: String(e).slice(0, 200) });
  }
  await p2.ctx.close().catch(() => undefined);
}

// ---------------------------------------------------------------------------------------------------------------
// Item 2: tab strips at load
// ---------------------------------------------------------------------------------------------------------------

function tablists() {
  return Array.from(document.querySelectorAll<HTMLElement>('[role="tablist"]')).map((el) => {
    const tabs = Array.from(el.querySelectorAll<HTMLElement>('[role="tab"]'));
    const sel = tabs.findIndex((t) => t.getAttribute('aria-selected') === 'true');
    return {
      tabs: tabs.length,
      selected: sel,
      selectedText: (tabs[sel]?.textContent ?? '').trim().slice(0, 30),
      scrollLeft: Math.round(el.scrollLeft),
      scrollWidth: el.scrollWidth,
      clientWidth: el.clientWidth,
      overflow: el.getAttribute('data-overflow'),
    };
  });
}

async function tabsItem(browser: Browser, ownerCtx: BrowserContext) {
  const w = world();
  await setMain(ownerCtx, 'lounge-dark');
  const lines: string[] = [];
  const { ctx, page } = await newPage(browser, 'phone', 'lounge-dark', false, lines);
  const sample = async (id: string) => {
    for (const ms of [0, 300, 700]) {
      await page.waitForTimeout(ms);
      log(`${id}-${ms === 0 ? 'load' : ms === 300 ? '300ms' : '1s'}`, {
        strips: await page.evaluate(tablists),
      });
    }
  };
  try {
    await page.goto('/admin/dashboard', { waitUntil: 'domcontentloaded', timeout: 15_000 });
    await idle(page, 4000);
    await page
      .locator('[role="dialog"] nav ul li > button')
      .filter({ hasText: /moderation/i })
      .first()
      .evaluate((el) => (el as HTMLElement).click());
    await sample('tabs-moderation');
    await shot(page, 'tabs-moderation');
    // A late tab chosen by a tap still comes into view.
    if (!BASE) {
      const last = page.locator('[role="tablist"] [role="tab"]').last();
      await last.tap().catch(() => undefined);
      await page.waitForTimeout(500);
      log('tabs-moderation-late-tap', { strips: await page.evaluate(tablists) });
      const inView = await page.evaluate(() => {
        const t = document.querySelector('[role="tablist"] [role="tab"][aria-selected="true"]');
        const l = t?.parentElement?.getBoundingClientRect();
        const r = t?.getBoundingClientRect();
        return r && l ? r.left >= l.left - 1 && r.right <= l.right + 1 : null;
      });
      log('tabs-moderation-late-tap-in-view', { inView });
    }
    await page.goto(`/settings/server/${w.guildId}/overview`, {
      waitUntil: 'domcontentloaded',
      timeout: 15_000,
    });
    await idle(page, 3000);
    await sample('tabs-server-settings');
    await shot(page, 'tabs-server-settings');
    await page.goto(`/channels/${w.guildId}/${w.channelId}`, {
      waitUntil: 'domcontentloaded',
      timeout: 15_000,
    });
    await idle(page, 3000);
    const saved = page.getByRole('button', { name: /saved items/i }).first();
    if (await saved.count()) {
      await saved.evaluate((el) => (el as HTMLElement).click());
      await sample('tabs-saved');
      await shot(page, 'tabs-saved');
    } else log('tabs-saved', { ok: false, error: 'no Saved items button' });
  } catch (e) {
    log('tabs', { ok: false, error: String(e).slice(0, 300) });
  }
  await ctx.close().catch(() => undefined);
  appendFileSync(resolve(OUT, `${PRE}console.txt`), `${lines.join('\n')}\n`);
}

// ---------------------------------------------------------------------------------------------------------------
// Item 3: the invite and template cards
// ---------------------------------------------------------------------------------------------------------------

function cardInPage() {
  const rd = (n: number) => Math.round(n * 10) / 10;
  const bx = (e: Element) => {
    const r = e.getBoundingClientRect();
    return [rd(r.left), rd(r.top), rd(r.width), rd(r.height)];
  };
  const card = document.querySelector<HTMLElement>('.ent-invite-card');
  if (!card) return null;
  const cs = getComputedStyle(card);
  const r = card.getBoundingClientRect();
  const content = [
    rd(r.left + parseFloat(cs.paddingLeft) + parseFloat(cs.borderLeftWidth)),
    rd(r.width - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight)),
  ];
  const form = card.querySelector('form');
  const field = card.querySelector('form input');
  const btn = card.querySelector('form button[type=submit]');
  const de = document.documentElement;
  const outside = Array.from(card.querySelectorAll('*')).filter((e) => {
    const b = e.getBoundingClientRect();
    return b.width > 0 && (b.left < r.left - 1 || b.right > r.right + 1);
  }).length;
  return {
    theme: document.documentElement.getAttribute('data-theme'),
    card: bx(card),
    contentLeftAndWidth: content,
    form: form ? bx(form) : null,
    field: field ? bx(field) : null,
    button: btn ? bx(btn) : null,
    label: card.querySelector('form label') ? bx(card.querySelector('form label')!) : null,
    children: Array.from(card.children).map(
      (c) =>
        `${c.tagName.toLowerCase()}.${(c as HTMLElement).className.toString().slice(0, 24)}=${bx(c).join(',')}`,
    ),
    doc: de.scrollWidth - de.clientWidth,
    outside,
  };
}

async function cardsItem(browser: Browser, ownerCtx: BrowserContext) {
  const w = world();
  mkdirSync(OUT, { recursive: true });
  let tplCode: string | null = null;
  try {
    const tpl = await apiPost<{ code: string }>(
      ownerCtx.request,
      `/guilds/${w.guildId}/templates`,
      {
        name: 'P85 shots template',
        description: 'Made by the wave 85 kit spec; deleted at its end.',
      },
    );
    tplCode = tpl.code;
    const inv = await apiPost<{ code: string }>(
      ownerCtx.request,
      `/channels/${w.channelId}/invites`,
      {},
    );
    console.log(`REPORT made template ${tplCode} and invite ${inv.code}`);
    const cases: { id: string; path: string; size: Size; theme: string; out: boolean }[] = [];
    for (const size of ['phone', 'desktop'] as Size[]) {
      for (const theme of ['lounge-editorial', 'lounge-light', 'lounge-dark']) {
        cases.push({
          id: `template-app-${size}-${theme}`,
          path: `/template/${tplCode}`,
          size,
          theme,
          out: false,
        });
      }
      cases.push({
        id: `template-out-${size}`,
        path: `/template/${tplCode}`,
        size,
        theme: 'lounge-dark',
        out: true,
      });
      cases.push({
        id: `invite-app-${size}`,
        path: `/invite/${inv.code}`,
        size,
        theme: 'lounge-dark',
        out: false,
      });
      cases.push({
        id: `invite-out-${size}`,
        path: `/invite/${inv.code}`,
        size,
        theme: 'lounge-dark',
        out: true,
      });
    }
    let last = '';
    for (const c of cases) {
      if (!c.out && c.theme !== last) {
        await setMain(ownerCtx, c.theme);
        last = c.theme;
      }
      const { ctx, page } = await newPage(browser, c.size, c.theme, c.out);
      try {
        await page.goto(c.path, { waitUntil: 'domcontentloaded', timeout: 20_000 });
        await page.locator('.ent-invite-card').first().waitFor({ timeout: 15_000 });
        await idle(page, 3500);
        log(c.id, (await page.evaluate(cardInPage)) ?? { ok: false, error: 'no card' });
        await shot(page, c.id);
      } catch (e) {
        log(c.id, { ok: false, error: String(e).slice(0, 200) });
        await shot(page, `${c.id}-failed`).catch(() => undefined);
      }
      await ctx.close().catch(() => undefined);
    }
  } finally {
    if (tplCode) {
      const tok = (await (
        await ownerCtx.request.get('/api/v1/sr/csrf', { headers: { Origin: ORIGIN } })
      ).json()) as { token: string };
      const res = await ownerCtx.request.delete(
        `/api/v1/guilds/${w.guildId}/templates/${tplCode}`,
        {
          headers: { Origin: ORIGIN, 'X-SR-CSRF': tok.token },
        },
      );
      console.log(`REPORT template deleted: ${res.status()}`);
      const gone = await ownerCtx.request.get(`/api/v1/guilds/templates/${tplCode}`);
      console.log(`REPORT template read back after delete: ${gone.status()}`);
    }
  }
}

test('SEC-AUD-365, 366, 367: the More menu, a tab strip at load, the template and invite cards', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  if (MODE === 'more' || MODE === 'all') await moreItem(browser, ownerCtx);
  if (MODE === 'tabs' || MODE === 'all') await tabsItem(browser, ownerCtx);
  if (MODE === 'cards' || MODE === 'all') await cardsItem(browser, ownerCtx);
  await ownerCtx.close();
});
