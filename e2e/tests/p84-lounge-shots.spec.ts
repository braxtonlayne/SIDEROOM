import { appendFileSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-354, 355 and 356 (wave 84; D-858, D-859), on the sweep's seeded world (`e2e/.state/sweep/world.json`, made by
 * `page-sweep.spec.ts`'s "seed the sweep world"; this spec never edits that spec). The Owner's account. Local only.
 *  - SR_P84_LOUNGE=header: item 1. At 375x812 (touch, mobile user agent) in Lounge Dark and Light: every button of the
 *    channel header (and a DM's): box, the hit test (`elementFromPoint` 21px above and below the centre, and at the
 *    centre), the header's height and the title's box; a tap on Pinned Messages and on Inbox. At 1280: the boxes.
 *  - SR_P84_LOUNGE=more: item 2. At 1280x800, Lounge Dark: a message in the middle of the list, hover, click "More";
 *    three logs (click, 300 ms, 1 s; the pointer moved onto the menu's first item after the first), Escape and focus,
 *    the toolbar's hiding with nothing open. The same for the toolbar's emoji picker.
 *  - SR_P84_LOUNGE=profile: item 3. At 375 in Lounge Dark and Light, from a message's author and from the member list
 *    (a mouse click on the row after the swipe, as wave 83's spec does): the pop-up's boxes, the close button, the hit
 *    test, the focus after opening and after a tap, "View Full Profile". At 1280: display of the button, the boxes.
 *  - SR_P84_LOUNGE=all runs all three. SR_P84_BASE=1 names the files "base-..." (run with the change held back).
 * Shots and measure.json go to SR_P84_LOUNGE_OUT. No assertions: it measures and writes lines.
 */
const MODE = process.env.SR_P84_LOUNGE ?? '';
const ON = ['header', 'more', 'profile', 'all'].includes(MODE);
const BASE = process.env.SR_P84_BASE === '1';
const PRE = BASE ? 'base-' : '';
const OUT =
  process.env.SR_P84_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p84-lounge';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const WORLD_FILE = resolve(STATE, 'world.json');
const OWNER_STATE = resolve(STATE, 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SIZES = {
  phone: { width: 375, height: 812 },
  desktop: { width: 1280, height: 800 },
} as const;
type Size = keyof typeof SIZES;
const THEMES = ['lounge-dark', 'lounge-light'] as const;

test.skip(!ON, 'local visual check only');
test.setTimeout(1_200_000);

interface World {
  guildId: string;
  channelId: string;
  dmId: string | null;
}
const world = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

const log = (id: string, line: Record<string, unknown>) => {
  mkdirSync(OUT, { recursive: true });
  const row = { id: `${PRE}${id}`, ...line };
  appendFileSync(resolve(OUT, `${PRE}measure.json`), `${JSON.stringify(row)}\n`);
  console.log(`REPORT ${JSON.stringify(row).slice(0, 900)}`);
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

async function newPage(browser: Browser, size: Size, base: BrowserContext, theme: string) {
  const phone = size === 'phone';
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: SIZES[size],
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

async function hit(loc: Locator, size: Size, force = false) {
  const el = loc.first();
  await el.scrollIntoViewIfNeeded({ timeout: 4000 }).catch(() => undefined);
  if (size === 'phone') await el.tap({ force, timeout: 5000 });
  else await el.click({ force, timeout: 5000 });
  await el.page().waitForTimeout(450);
}

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

const shot = (page: Page, name: string) =>
  page.screenshot({ path: resolve(OUT, `${PRE}${name}.png`), timeout: 10_000 });

// ---------------------------------------------------------------------------------------------------------------
// Item 1: the channel header
// ---------------------------------------------------------------------------------------------------------------

interface HeaderBtn {
  name: string;
  cls: string;
  box: number[];
  hitAbove: boolean;
  hitBelow: boolean;
  centreIsSelf: boolean;
  /** The ::after's box, when there is one (target = the box of the pseudo-element). */
  after: string | null;
}
function headerInPage(): {
  buttons: HeaderBtn[];
  header: number[];
  title: number[] | null;
  doc: number;
} {
  const hdr = document.querySelector('.lng-header') as HTMLElement | null;
  const r2 = (r: DOMRect) => [
    Math.round(r.left),
    Math.round(r.top),
    Math.round(r.width),
    Math.round(r.height),
  ];
  if (!hdr) return { buttons: [], header: [], title: null, doc: 0 };
  const ok = (b: Element, t: Element | null) =>
    !!t && (t === b || b.contains(t) || t.closest('button') === b);
  const buttons: HeaderBtn[] = [];
  for (const b of Array.from(hdr.querySelectorAll<HTMLElement>('button'))) {
    const r = b.getBoundingClientRect();
    const cs = getComputedStyle(b);
    if (r.width === 0 || r.height === 0 || cs.display === 'none') continue;
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const above = document.elementFromPoint(cx, cy - 21);
    const below = document.elementFromPoint(cx, cy + 21);
    const mid = document.elementFromPoint(cx, cy);
    const a = getComputedStyle(b, '::after');
    buttons.push({
      name: b.getAttribute('aria-label') ?? (b.textContent ?? '').trim().slice(0, 30),
      cls: b.className
        .toString()
        .replace(/_[a-z0-9]{5,}/gi, '')
        .slice(0, 40),
      box: r2(r),
      hitAbove: ok(b, above),
      hitBelow: ok(b, below),
      centreIsSelf: ok(b, mid),
      after: a.content !== 'none' ? `${a.insetBlockStart}/${a.insetInlineStart}` : null,
    });
  }
  const title = hdr.querySelector('.lng-header-title');
  return {
    buttons,
    header: r2(hdr.getBoundingClientRect()),
    title: title ? r2(title.getBoundingClientRect()) : null,
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
  };
}

async function headerItem(browser: Browser, ownerCtx: BrowserContext) {
  const w = world();
  const chan = `/channels/${w.guildId}/${w.channelId}`;
  for (const theme of THEMES) {
    await setTheme(ownerCtx, theme);
    for (const size of ['phone', 'desktop'] as Size[]) {
      const id = `header-${size}-${theme}`;
      const { ctx, page } = await newPage(browser, size, ownerCtx, theme);
      try {
        await page.goto(chan, { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await idle(page, 4000);
        await page.locator('.lng-header').waitFor({ timeout: 8000 });
        await page.waitForTimeout(800);
        const m = await page.evaluate(headerInPage);
        log(id, {
          ...m,
          themeAttr: await page.evaluate(() =>
            document.querySelector('[data-theme]')?.getAttribute('data-theme'),
          ),
        });
        await shot(page, `header-${size}-${theme}`);
        if (size === 'phone') {
          // A tap on Pinned Messages opens it; a tap on Inbox opens it.
          for (const name of ['Pinned Messages', 'Inbox']) {
            const b = page.getByRole('button', { name, exact: true });
            await hit(b, size);
            const expanded = await b.first().getAttribute('aria-expanded');
            const open = await page.locator('[role="dialog"]:visible, .lng-inbox:visible').count();
            log(`${id}-tap-${name}`, { expanded, openDialogs: open });
            if (theme === 'lounge-dark')
              await shot(page, `header-phone-tap-${name.split(' ')[0]}-${theme}`);
            await page.keyboard.press('Escape');
            await page.waitForTimeout(400);
          }
        }
        if (w.dmId) {
          await page.goto(`/channels/@me/${w.dmId}`, {
            waitUntil: 'domcontentloaded',
            timeout: 15_000,
          });
          await idle(page, 4000);
          await page.locator('.lng-header').waitFor({ timeout: 8000 });
          await page.waitForTimeout(800);
          log(`${id}-dm`, await page.evaluate(headerInPage));
          if (size === 'phone') {
            await hit(page.getByRole('button', { name: /More actions|More/i }).first(), size).catch(
              () => undefined,
            );
            log(`${id}-dm-more-open`, await page.evaluate(headerInPage));
          }
        }
      } catch (e) {
        log(id, { ok: false, error: String(e).slice(0, 300) });
        await shot(page, `${id}-failed`).catch(() => undefined);
      }
      await ctx.close().catch(() => undefined);
    }
  }
}

// ---------------------------------------------------------------------------------------------------------------
// Item 2: the message "More" menu
// ---------------------------------------------------------------------------------------------------------------

async function moreState(page: Page, what: string) {
  return page.evaluate((w) => {
    const r2 = (r: DOMRect) => [
      Math.round(r.left),
      Math.round(r.top),
      Math.round(r.width),
      Math.round(r.height),
    ];
    const msg = document.querySelector('[data-p84]') as HTMLElement | null;
    const tools = msg?.querySelector('.lng-msg-tools') as HTMLElement | null;
    const trig = tools?.querySelector(`button[aria-label="${w}"]`) as HTMLElement | null;
    const menu = Array.from(
      document.querySelectorAll<HTMLElement>('[role="menu"], [role="dialog"]'),
    )
      .filter((e) => e.getBoundingClientRect().width > 0 && !e.closest('.lng-msg'))
      .pop();
    const tr = trig?.getBoundingClientRect();
    const mr = menu?.getBoundingClientRect();
    return {
      trigger: tr ? r2(tr) : null,
      triggerExpanded: trig?.getAttribute('aria-expanded') ?? null,
      toolsDisplay: tools ? getComputedStyle(tools).display : null,
      menu: mr ? r2(mr) : null,
      menuRole: menu?.getAttribute('role') ?? null,
      /** menu's inline-end edge minus the trigger's (bottom-end: 0). */
      endDelta: tr && mr ? Math.round(mr.right - tr.right) : null,
      inViewport: mr
        ? mr.left >= 0 && mr.top >= 0 && mr.right <= innerWidth && mr.bottom <= innerHeight
        : null,
      active: document.activeElement
        ? `${document.activeElement.tagName.toLowerCase()}[${document.activeElement.getAttribute('aria-label') ?? ''}]`
        : null,
    };
  }, what);
}

async function moreItem(browser: Browser, ownerCtx: BrowserContext) {
  const w = world();
  await setTheme(ownerCtx, 'lounge-dark');
  const { ctx, page } = await newPage(browser, 'desktop', ownerCtx, 'lounge-dark');
  try {
    await page.goto(`/channels/${w.guildId}/${w.channelId}`, {
      waitUntil: 'domcontentloaded',
      timeout: 15_000,
    });
    await idle(page, 4000);
    const msgs = page
      .locator('.lng-msg')
      .filter({ has: page.getByRole('button', { name: /^Open profile of / }) });
    const n = await msgs.count();
    log('more-count', { messages: n });
    const idx = Math.max(0, Math.floor(n / 2));
    const m = msgs.nth(idx);
    await m.scrollIntoViewIfNeeded();
    await m.evaluate((e) => e.setAttribute('data-p84', '1'));
    for (const what of ['More', 'Add Reaction']) {
      const tag = what === 'More' ? 'more' : 'emoji';
      await page.keyboard.press('Escape');
      await page.mouse.move(5, 5);
      await page.waitForTimeout(500);
      await m.scrollIntoViewIfNeeded();
      const box = await m.boundingBox();
      if (!box) throw new Error('no message box');
      log(`${tag}-message`, { box, scrollerMid: idx });
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      await page.waitForTimeout(300);
      const trig = m.locator('.lng-msg-tools').getByRole('button', { name: what, exact: true });
      await trig.click();
      await page.waitForTimeout(100);
      const s0 = await moreState(page, what);
      log(`${tag}-t0-click`, s0);
      await shot(page, `${tag}-menu-open`);
      // The pointer moves onto the menu's first item (or into the popover).
      const target = page
        .locator('[role="menu"] [role="menuitem"], [role="dialog"] button')
        .first();
      const tb = await target.boundingBox();
      if (tb) await page.mouse.move(tb.x + tb.width / 2, tb.y + tb.height / 2);
      await page.waitForTimeout(250);
      log(`${tag}-t300`, await moreState(page, what));
      await page.waitForTimeout(700);
      const s1 = await moreState(page, what);
      log(`${tag}-t1000`, s1);
      await shot(page, `${tag}-menu-1s`);
      await page.keyboard.press('Escape');
      await page.waitForTimeout(400);
      log(`${tag}-escape`, await moreState(page, what));
      // With nothing open the toolbar hides when the pointer leaves.
      await page.mouse.move(5, 5);
      await page.waitForTimeout(500);
      log(`${tag}-hides`, await moreState(page, what));
    }
  } catch (e) {
    log('more', { ok: false, error: String(e).slice(0, 300) });
    await shot(page, 'more-failed').catch(() => undefined);
  }
  await ctx.close().catch(() => undefined);
}

// ---------------------------------------------------------------------------------------------------------------
// Item 3: the profile pop-up's close button
// ---------------------------------------------------------------------------------------------------------------

function popInPage() {
  const r2 = (r: DOMRect) => [
    Math.round(r.left),
    Math.round(r.top),
    Math.round(r.width),
    Math.round(r.height),
  ];
  const pop = document.querySelector('.lng-pop') as HTMLElement | null;
  if (!pop) return null;
  const q = (s: string) => {
    const e = pop.querySelector(s);
    return e ? r2(e.getBoundingClientRect()) : null;
  };
  const lum = (c: string) => {
    const m = c.match(/[\d.]+/g)?.map(Number) ?? [0, 0, 0];
    const f = (v: number) => {
      const x = v / 255;
      return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * f(m[0]!) + 0.7152 * f(m[1]!) + 0.0722 * f(m[2]!);
  };
  const close = pop.querySelector('.lng-pop-close') as HTMLElement | null;
  let closeInfo: Record<string, unknown> | null = null;
  if (close) {
    const cs = getComputedStyle(close);
    const r = close.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const ok = (t: Element | null) => !!t && (t === close || close.contains(t));
    const a = lum(cs.color);
    const b = lum(cs.backgroundColor);
    closeInfo = {
      display: cs.display,
      label: close.getAttribute('aria-label'),
      box: r2(r),
      hitAbove: ok(document.elementFromPoint(cx, cy - 21)),
      hitBelow: ok(document.elementFromPoint(cx, cy + 21)),
      contrast: Math.round(((Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)) * 100) / 100,
      color: cs.color,
      ground: cs.backgroundColor,
      inside:
        r.left >= pop.getBoundingClientRect().left &&
        r.right <= pop.getBoundingClientRect().right &&
        r.top >= pop.getBoundingClientRect().top,
    };
  }
  return {
    pop: r2(pop.getBoundingClientRect()),
    avatar: q('.lng-pop-head'),
    name: q('.lng-pop-name'),
    handle: q('.lng-pop-handle'),
    badges: q('.lng-pop-body > ul, .lng-pop-badges, [class*="badge"]'),
    banner: q('.lng-pop-banner, [class*="banner"]'),
    close: closeInfo,
    active: document.activeElement
      ? `${document.activeElement.tagName.toLowerCase()}.${String(document.activeElement.className).split(/\s+/)[0]}[${document.activeElement.getAttribute('aria-label') ?? ''}]`
      : null,
    vw: document.documentElement.clientWidth,
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
  };
}

async function profileItem(browser: Browser, ownerCtx: BrowserContext) {
  const w = world();
  const chan = `/channels/${w.guildId}/${w.channelId}`;
  for (const theme of THEMES) {
    await setTheme(ownerCtx, theme);
    for (const size of ['phone', 'desktop'] as Size[]) {
      if (size === 'desktop' && theme !== 'lounge-dark') continue;
      for (const from of ['message', 'members'] as const) {
        if (size === 'desktop' && from === 'members') continue;
        if (BASE && !(theme === 'lounge-dark' || size === 'desktop')) continue;
        const id = `profile-${from}-${size}-${theme}`;
        const { ctx, page } = await newPage(browser, size, ownerCtx, theme);
        try {
          await page.goto(chan, { waitUntil: 'domcontentloaded', timeout: 15_000 });
          await idle(page, 4000);
          const openIt = async () => {
            if (from === 'message') {
              await hit(page.getByRole('button', { name: /^Open profile of / }), size, true);
            } else {
              await swipeToMembers(page);
              await page
                .locator('.lng-right .lng-member')
                .nth(0)
                .click({ timeout: 5000, force: true });
              await page.waitForTimeout(900);
            }
            await page.locator('.lng-pop').waitFor({ timeout: 6000 });
            await page.waitForTimeout(700);
          };
          await openIt();
          const m = await page.evaluate(popInPage);
          log(id, {
            theme: await page.evaluate(() =>
              document.querySelector('[data-theme]')?.getAttribute('data-theme'),
            ),
            ...m,
          });
          await shot(page, id);
          const names = await page
            .locator('.lng-pop button, .lng-pop [role="button"]')
            .evaluateAll((l) =>
              l.map((b) => b.getAttribute('aria-label') ?? b.textContent?.trim()).slice(0, 12),
            );
          log(`${id}-buttons`, { names, hasClose: names.includes('Close') });
          if (size === 'phone' && !BASE) {
            // A tap on the button closes the pop-up (a mouse click after the swipe when the synthetic gesture sends no click).
            const btn = page.locator('.lng-pop .lng-pop-close');
            let how = 'tap';
            await btn.tap({ timeout: 4000 }).catch(async () => {
              how = 'click (the tap failed)';
              await btn.click({ timeout: 4000 });
            });
            await page.waitForTimeout(500);
            const gone = (await page.locator('.lng-pop').count()) === 0;
            log(`${id}-tap-close`, {
              how,
              gone,
              focus: await page.evaluate(() =>
                document.activeElement
                  ? `${document.activeElement.tagName.toLowerCase()}[${document.activeElement.getAttribute('aria-label') ?? ''}]`
                  : null,
              ),
            });
            // The full profile still opens from the pop-up.
            await openIt();
            await page.getByRole('button', { name: 'View Full Profile' }).click({ timeout: 4000 });
            await page.waitForTimeout(900);
            log(`${id}-full`, {
              fullOpen: await page.locator('[role="dialog"]').count(),
              popStill: await page.locator('.lng-pop').count(),
            });
          } else if (size === 'phone') {
            // Base: what an outside tap leaves focused.
            await page.mouse.click(5, 5);
            await page.waitForTimeout(500);
            log(`${id}-outside`, {
              gone: (await page.locator('.lng-pop').count()) === 0,
              focus: await page.evaluate(() =>
                document.activeElement
                  ? `${document.activeElement.tagName.toLowerCase()}[${document.activeElement.getAttribute('aria-label') ?? ''}]`
                  : null,
              ),
            });
          }
        } catch (e) {
          log(id, { ok: false, error: String(e).slice(0, 300) });
          await shot(page, `${id}-failed`).catch(() => undefined);
        }
        await ctx.close().catch(() => undefined);
      }
    }
  }
}

test('SEC-AUD-354, 355, 356: the channel header on a phone, the message More menu, the profile close button', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  if (MODE === 'header' || MODE === 'all') await headerItem(browser, ownerCtx);
  if (MODE === 'more' || MODE === 'all') await moreItem(browser, ownerCtx);
  if (MODE === 'profile' || MODE === 'all') await profileItem(browser, ownerCtx);
  await ownerCtx.close();
});
