import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-347 and SEC-AUD-348 (wave 83; D-853), on the sweep's seeded world (`e2e/.state/sweep/world.json`, made by
 * `page-sweep.spec.ts`'s "seed the sweep world"; this spec never edits that spec). The Owner's account.
 *  - SR_P83_LOUNGE=thread: item 1. A thread open at 375x812 (touch, mobile user agent) and 1280x800: every node the
 *    "Member List" query of wave 82 matches and every node the thread's own header offers (box, visible, what
 *    `elementFromPoint` finds at its centre, height), a real tap on the thread's control, a swipe as in the channel,
 *    the state measured (document and main scroller sideways scroll), shots in Lounge Dark and Lounge Light.
 *  - SR_P83_LOUNGE=overlays: item 2. Each overlay opened by its own control, one at a time, measured at 375 and
 *    1280 (Lounge Dark): its box and how far an edge is outside the viewport, the document's sideways scroll, its
 *    close control, its own sideways scroll. One JSON line each in `measure.json`; a shot of every overlay that has
 *    a fault (`SR_P83_SHOTS=all` shoots every overlay).
 *  - SR_P83_LOUNGE=all runs both. SR_P83_ONLY is a regex over overlay ids.
 * Local only; shots and measure.json go to SR_P83_LOUNGE_OUT; a limit of 20 s per overlay; no retries.
 */
const MODE = process.env.SR_P83_LOUNGE ?? '';
const ON = MODE === 'thread' || MODE === 'overlays' || MODE === 'all';
const OUT =
  process.env.SR_P83_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p83-lounge';
const SHOTS = process.env.SR_P83_SHOTS ?? '';
const ONLY = process.env.SR_P83_ONLY ? new RegExp(process.env.SR_P83_ONLY) : null;
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
const LIMIT_MS = 20_000;

test.skip(!ON, 'local visual check only');
test.setTimeout(1_800_000);

interface World {
  guildId: string;
  channelId: string;
  threadId: string;
  dmId: string | null;
  memberName: string;
}
const world = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

const log = (file: string, line: Record<string, unknown>) => {
  mkdirSync(OUT, { recursive: true });
  appendFileSync(resolve(OUT, file), `${JSON.stringify(line)}\n`);
  console.log(`REPORT ${JSON.stringify(line).slice(0, 700)}`);
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

/** A hit on a control: a real finger tap on the phone, a click at 1280. `force` for a control that shows on hover. */
async function hit(loc: Locator, size: Size, force = false) {
  const el = loc.first();
  await el.scrollIntoViewIfNeeded({ timeout: 4000 }).catch(() => undefined);
  if (size === 'phone') await el.tap({ force, timeout: 5000 });
  else await el.click({ force, timeout: 5000 });
  await el.page().waitForTimeout(450);
}

/** A real finger swipe to the member list pane (DevTools touch events, as p69's spec does). */
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

/** A real long press (500 ms in the kit) at a point, through the DevTools touch events. */
async function longPress(page: Page, x: number, y: number) {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
  await page.waitForTimeout(800);
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await cdp.detach();
  await page.waitForTimeout(450);
}

// ---------------------------------------------------------------------------------------------------------------
// Measuring, in the page
// ---------------------------------------------------------------------------------------------------------------

interface OverlayMeasure {
  found: boolean;
  tag?: string;
  cls?: string;
  box?: number[];
  out?: { left: number; right: number; top: number; bottom: number };
  doc: number;
  vw: number;
  vh: number;
  scroller: number;
  inner: string[];
  close: { box: number[]; h: number; inside: boolean; label: string } | null;
  theme: string | null;
}

function measureOverlay(arg: { sel: string | null; closeSel: string | null }): OverlayMeasure {
  const de = document.documentElement;
  const vw = de.clientWidth;
  const vh = de.clientHeight;
  const vis = (e: Element) => {
    const r = e.getBoundingClientRect();
    const cs = getComputedStyle(e);
    return r.width > 0 && r.height > 0 && cs.display !== 'none' && cs.visibility !== 'hidden';
  };
  const pool = arg.sel
    ? Array.from(document.querySelectorAll(arg.sel))
    : Array.from(document.querySelectorAll('[role="dialog"], [role="menu"]'));
  const all = pool.filter(vis);
  const el = all[all.length - 1] ?? null;
  const themed = document.querySelector('[data-theme]');
  const cands = Array.from(document.querySelectorAll<HTMLElement>('body *')).filter((e) => {
    if (e.clientWidth === 0) return false;
    const cs = getComputedStyle(e);
    return (
      cs.display !== 'none' &&
      /(auto|scroll|overlay)/.test(cs.overflowY) &&
      e.clientWidth * e.clientHeight >= vw * vh * 0.2
    );
  });
  cands.sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight);
  const main = cands[0];
  const base = {
    doc: de.scrollWidth - vw,
    vw,
    vh,
    scroller: main ? main.scrollWidth - main.clientWidth : 0,
    theme: themed?.getAttribute('data-theme') ?? null,
  };
  if (!el) return { found: false, ...base, inner: [], close: null };
  const r = el.getBoundingClientRect();
  const inner: string[] = [];
  for (const e of [el, ...Array.from(el.querySelectorAll<HTMLElement>('*'))]) {
    const h = e as HTMLElement;
    if (h.scrollWidth > h.clientWidth + 1 && h.clientWidth > 0) {
      const ox = getComputedStyle(h).overflowX;
      const cls = typeof h.className === 'string' ? h.className.trim().split(/\s+/)[0] : '';
      inner.push(
        `${h.tagName.toLowerCase()}.${cls} over ${h.scrollWidth - h.clientWidth} overflow-x:${ox}`,
      );
    }
    if (inner.length >= 6) break;
  }
  let closeEl: Element | null = null;
  const scope = el.closest('[data-sr-part="modal-backdrop"]') ?? el;
  if (arg.closeSel) closeEl = document.querySelector(arg.closeSel);
  else
    closeEl =
      scope.querySelector('button[aria-label="Close"], button[aria-label^="Close" i]') ?? null;
  let close: OverlayMeasure['close'] = null;
  if (closeEl) {
    const c = closeEl.getBoundingClientRect();
    close = {
      box: [Math.round(c.left), Math.round(c.top), Math.round(c.width), Math.round(c.height)],
      h: Math.round(c.height),
      inside: c.left >= 0 && c.top >= 0 && c.right <= vw && c.bottom <= vh,
      label: closeEl.getAttribute('aria-label') ?? '',
    };
  }
  return {
    found: true,
    tag: el.tagName.toLowerCase(),
    cls: typeof el.className === 'string' ? el.className.slice(0, 70) : '',
    box: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)],
    out: {
      left: Math.max(0, Math.round(-r.left)),
      right: Math.max(0, Math.round(r.right - vw)),
      top: Math.max(0, Math.round(-r.top)),
      bottom: Math.max(0, Math.round(r.bottom - vh)),
    },
    ...base,
    inner,
    close,
  };
}

// ---------------------------------------------------------------------------------------------------------------
// Item 1: the thread's member list
// ---------------------------------------------------------------------------------------------------------------

interface NodeInfo {
  name: string;
  tag: string;
  cls: string;
  box: number[];
  visible: boolean;
  inThread: boolean;
  inChannelHeader: boolean;
  top: string;
  topIsSelf: boolean;
  pressed: string | null;
  expanded: string | null;
}

function nodesInPage(re: string): NodeInfo[] {
  const rx = new RegExp(re, 'i');
  const out: NodeInfo[] = [];
  for (const b of Array.from(document.querySelectorAll<HTMLElement>('button, [role="button"]'))) {
    const name = b.getAttribute('aria-label') ?? b.textContent?.trim() ?? '';
    if (!rx.test(name)) continue;
    const r = b.getBoundingClientRect();
    const cs = getComputedStyle(b);
    const visible =
      r.width > 0 && r.height > 0 && cs.display !== 'none' && cs.visibility !== 'hidden';
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const top = visible ? document.elementFromPoint(cx, cy) : null;
    const topIsSelf = !!top && (top === b || b.contains(top));
    out.push({
      name,
      tag: b.tagName.toLowerCase(),
      cls: b.className.toString().slice(0, 50),
      box: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)],
      visible,
      inThread: !!b.closest('.lng-thread-pane'),
      inChannelHeader: !!b.closest('.lng-header'),
      top: top
        ? `${top.tagName.toLowerCase()}.${top.className.toString().split(/\s+/)[0]}${top.closest('.lng-thread-pane') ? ' (thread pane)' : ''}`
        : 'none',
      topIsSelf,
      pressed: b.getAttribute('aria-pressed'),
      expanded: b.getAttribute('aria-expanded'),
    });
  }
  return out;
}

async function threadItem(browser: Browser, ownerCtx: BrowserContext) {
  const w = world();
  const path = `/channels/${w.guildId}/${w.channelId}/threads/${w.threadId}`;
  for (const theme of ['lounge-dark', 'lounge-light']) {
    await setTheme(ownerCtx, theme);
    for (const size of ['desktop', 'phone'] as Size[]) {
      const { ctx, page } = await newPage(browser, size, ownerCtx, theme);
      const id = `thread-${size}-${theme}`;
      try {
        await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await idle(page, 4000);
        await page.locator('.lng-thread-pane').waitFor({ timeout: 8000 });
        const m0 = await page.evaluate(measureOverlay, { sel: '.lng-thread-pane', closeSel: null });
        log('measure.json', { id: `${id}-pane`, ...m0 });
        // Every control of the thread's own header: its box (a phone control under 44px tall is a fault, D-846).
        const head = await page.evaluate(() =>
          Array.from(document.querySelectorAll('.lng-thread-head button')).map((b) => {
            const r = b.getBoundingClientRect();
            const a = getComputedStyle(b, '::after');
            return {
              name: b.getAttribute('aria-label') ?? (b.textContent ?? '').trim(),
              box: [
                Math.round(r.left),
                Math.round(r.top),
                Math.round(r.width),
                Math.round(r.height),
              ],
              afterHitArea: a.content !== 'none' ? [a.insetInlineStart, a.insetInlineEnd] : null,
            };
          }),
        );
        log('measure.json', { id: `${id}-head`, head });
        const title = await page.evaluate(() => {
          const t = document.querySelector('.lng-thread-title');
          return t ? { w: Math.round(t.getBoundingClientRect().width), text: t.textContent } : null;
        });
        log('measure.json', { id: `${id}-title`, title });
        // Every node the wave 82 query matches, and every node of the thread header that is about members.
        const wave82 = await page.evaluate(nodesInPage, 'Member List');
        const own = await page.evaluate(nodesInPage, 'Thread members|Member List|Members');
        log('measure.json', { id: `${id}-nodes`, wave82Query: wave82, memberControls: own });
        if (theme === 'lounge-dark') {
          // The wave 82 spec's own click (`.first()`), to see what it picks and why it times out.
          const first = page.getByRole('button', { name: /Member List/ }).first();
          let res = 'clicked';
          await first.click({ timeout: 3000 }).catch((e) => {
            res = `click failed: ${String(e).split('\n')[0]?.slice(0, 160)} ${String(e).includes('intercepts pointer events') ? '(covered)' : ''}`;
          });
          log('measure.json', { id: `${id}-wave82-click`, result: res });
          await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
          await idle(page, 4000);
        }
        // The thread's own control: a tap (a click at 1280) opens the list; again closes it.
        const ctl = page
          .locator('.lng-thread-pane')
          .getByRole('button', { name: /Thread members/ });
        const box = await ctl.first().boundingBox();
        await hit(ctl, size);
        const dlg = '[role="dialog"][aria-label="Thread members"]';
        await page.locator(dlg).waitFor({ timeout: 5000 });
        await page.waitForTimeout(600);
        const m1 = await page.evaluate(measureOverlay, { sel: dlg, closeSel: null });
        log('measure.json', { id: `${id}-open`, controlBox: box, ...m1 });
        await page.screenshot({ path: resolve(OUT, `${id}-members-open.png`), timeout: 10_000 });
        // Close: a second tap on the control, then Escape; focus must be back on the control.
        await hit(ctl, size);
        const gone2 = (await page.locator(dlg).count()) === 0;
        let how = 'second tap';
        if (!gone2) {
          await page.keyboard.press('Escape');
          await page.waitForTimeout(400);
          how = 'Escape (a second tap left it open)';
        }
        const focusBack = await page.evaluate(
          () =>
            document.activeElement?.getAttribute('aria-label') ?? document.activeElement?.tagName,
        );
        log('measure.json', {
          id: `${id}-close`,
          closedBy: how,
          goneAfterSecondTap: gone2,
          focus: focusBack,
        });
        if (size === 'phone' && theme === 'lounge-dark') {
          // A swipe as in the channel: does the channel's own member list come up over the thread?
          await swipeToMembers(page);
          const right = await page.evaluate(() => {
            const a = document.querySelector('.lng-right') as HTMLElement | null;
            if (!a) return null;
            const r = a.getBoundingClientRect();
            const top = document.elementFromPoint(r.left + r.width / 2, r.top + 200);
            return {
              box: [
                Math.round(r.left),
                Math.round(r.top),
                Math.round(r.width),
                Math.round(r.height),
              ],
              inert: a.hasAttribute('inert'),
              topAtCentre: top
                ? `${top.tagName.toLowerCase()}.${top.className.toString().split(/\s+/)[0]}${top.closest('.lng-thread-pane') ? ' (thread pane)' : ''}`
                : 'none',
              vw: document.documentElement.clientWidth,
            };
          });
          log('measure.json', { id: `${id}-swipe`, right });
          await page.screenshot({ path: resolve(OUT, `${id}-after-swipe.png`), timeout: 10_000 });
        }
      } catch (e) {
        log('measure.json', { id, ok: false, error: String(e).slice(0, 300) });
        await page.screenshot({ path: resolve(OUT, `${id}-failed.png`) }).catch(() => undefined);
      }
      await ctx.close().catch(() => undefined);
    }
  }
}

// ---------------------------------------------------------------------------------------------------------------
// Item 2: the overlays
// ---------------------------------------------------------------------------------------------------------------

interface Overlay {
  id: string;
  /** Where to start: the channel (default), the guild's channel list, or Home. */
  at?: 'channel' | 'list' | 'home' | 'dm';
  /** Sizes it exists at (default both). */
  only?: Size;
  /** Selector of the overlay's own root when it is not a dialog or menu. */
  sel?: string;
  closeSel?: string;
  open: (page: Page, size: Size, w: World) => Promise<void>;
  /** How it closes (default: Escape). */
  close?: (page: Page, size: Size) => Promise<void>;
  /** The overlay closes by Escape or an outside tap only. */
  closeBy?: string;
}

const wrap = (p: Page) => p.locator('.lng-composer-wrap').last();
const lastMsg = (p: Page) =>
  p
    .locator('.lng-msg')
    .filter({ has: p.getByRole('button', { name: /^Open profile of / }) })
    .last();

const OVERLAYS: Overlay[] = [
  {
    id: 'emoji-picker-composer',
    open: (p, s) => hit(wrap(p).getByRole('button', { name: /^Open emoji picker$/ }), s),
  },
  {
    id: 'emoji-picker-reaction-pill',
    open: (p, s) => hit(p.getByRole('button', { name: /^Add reaction to this message$/ }), s, true),
  },
  {
    id: 'emoji-picker-message-toolbar',
    only: 'desktop',
    open: async (p, s) => {
      await lastMsg(p).hover();
      await hit(lastMsg(p).getByRole('button', { name: /^Add Reaction$/ }), s);
    },
  },
  {
    id: 'gif-picker',
    open: (p, s) => hit(wrap(p).getByRole('button', { name: /^Open GIF picker$/ }), s),
  },
  {
    id: 'sticker-picker',
    open: (p, s) => hit(wrap(p).getByRole('button', { name: /^Open sticker picker$/ }), s),
  },
  {
    id: 'profile-from-message-author',
    open: (p, s) => hit(p.getByRole('button', { name: /^Open profile of / }), s, true),
  },
  {
    id: 'profile-from-member-list',
    open: async (p, s) => {
      if (s === 'phone') await swipeToMembers(p);
      else {
        const b = p.getByRole('button', { name: /Member List/ }).first();
        if ((await b.getAttribute('aria-pressed')) !== 'true') await hit(b, s);
      }
      const row = p.locator('.lng-right .lng-member');
      // After a swipe made through the DevTools protocol the browser sends no click for the next finger tap (the touch
      // events arrive, a click does not); a mouse click on the row is what opens the profile, so that is what is used.
      await row.nth(0).click({ timeout: 5000, force: true });
      await p.waitForTimeout(900);
    },
  },
  {
    id: 'message-menu',
    open: async (p, s) => {
      if (s === 'phone') {
        const r = await lastMsg(p).boundingBox();
        if (!r) throw new Error('no message');
        await longPress(
          p,
          r.x + Math.min(120, r.width / 2),
          Math.min(Math.max(r.y + 20, 130), r.y + r.height - 8, 600),
        );
      } else {
        await lastMsg(p).hover();
        await hit(lastMsg(p).getByRole('button', { name: /^More$/ }), s);
      }
    },
  },
  {
    id: 'pins',
    open: (p, s) => hit(p.getByRole('button', { name: 'Pinned Messages' }), s),
  },
  {
    id: 'search-results',
    open: async (p, s) => {
      if (s === 'phone') await hit(p.getByRole('button', { name: 'Search this conversation' }), s);
      const box = p.getByRole('searchbox').first();
      await box.fill('the');
      await box.press('Enter');
      await p.waitForTimeout(1200);
    },
    sel: '.lng-search-sheet, .lng-search-panel',
    closeBy: 'Escape or the search button',
  },
  {
    id: 'inbox',
    sel: '.lng-inbox',
    open: (p, s) => hit(p.getByRole('button', { name: 'Inbox' }), s),
  },
  {
    id: 'channel-list-drawer',
    only: 'phone',
    sel: '.lng-sidebar',
    open: (p, s) => hit(p.getByRole('button', { name: 'Back to channels' }), s),
    closeBy: 'a pane: choosing a channel or a swipe',
  },
  {
    id: 'member-list-drawer',
    sel: '.lng-right',
    open: async (p, s) => {
      if (s === 'phone') await swipeToMembers(p);
      else {
        const b = p.getByRole('button', { name: /Member List/ }).first();
        if ((await b.getAttribute('aria-pressed')) !== 'true') await hit(b, s);
      }
    },
    closeBy: 'a pane: a swipe or the header button',
  },
  {
    id: 'channel-notification-settings',
    open: (p, s) => hit(p.getByRole('button', { name: 'Notification Settings' }), s),
  },
  {
    id: 'create-channel',
    at: 'list',
    open: (p, s) => hit(p.getByRole('button', { name: /^Create Channel in / }), s, true),
  },
  {
    id: 'create-server',
    at: 'list',
    open: (p, s) => hit(p.getByRole('button', { name: /^Add a Server$/ }), s, true),
  },
  {
    id: 'invite-people',
    at: 'list',
    open: async (p, s) => {
      await hit(p.locator('.lng-guild-header'), s);
      await hit(p.getByRole('menuitem', { name: /Invite People|Invite to Server/ }), s);
    },
  },
  {
    id: 'server-menu',
    at: 'list',
    open: (p, s) => hit(p.locator('.lng-guild-header'), s),
  },
  {
    id: 'status-menu',
    at: 'list',
    open: (p, s) => hit(p.getByRole('button', { name: /^Set status, currently / }), s),
  },
  {
    id: 'tools-drawer',
    at: 'home',
    open: (p, s) => hit(p.locator('.lng-find'), s),
  },
];

async function overlays(browser: Browser, ownerCtx: BrowserContext) {
  const w = world();
  await setTheme(ownerCtx, 'lounge-dark');
  const counts: Record<string, { opened: number; faults: number; notReached: string[] }> = {
    phone: { opened: 0, faults: 0, notReached: [] },
    desktop: { opened: 0, faults: 0, notReached: [] },
  };
  for (const size of ['phone', 'desktop'] as Size[]) {
    let { ctx, page } = await newPage(browser, size, ownerCtx, 'lounge-dark');
    for (const o of OVERLAYS) {
      if (ONLY && !ONLY.test(o.id)) continue;
      if (o.only && o.only !== size) continue;
      const at = o.at ?? 'channel';
      const path =
        at === 'channel'
          ? `/channels/${w.guildId}/${w.channelId}`
          : at === 'list'
            ? size === 'phone'
              ? `/channels/${w.guildId}`
              : `/channels/${w.guildId}/${w.channelId}`
            : at === 'dm' && w.dmId
              ? `/channels/@me/${w.dmId}`
              : '/channels/@me';
      let timer: NodeJS.Timeout | undefined;
      try {
        await Promise.race([
          (async () => {
            await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
            await idle(page, 4000);
            await page.waitForTimeout(500);
            await o.open(page, size, w);
            await page.waitForTimeout(500);
            const m = await page.evaluate(measureOverlay, {
              sel: o.sel ?? null,
              closeSel: o.closeSel ?? null,
            });
            const outAny = m.out ? Math.max(m.out.left, m.out.right, m.out.top, m.out.bottom) : 0;
            const closeBox = m.close;
            const faults: string[] = [];
            if (!m.found) faults.push('overlay not found');
            if (outAny > 0) faults.push(`edge outside the viewport by ${outAny}px`);
            if (m.doc > 0) faults.push(`document scrolls sideways by ${m.doc}px`);
            if (closeBox && !closeBox.inside) faults.push('close control outside the viewport');
            if (closeBox && size === 'phone' && closeBox.h < 44)
              faults.push(`close control ${closeBox.h}px tall`);
            const real = faults.filter((f) => f !== 'overlay not found');
            const fault = real.length > 0;
            const shot = fault || !m.found || SHOTS === 'all' ? `${o.id}-${size}.png` : null;
            if (shot) await page.screenshot({ path: resolve(OUT, shot), timeout: 10_000 });
            log('measure.json', {
              id: o.id,
              size,
              ok: m.found,
              ...m,
              closeBy: closeBox
                ? `its own control (${closeBox.label})`
                : (o.closeBy ?? 'Escape or an outside tap'),
              faults,
              shot,
            });
            if (m.found) counts[size]!.opened++;
            if (fault) counts[size]!.faults++;
            if (!m.found)
              counts[size]!.notReached.push(`${o.id} (opened, but no overlay root found)`);
            // Close it again before the next.
            if (o.close) await o.close(page, size);
            else await page.keyboard.press('Escape');
          })(),
          new Promise<never>((_, rej) => {
            timer = setTimeout(() => rej(new Error(`${o.id}: over ${LIMIT_MS} ms`)), LIMIT_MS);
          }),
        ]);
      } catch (e) {
        counts[size]!.notReached.push(`${o.id}: ${String(e).split('\n')[0]?.slice(0, 160)}`);
        log('measure.json', { id: o.id, size, ok: false, error: String(e).slice(0, 300) });
        await ctx.close().catch(() => undefined);
        ({ ctx, page } = await newPage(browser, size, ownerCtx, 'lounge-dark'));
      } finally {
        if (timer) clearTimeout(timer);
      }
    }
    await ctx.close().catch(() => undefined);
  }
  console.log(`REPORT totals ${JSON.stringify(counts)}`);
  writeFileSync(resolve(OUT, 'totals.json'), JSON.stringify(counts, null, 1));
}

test('SEC-AUD-347 and SEC-AUD-348: a thread member list on a phone; the overlays inside the viewport', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  if (MODE === 'thread' || MODE === 'all') await threadItem(browser, ownerCtx);
  if (MODE === 'overlays' || MODE === 'all') await overlays(browser, ownerCtx);
  await ownerCtx.close();
});
