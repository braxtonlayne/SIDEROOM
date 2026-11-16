import { appendFileSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';

/**
 * SEC-AUD-368 (wave 85; D-872): a channel's header on a phone, on the sweep's seeded world (`e2e/.state/sweep/world.json`,
 * made by `page-sweep.spec.ts`'s "seed the sweep world"; this spec never edits that spec). The Owner's account. Local
 * only. SR_P85_LOUNGE=1 runs it; SR_P85_BASE=1 names the files "base-..." and skips what only the change has (the
 * strip). At 375x812 (touch, mobile user agent) in Lounge Dark and Light: the header's buttons (box, the hit test 21 px
 * above and below the centre, the target's box from the ::after, whether it covers a neighbour's centre or reaches
 * above the strip), the title's box and its scrollWidth against clientWidth, the header's height, the document's
 * scrollWidth; the strip closed and open, a tap on More twice, Escape, a tap on Pinned Messages and Inbox from the
 * strip, Member List; a channel with a name of 30 letters; a DM's header. At 1280: the boxes. No assertions: it
 * measures and writes lines. Shots and measure.json go to SR_P85_LOUNGE_OUT.
 */
const ON = process.env.SR_P85_LOUNGE === '1';
const BASE = process.env.SR_P85_BASE === '1';
const PRE = BASE ? 'base-' : '';
const OUT =
  process.env.SR_P85_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p85-lounge';
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
const LONG_NAME = 'a-very-long-channel-name-here';

test.skip(!ON, 'local visual check only');
test.setTimeout(600_000);

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

async function hit(loc: Locator, size: Size) {
  const el = loc.first();
  await el.scrollIntoViewIfNeeded({ timeout: 4000 }).catch(() => undefined);
  if (size === 'phone') await el.tap({ timeout: 5000 });
  else await el.click({ timeout: 5000 });
  await el.page().waitForTimeout(450);
}

const shot = (page: Page, name: string) =>
  page.screenshot({ path: resolve(OUT, `${PRE}${name}.png`), timeout: 10_000 });

interface HeaderBtn {
  name: string;
  box: number[];
  hitAbove: boolean;
  hitBelow: boolean;
  centreIsSelf: boolean;
  /** The ::after target's box (x, y, w, h), whether it covers a neighbour's centre, whether it reaches above the strip. */
  target: number[] | null;
  coversNeighbourCentre: boolean;
  reachesAboveStrip: boolean;
}
function headerInPage(): {
  buttons: HeaderBtn[];
  header: number[];
  title: number[] | null;
  titleText: string;
  titleScroll: number[];
  headerScroll: number[];
  extra: number[] | null;
  searchBoxShown: boolean;
  more: string | null;
  doc: number;
} {
  const hdr = document.querySelector('.lng-header') as HTMLElement | null;
  const r2 = (r: DOMRect) => [
    Math.round(r.left),
    Math.round(r.top),
    Math.round(r.width),
    Math.round(r.height),
  ];
  const empty = {
    buttons: [],
    header: [],
    title: null,
    titleText: '',
    titleScroll: [],
    headerScroll: [],
    extra: null,
    searchBoxShown: false,
    more: null,
    doc: 0,
  };
  if (!hdr) return empty;
  const ok = (b: Element, t: Element | null) =>
    !!t && (t === b || b.contains(t) || t.closest('button') === b);
  const hdrBottom = hdr.getBoundingClientRect().bottom;
  const els: HTMLElement[] = [];
  for (const b of Array.from(hdr.querySelectorAll<HTMLElement>('button'))) {
    const r = b.getBoundingClientRect();
    if (r.width === 0 || r.height === 0 || getComputedStyle(b).display === 'none') continue;
    els.push(b);
  }
  const rects = els.map((b) => b.getBoundingClientRect());
  const buttons: HeaderBtn[] = els.map((b, i) => {
    const r = rects[i]!;
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const a = getComputedStyle(b, '::after');
    let target: number[] | null = null;
    if (a.content !== 'none') {
      const top = r.top - Math.abs(parseFloat(a.insetBlockStart) || 0);
      const bottom = r.bottom + Math.abs(parseFloat(a.insetBlockEnd) || 0);
      const left = r.left - Math.abs(parseFloat(a.insetInlineStart) || 0);
      const right = r.right + Math.abs(parseFloat(a.insetInlineEnd) || 0);
      target = [left, top, right - left, bottom - top].map(Math.round);
    }
    const inStrip = !!b.closest('.lng-dm-extra[data-open]');
    const covers =
      !!target &&
      rects.some((o, j) => {
        if (j === i) return false;
        const ox = o.left + o.width / 2;
        const oy = o.top + o.height / 2;
        return (
          ox > target![0]! &&
          ox < target![0]! + target![2]! &&
          oy > target![1]! &&
          oy < target![1]! + target![3]!
        );
      });
    return {
      name: b.getAttribute('aria-label') ?? (b.textContent ?? '').trim().slice(0, 30),
      box: r2(r),
      hitAbove: ok(b, document.elementFromPoint(cx, cy - 21)),
      hitBelow: ok(b, document.elementFromPoint(cx, cy + 21)),
      centreIsSelf: ok(b, document.elementFromPoint(cx, cy)),
      target,
      coversNeighbourCentre: covers,
      reachesAboveStrip: inStrip && !!target && target[1]! < hdrBottom,
    };
  });
  const title = hdr.querySelector<HTMLElement>('.lng-header-title');
  const extra = hdr.querySelector<HTMLElement>('.lng-dm-extra[data-open]');
  const search = hdr.querySelector<HTMLElement>('.lng-search');
  const more = hdr.querySelector<HTMLElement>('.lng-dm-more');
  return {
    buttons,
    header: r2(hdr.getBoundingClientRect()),
    title: title ? r2(title.getBoundingClientRect()) : null,
    titleText: title?.textContent ?? '',
    titleScroll: title ? [title.scrollWidth, title.clientWidth] : [],
    headerScroll: [hdr.scrollWidth, hdr.clientWidth],
    extra: extra ? r2(extra.getBoundingClientRect()) : null,
    searchBoxShown: !!search && getComputedStyle(search).display !== 'none',
    more: more ? `${getComputedStyle(more).display}/${more.getAttribute('aria-expanded')}` : null,
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
  };
}

const popups = (page: Page) =>
  page.evaluate(() => {
    const vw = document.documentElement.clientWidth;
    const vh = document.documentElement.clientHeight;
    return Array.from(
      document.querySelectorAll<HTMLElement>('[role="dialog"], [role="menu"], .lng-inbox'),
    )
      .map((e) => ({ cls: e.className.toString().slice(0, 30), r: e.getBoundingClientRect() }))
      .filter((o) => o.r.width > 0 && o.r.height > 0)
      .map((o) => ({
        cls: o.cls,
        box: [o.r.left, o.r.top, o.r.width, o.r.height].map(Math.round),
        inside: o.r.left >= 0 && o.r.top >= 0 && o.r.right <= vw && o.r.bottom <= vh,
      }));
  });

async function gotoHeader(page: Page, path: string) {
  await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
  await idle(page, 4000);
  await page.locator('.lng-header').waitFor({ timeout: 8000 });
  await page.waitForTimeout(800);
}

async function run(browser: Browser, ownerCtx: BrowserContext) {
  const w = world();
  const long = await apiPost<{ id: string }>(ownerCtx.request, `/guilds/${w.guildId}/channels`, {
    name: LONG_NAME,
    type: 0,
  }).catch(async () => {
    const l = (await (
      await ownerCtx.request.get(`/api/v1/guilds/${w.guildId}/channels`)
    ).json()) as { id: string; name: string }[];
    return l.find((c) => c.name === LONG_NAME) ?? { id: w.channelId };
  });
  for (const theme of THEMES) {
    await setTheme(ownerCtx, theme);
    for (const size of ['phone', 'desktop'] as Size[]) {
      const id = `${size}-${theme}`;
      const { ctx, page } = await newPage(browser, size, ownerCtx, theme);
      try {
        await gotoHeader(page, `/channels/${w.guildId}/${w.channelId}`);
        log(`${id}-closed`, {
          ...(await page.evaluate(headerInPage)),
          themeAttr: await page.evaluate(() =>
            document.querySelector('[data-theme]')?.getAttribute('data-theme'),
          ),
        });
        await shot(page, `header-${id}-closed`);
        if (size === 'phone' && !BASE) {
          const more = page.getByRole('button', { name: 'More actions', exact: true });
          await hit(more, size);
          log(`${id}-open`, await page.evaluate(headerInPage));
          await shot(page, `header-${id}-open`);
          await hit(more, size);
          log(`${id}-second-tap`, { expanded: await more.getAttribute('aria-expanded') });
          await hit(more, size);
          await page.keyboard.press('Escape');
          await page.waitForTimeout(300);
          log(`${id}-escape`, { expanded: await more.getAttribute('aria-expanded') });
          for (const name of ['Pinned Messages', 'Inbox']) {
            await hit(more, size);
            const b = page.getByRole('button', { name, exact: true });
            await hit(b, size);
            log(`${id}-tap-${name}`, {
              expanded: await b.first().getAttribute('aria-expanded'),
              popups: await popups(page),
            });
            if (theme === 'lounge-dark') await shot(page, `header-${id}-tap-${name.split(' ')[0]}`);
            await page.keyboard.press('Escape');
            await page.waitForTimeout(400);
            // Close the strip again if the popover left it open.
            if ((await more.getAttribute('aria-expanded')) === 'true') await hit(more, size);
          }
          await hit(more, size);
          await hit(page.getByRole('button', { name: 'Member List', exact: true }), size);
          log(`${id}-tap-Member List`, {
            pane: await page.evaluate(() =>
              document.querySelector('.lng-root')?.getAttribute('data-pane'),
            ),
            members: await page
              .locator('.lng-members')
              .first()
              .isVisible()
              .catch(() => false),
          });
          await shot(page, `header-${id}-memberlist`);
        } else if (size === 'phone') {
          for (const name of ['Pinned Messages', 'Inbox']) {
            const b = page.getByRole('button', { name, exact: true });
            await hit(b, size).catch(() => undefined);
            log(`${id}-tap-${name}`, { popups: await popups(page) });
            await page.keyboard.press('Escape');
            await page.waitForTimeout(300);
          }
        }
        if (size === 'phone') {
          await gotoHeader(page, `/channels/${w.guildId}/${long.id}`);
          log(`${id}-long`, await page.evaluate(headerInPage));
          if (theme === 'lounge-dark') await shot(page, `header-${id}-long`);
        }
        if (w.dmId) {
          await gotoHeader(page, `/channels/@me/${w.dmId}`);
          log(`${id}-dm`, await page.evaluate(headerInPage));
          if (theme === 'lounge-dark') await shot(page, `header-${id}-dm`);
          if (size === 'phone') {
            await hit(page.getByRole('button', { name: 'More actions', exact: true }), size).catch(
              () => undefined,
            );
            log(`${id}-dm-more-open`, await page.evaluate(headerInPage));
            if (theme === 'lounge-dark') await shot(page, `header-${id}-dm-open`);
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

test('SEC-AUD-368: a channel header on a phone keeps the name; the rest opens from More', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  await run(browser, ownerCtx);
  await ownerCtx.close();
});
