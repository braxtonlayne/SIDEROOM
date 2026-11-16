import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-362 and SEC-AUD-363 (wave 84; D-867, D-868). The Owner on the visual sweep's seeded world
 * (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`'s "seed the sweep world"; that spec is never edited).
 * Three settings: 375x812 (touch, a mobile user agent) in Lounge Light and in Lounge Dark, and 1280x800 in Editorial
 * (the account's main theme by PATCH and localStorage, `data-theme` read back from the page before each shot and
 * written into the file name).
 *
 * Part `bar` (item 1): the four pages that show the unsaved bar (a text channel's Modules tab; the member's Settings >
 * Modules; a server's Features; Admin > Feature modules, in its drawer), clean and dirty (one field changed, not
 * saved): each button's box, its label's line count, whether it is inside its card, where it starts against the
 * card's text column, the scroller's sideways scroll, and whether the dirty bar covers the field that was changed
 * (scrolled to the middle of the viewport).
 * Part `click` (item 1): on a channel's Modules tab: change a field, Discard (the field is back), change it again,
 * Save (saved, the bar is clean); three shots.
 * Part `walk` (item 2): every page the Admin navigation lists, read from the page. Per page and setting: the viewport
 * shot (one more scrolled to the middle and one to the end where the scroller is taller than two viewports) and one
 * JSON line in `measure.json`: sideways scroll of the document and the scroller; at 375 the controls that fail the
 * hit test of D-856 (the kit's and the others, apart); text wider than its box with no ellipsis; the boxes of cards,
 * tables and their direct children.
 *
 * Local only (SR_P84_ADMIN=1); out SR_P84_ADMIN_OUT; SR_P84_PART = bar | click | walk (default all);
 * SR_P84_ONLY a regex over page ids; SR_P84_TAG a prefix for the file names (the JSON lines carry it);
 * SR_P84_SETTINGS = a comma list of `phone-light`, `phone-dark`, `desk` (default all three).
 */
const ON = process.env.SR_P84_ADMIN === '1';
const OUT =
  process.env.SR_P84_ADMIN_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p84-admin';
const ONLY = process.env.SR_P84_ONLY ? new RegExp(process.env.SR_P84_ONLY) : null;
const TAG = process.env.SR_P84_TAG ?? '';
const PART = process.env.SR_P84_PART ?? 'bar,click,walk';
const WANT = (process.env.SR_P84_SETTINGS ?? 'phone-light,phone-dark,desk').split(',');
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
const SETTINGS: { key: string; size: Size; theme: string }[] = [
  { key: 'phone-light', size: 'phone' as Size, theme: 'lounge-light' },
  { key: 'phone-dark', size: 'phone' as Size, theme: 'lounge-dark' },
  { key: 'desk', size: 'desktop' as Size, theme: 'lounge-editorial' },
].filter((s) => WANT.includes(s.key));
const LIMIT_MS = 30_000;
const NAV = '[role="dialog"] nav ul li > button';

test.skip(!ON, 'local visual check only');
test.setTimeout(1_800_000);

interface World {
  guildId: string;
  channelId: string;
  voiceChannelId: string | null;
}
const loadWorld = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

interface Box {
  sel: string;
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Ctl {
  tag: string;
  cls: string;
  text: string;
  box: number[];
  kit: boolean;
}
interface Measured {
  vw: number;
  vh: number;
  theme: string | null;
  doc: number;
  scroller: string | null;
  scrollerOver: number;
  scrollerHeight: number;
  scrollerClient: number;
  failKit: number;
  failOther: number;
  fails: Ctl[];
  wide: { tag: string; cls: string; text: string; sw: number; cw: number; ox: string }[];
  boxes: Box[];
}

/** In the page: everything the look pass logs for one page at one setting (the scroller is left at the top). */
function measureInPage(phone: boolean): Measured {
  const de = document.documentElement;
  const vw = de.clientWidth;
  const vh = de.clientHeight;
  const sel = (e: Element) => {
    const cls = typeof e.className === 'string' ? e.className.trim().split(/\s+/).slice(0, 3) : [];
    return `${e.tagName.toLowerCase()}${cls.length ? `.${cls.join('.')}` : ''}`;
  };
  const visible = (e: Element) => {
    const r = e.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return false;
    const cs = getComputedStyle(e);
    return cs.display !== 'none' && cs.visibility !== 'hidden';
  };
  const vp = vw * vh;
  const cands = Array.from(document.querySelectorAll<HTMLElement>('body *')).filter((e) => {
    if (e.clientWidth === 0 || e.closest('nav')) return false;
    const cs = getComputedStyle(e);
    if (cs.display === 'none') return false;
    return /(auto|scroll|overlay)/.test(cs.overflowY) && e.clientWidth * e.clientHeight >= vp * 0.2;
  });
  cands.sort((a, b) => b.scrollHeight - a.scrollHeight);
  const sc = cands[0] ?? null;
  if (sc) sc.scrollTop = 0;
  const scEl: HTMLElement = sc ?? (document.scrollingElement as HTMLElement);
  const themed =
    document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]');
  const root: ParentNode = sc ?? document.body;

  // Controls failing the hit test of D-856 (a phone only): each is scrolled to the middle of its scroller first.
  const fails: Ctl[] = [];
  let failKit = 0;
  let failOther = 0;
  if (phone) {
    const ctl = Array.from(
      document.querySelectorAll<HTMLElement>(
        'button, a[href], input, select, textarea, [role=switch], [role=tab], [role=checkbox], [role=radio], [role=menuitem]',
      ),
    );
    for (const e of ctl) {
      if (!visible(e)) continue;
      if (e instanceof HTMLInputElement && e.type === 'hidden') continue;
      let r = e.getBoundingClientRect();
      if (r.right <= 0 || r.left >= vw) continue; // an off-screen drawer
      if (e.closest('.ss-sr, .ss-bridge-sr')) continue;
      if (e.tagName === 'A') {
        const p = e.closest('p');
        if (p && (p.textContent ?? '').trim() !== (e.textContent ?? '').trim()) continue;
      }
      if (r.height >= 43.5) continue;
      e.scrollIntoView({ block: 'center', inline: 'nearest' });
      r = e.getBoundingClientRect();
      const cx = r.left + r.width / 2;
      const cy = r.top + r.height / 2;
      const labels = (e as HTMLInputElement).labels
        ? Array.from((e as HTMLInputElement).labels ?? [])
        : [];
      const own = e.closest('label');
      const ok = (y: number) => {
        const h = document.elementFromPoint(cx, y);
        if (!h) return false;
        return (
          h === e ||
          e.contains(h) ||
          labels.some((l) => l === h || l.contains(h)) ||
          (own !== null && own.contains(h))
        );
      };
      if (ok(cy - 21) && ok(cy + 21)) continue;
      const cls = typeof e.className === 'string' ? e.className : '';
      const kit = cls.split(/\s+/).some((c) => /^_/.test(c) || /module/.test(c));
      if (kit) failKit++;
      else failOther++;
      const lab =
        e.getAttribute('aria-label') ||
        (e.textContent ?? '').trim() ||
        (e as HTMLInputElement).placeholder ||
        (e as HTMLInputElement).name ||
        (e.id ? (document.querySelector(`label[for="${e.id}"]`)?.textContent ?? '') : '') ||
        '';
      if (fails.length < 80)
        fails.push({
          tag: e.tagName.toLowerCase(),
          cls: cls.slice(0, 60),
          text: lab.trim().slice(0, 40),
          box: [
            Math.round(r.left),
            Math.round(r.top + scEl.scrollTop),
            Math.round(r.width),
            Math.round(r.height * 10) / 10,
          ],
          kit,
        });
    }
    if (sc) sc.scrollTop = 0;
    scEl.scrollTop = 0;
  }

  // Own text wider than its box, with no ellipsis.
  const wide: Measured['wide'] = [];
  for (const e of Array.from(document.querySelectorAll<HTMLElement>('body *'))) {
    if (e.clientWidth < 2 || e.scrollWidth <= e.clientWidth + 1) continue;
    if (e.closest('.ss-sr, .ss-bridge-sr')) continue;
    if (!visible(e)) continue;
    const own = Array.from(e.childNodes).some(
      (n) => n.nodeType === 3 && (n.textContent ?? '').trim() !== '',
    );
    const isField = e instanceof HTMLInputElement || e instanceof HTMLTextAreaElement;
    if (!own && !isField) continue;
    const cs = getComputedStyle(e);
    if (cs.textOverflow === 'ellipsis') continue;
    if (wide.length < 40)
      wide.push({
        tag: e.tagName.toLowerCase(),
        cls: typeof e.className === 'string' ? e.className.slice(0, 60) : '',
        text: (e.textContent ?? '').trim().slice(0, 40),
        sw: e.scrollWidth,
        cw: e.clientWidth,
        ox: cs.overflowX,
      });
  }

  // Boxes of the page's cards, tables and their direct children (coordinates inside the scroller).
  const top0 = sc ? sc.getBoundingClientRect().top : 0;
  const left0 = sc ? sc.getBoundingClientRect().left : 0;
  const boxes: Box[] = [];
  const cards = Array.from(
    root.querySelectorAll<HTMLElement>(
      '[class*="card"], table, .ss-danger-zone, .ss-form, .ss-table, .ss-toolbar, .ss-group, .adm-form, .adm-unsaved, [role=tabpanel]',
    ),
  );
  const put = (e: Element, i: number) => {
    if (boxes.length >= 260) return;
    const r = e.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) return;
    boxes.push({
      sel: `${i}:${sel(e)}`,
      x: Math.round((r.left - left0) * 10) / 10,
      y: Math.round((r.top - top0 + (sc ? sc.scrollTop : 0)) * 10) / 10,
      w: Math.round(r.width * 10) / 10,
      h: Math.round(r.height * 10) / 10,
    });
  };
  cards.forEach((c, i) => {
    put(c, i);
    Array.from(c.children).forEach((k, j) => put(k, i * 100 + j + 1));
  });

  return {
    vw,
    vh,
    theme: themed?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - vw,
    scroller: sc ? sel(sc) : null,
    scrollerOver: sc ? sc.scrollWidth - sc.clientWidth : 0,
    scrollerHeight: scEl.scrollHeight,
    scrollerClient: scEl.clientHeight,
    failKit,
    failOther,
    fails,
    wide,
    boxes,
  };
}

/** Scroll the page's scroller to a fraction of its way down (0 to 1); the same pick as `measureInPage`. */
function scrollScroller(frac: number) {
  const de = document.documentElement;
  const vp = de.clientWidth * de.clientHeight;
  const cands = Array.from(document.querySelectorAll<HTMLElement>('body *')).filter((e) => {
    if (e.clientWidth === 0 || e.closest('nav')) return false;
    const cs = getComputedStyle(e);
    if (cs.display === 'none') return false;
    return /(auto|scroll|overlay)/.test(cs.overflowY) && e.clientWidth * e.clientHeight >= vp * 0.2;
  });
  cands.sort((a, b) => b.scrollHeight - a.scrollHeight);
  const sc = cands[0] ?? (document.scrollingElement as HTMLElement);
  sc.scrollTop = (sc.scrollHeight - sc.clientHeight) * frac;
}

/** In the page: the first unsaved bar, its buttons, its card, the scroller. `field` is the changed field's box. */
function measureBar() {
  const bar = document.querySelector<HTMLElement>('.adm-unsaved');
  if (!bar) return null;
  const br = bar.getBoundingClientRect();
  const card =
    bar.closest<HTMLElement>('.us-card, .ss-card, [class*="card"]') ??
    bar.closest<HTMLElement>('[role=dialog]') ??
    document.body;
  const cr = card.getBoundingClientRect();
  const cs = getComputedStyle(card);
  const textX =
    cr.left +
    parseFloat(cs.borderInlineStartWidth || '0') +
    parseFloat(cs.paddingInlineStart || '0');
  const rnd = (n: number) => Math.round(n * 10) / 10;
  const buttons = Array.from(bar.querySelectorAll<HTMLElement>('.adm-actions > button')).map(
    (b) => {
      const r = b.getBoundingClientRect();
      const range = document.createRange();
      range.selectNodeContents(b);
      const tops: number[] = [];
      for (const q of Array.from(range.getClientRects()).sort((a, b) => a.top - b.top))
        if (tops.length === 0 || q.top - (tops[tops.length - 1] ?? 0) > 6) tops.push(q.top);
      return {
        text: (b.textContent ?? '').trim().slice(0, 50),
        x: rnd(r.left),
        y: rnd(r.top),
        w: rnd(r.width),
        h: rnd(r.height),
        lines: tops.length,
        insideCard: r.left >= cr.left - 0.5 && r.right <= cr.right + 0.5,
        startVsText: rnd(r.left - textX),
        disabled: (b as HTMLButtonElement).disabled,
      };
    },
  );
  const rows = new Set(buttons.map((b) => Math.round(b.y)));
  const de = document.documentElement;
  const vp = de.clientWidth * de.clientHeight;
  const cands = Array.from(document.querySelectorAll<HTMLElement>('body *')).filter((e) => {
    if (e.clientWidth === 0 || e.closest('nav')) return false;
    const c = getComputedStyle(e);
    return (
      c.display !== 'none' &&
      /(auto|scroll|overlay)/.test(c.overflowY) &&
      e.clientWidth * e.clientHeight >= vp * 0.2
    );
  });
  cands.sort((a, b) => b.scrollHeight - a.scrollHeight);
  const sc = cands[0] ?? null;
  const themed =
    document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]');
  return {
    theme: themed?.getAttribute('data-theme') ?? null,
    dirty: bar.getAttribute('data-dirty'),
    bar: { x: rnd(br.left), y: rnd(br.top), w: rnd(br.width), h: rnd(br.height) },
    card: {
      x: rnd(cr.left),
      w: rnd(cr.width),
      textX: rnd(textX),
      cls: card.className.toString().slice(0, 40),
    },
    buttons,
    rows: rows.size,
    scroller: sc ? `${sc.tagName.toLowerCase()}.${String(sc.className).split(/\s+/)[0]}` : null,
    scrollerOver: sc ? sc.scrollWidth - sc.clientWidth : 0,
    doc: de.scrollWidth - de.clientWidth,
  };
}

const slug = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 36);

const idle = async (page: Page, ms = 3000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.waitForTimeout(500);
};

async function withLimit<T>(what: string, fn: () => Promise<T>): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([
      fn(),
      new Promise<never>((_, rej) => {
        timer = setTimeout(() => rej(new Error(`${what}: over ${LIMIT_MS} ms`)), LIMIT_MS);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

async function newPage(browser: Browser, size: Size, theme: string, base: BrowserContext) {
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
  page.setDefaultTimeout(8000);
  return { ctx, page };
}

test('SEC-AUD-362/363: the unsaved bar, and every page of Admin looked at', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const barLog = resolve(OUT, 'bar.json');
  const world = loadWorld();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const pre = TAG ? `${TAG}-` : '';
  const totals = {
    pages: 0,
    shots: 0,
    lines: 0,
    failed: [] as string[],
    lists: {} as Record<string, string[]>,
  };

  const setTheme = async (theme: string) => {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    console.log(
      `REPORT account theme set ${theme}, read back: ${back.settings?.user_prefs?.theme}`,
    );
  };
  const themeOf = (page: Page) =>
    page.evaluate(
      () =>
        (
          document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
        )?.getAttribute('data-theme') ?? 'no-theme',
    );

  // ---- part bar / click -------------------------------------------------------------------------------------------
  /** Open each of the four pages that show the bar; `open` leaves the page ready (the bar on screen). */
  const BAR_PAGES: { id: string; open: (p: Page) => Promise<void> }[] = [
    {
      id: 'chan-modules',
      open: async (p) => {
        await p.goto(`/settings/server/${world.guildId}/channels?channel=${world.channelId}`, {
          waitUntil: 'domcontentloaded',
          timeout: 15_000,
        });
        await idle(p, 4000);
        await p
          .locator('.ss-tabs [role="tab"]', { hasText: /^Modules$/ })
          .first()
          .evaluate((el) => (el as HTMLElement).click());
        await idle(p, 2500);
      },
    },
    {
      id: 'member-modules',
      open: async (p) => {
        await p.goto('/settings/user/modules', { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await idle(p, 4000);
      },
    },
    {
      id: 'server-features',
      open: async (p) => {
        await p.goto(`/settings/server/${world.guildId}/features`, {
          waitUntil: 'domcontentloaded',
          timeout: 15_000,
        });
        await idle(p, 4000);
        await p
          .locator('article button[aria-expanded="false"]')
          .first()
          .evaluate((el) => (el as HTMLElement).click());
        await idle(p, 1500);
      },
    },
    {
      id: 'admin-features',
      open: async (p) => {
        await p.goto('/admin/features', { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await idle(p, 4000);
        // The bar is in the settings drawer of the first module that has settings.
        const n = await p.locator('li button', { hasText: /^Settings$/ }).count();
        console.log(`REPORT admin-features: ${n} modules with a Settings button`);
        await p
          .locator('li button', { hasText: /^Settings$/ })
          .first()
          .evaluate((el) => (el as HTMLElement).click());
        await idle(p, 2000);
      },
    },
  ];

  /** Change the first field above the first bar; returns whether a field was found. */
  const makeDirty = async (p: Page) => {
    const bar = p.locator('.adm-unsaved').first();
    const box = bar.locator(
      'xpath=ancestor::*[self::article or contains(@class,"us-card") or contains(@class,"adm-form")][1]',
    );
    const scope = (await box.count()) > 0 ? box.first() : p.locator('body');
    const sw = scope.locator('input[type=checkbox]:not([disabled]), [role=switch]:not([disabled])');
    const txt = scope.locator(
      'input:not([type=hidden]):not([type=checkbox]):not([type=radio]):not([disabled]):not([role=switch])',
    );
    let el = null as ReturnType<Page['locator']> | null;
    if ((await txt.count()) > 0) {
      el = txt.first();
      const type = (await el.getAttribute('type')) ?? 'text';
      const cur = await el.inputValue();
      await el.scrollIntoViewIfNeeded();
      await el.fill(type === 'number' ? String((Number(cur) || 0) + 1) : `${cur}x`);
    } else if ((await sw.count()) > 0) {
      el = sw.first();
      await el.scrollIntoViewIfNeeded();
      await el.evaluate((e) => (e as HTMLElement).click());
    }
    await p.waitForTimeout(250);
    return el;
  };

  const barShot = async (p: Page, name: string) => {
    const f = `${pre}${name}.png`;
    await p.screenshot({ path: resolve(OUT, f), timeout: 10_000 });
    totals.shots++;
    return f;
  };

  const doBar = async (page: Page, size: Size, key: string) => {
    for (const bp of BAR_PAGES) {
      if (ONLY && !ONLY.test(bp.id)) continue;
      try {
        await withLimit(bp.id, async () => {
          await bp.open(page);
          if ((await page.locator('.adm-unsaved').count()) === 0) {
            console.log(`REPORT ${key} ${bp.id}: NO BAR on the page`);
            appendFileSync(barLog, `${JSON.stringify({ id: bp.id, key, tag: TAG, bar: false })}\n`);
            return;
          }
          const bar = page.locator('.adm-unsaved').first();
          await bar.evaluate((e) => e.scrollIntoView({ block: 'center' }));
          await page.waitForTimeout(250);
          const clean = await page.evaluate(measureBar);
          const boxes = (await page.evaluate(measureInPage, size === 'phone')).boxes;
          const th = await themeOf(page);
          const f1 = await barShot(page, `bar-${bp.id}-${key}-${th}-clean`);
          const field = await makeDirty(page);
          // The edited field scrolled to the middle of the viewport: does the sticky bar cover it?
          let covers: boolean | null = null;
          let fieldBox: number[] | null = null;
          if (field) {
            await field.evaluate((e) => e.scrollIntoView({ block: 'center' }));
            await page.waitForTimeout(250);
            const fb = await field.boundingBox();
            const bb = await bar.boundingBox();
            if (fb && bb) {
              fieldBox = [fb.x, fb.y, fb.width, fb.height].map(Math.round);
              covers = fb.y < bb.y + bb.height && fb.y + fb.height > bb.y;
            }
          }
          const dirtyMid = await page.evaluate(measureBar);
          const f2 = await barShot(page, `bar-${bp.id}-${key}-${th}-dirty-mid`);
          await bar.evaluate((e) => e.scrollIntoView({ block: 'center' }));
          await page.waitForTimeout(250);
          const dirty = await page.evaluate(measureBar);
          const f3 = await barShot(page, `bar-${bp.id}-${key}-${th}-dirty`);
          appendFileSync(
            barLog,
            `${JSON.stringify({ id: bp.id, key, tag: TAG, size, theme: th, boxes, clean, dirty, dirtyMid, fieldBox, covers, shots: [f1, f2, f3], changed: !!field })}\n`,
          );
          console.log(
            `REPORT ${key} ${bp.id}: clean btns ${clean?.buttons.map((b) => `${b.w}x${b.h}/${b.lines}l`).join(' ')} | dirty ${dirty?.buttons.map((b) => `${b.w}x${b.h}/${b.lines}l in=${b.insideCard}`).join(' ')} | scrollerOver ${dirty?.scrollerOver} doc ${dirty?.doc} covers ${covers}`,
          );
        });
      } catch (e) {
        totals.failed.push(`${key} bar ${bp.id}`);
        console.log(`REPORT ${key} ${bp.id}: NOT MEASURED ${String(e).slice(0, 160)}`);
      }
    }
  };

  const doClick = async (page: Page, key: string) => {
    const bp = BAR_PAGES[0]!;
    await bp.open(page);
    const bar = page.locator('.adm-unsaved').first();
    const save = bar.locator('.adm-actions > button').first();
    const discard = bar.locator('.adm-actions > button').nth(1);
    const fieldLoc = bar
      .locator('xpath=ancestor::article[1]')
      .locator(
        'input:not([type=hidden]):not([type=checkbox]):not([type=radio]):not([role=switch]):not([disabled])',
      );
    const f = fieldLoc.first();
    const before = await f.inputValue();
    const num = (await f.getAttribute('type')) === 'number';
    const changed = num ? String((Number(before) || 0) + 1) : `${before}x`;
    await f.scrollIntoViewIfNeeded();
    await f.fill(changed);
    await page.waitForTimeout(250);
    await discard.evaluate((e) => e.scrollIntoView({ block: 'center' }));
    const th = await themeOf(page);
    const a = await barShot(page, `click-1-changed-${key}-${th}`);
    await discard.click();
    await page.waitForTimeout(400);
    const afterDiscard = await f.inputValue();
    const cleanAfterDiscard = (await bar.getAttribute('data-dirty')) === 'false';
    const b = await barShot(page, `click-2-discarded-${key}-${th}`);
    await f.fill(changed);
    await page.waitForTimeout(250);
    await save.click();
    await page.waitForTimeout(1500);
    const cleanAfterSave = (await bar.getAttribute('data-dirty')) === 'false';
    const valueAfterSave = await f.inputValue();
    const c = await barShot(page, `click-3-saved-${key}-${th}`);
    // Put the value back so a later run starts from the same world.
    await f.fill(before);
    await page.waitForTimeout(250);
    if ((await bar.getAttribute('data-dirty')) === 'true') {
      await save.click().catch(() => undefined);
      await page.waitForTimeout(1000);
    }
    const line = {
      click: true,
      key,
      tag: TAG,
      before,
      changed,
      afterDiscard,
      discardRestored: afterDiscard === before,
      cleanAfterDiscard,
      valueAfterSave,
      saved: valueAfterSave === changed,
      cleanAfterSave,
      shots: [a, b, c],
    };
    appendFileSync(barLog, `${JSON.stringify(line)}\n`);
    console.log(`REPORT ${key} click: ${JSON.stringify(line)}`);
  };

  // ---- part walk ----------------------------------------------------------------------------------------------------
  const look = async (page: Page, id: string, size: Size, extra: Record<string, unknown>) => {
    await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
    const m = await page.evaluate(measureInPage, size === 'phone');
    const stem = `${pre}${id}-${size === 'phone' ? '375' : '1280'}-${m.theme ?? 'no-theme'}`;
    const shots: string[] = [];
    const shoot = async (suffix: string) => {
      const f = `${stem}${suffix}.png`;
      await page.screenshot({ path: resolve(OUT, f), timeout: 10_000 });
      shots.push(f);
      totals.shots++;
    };
    await shoot('');
    if (m.scrollerHeight > 2 * m.scrollerClient) {
      await page.evaluate(scrollScroller, 0.5);
      await page.waitForTimeout(250);
      await shoot('-mid');
      await page.evaluate(scrollScroller, 1);
      await page.waitForTimeout(250);
      await shoot('-end');
      await page.evaluate(scrollScroller, 0);
    }
    appendFileSync(
      jsonl,
      `${JSON.stringify({ id, tag: TAG || undefined, size, url: page.url().replace(ORIGIN, ''), ...extra, ...m, shots })}\n`,
    );
    totals.lines++;
    console.log(
      `REPORT ${size} ${m.theme} ${id}: doc ${m.doc} scroller ${m.scrollerOver} height ${m.scrollerHeight}/${m.scrollerClient} failKit ${m.failKit} failOther ${m.failOther} wide ${m.wide.length} shots ${shots.length}`,
    );
  };

  const walkAdmin = async (page: Page, size: Size) => {
    const path = '/admin/dashboard';
    await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
    await idle(page, 4000);
    const labels = await page
      .locator(NAV)
      .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
    console.log(`REPORT ${size} admin: ${labels.length} pages: ${labels.join(' | ')}`);
    totals.lists[`admin-${size}`] = labels;
    for (let i = 0; i < labels.length; i++) {
      const label = labels[i] ?? '';
      const id = `adm-${String(i + 1).padStart(2, '0')}-${slug(label)}`;
      if (ONLY && !ONLY.test(id)) continue;
      try {
        await withLimit(id, async () => {
          if ((await page.locator(NAV).count()) !== labels.length) {
            await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
            await idle(page, 3000);
          }
          await page
            .locator(NAV)
            .nth(i)
            .evaluate((el) => (el as HTMLElement).click());
          await idle(page, 2500);
          await look(page, id, size, { page: label });
        });
        totals.pages++;
      } catch (e) {
        totals.failed.push(`${size} ${id}`);
        appendFileSync(
          jsonl,
          `${JSON.stringify({ id, size, ok: false, error: String(e).slice(0, 200) })}\n`,
        );
        console.log(`REPORT ${size} ${id}: NOT MEASURED ${String(e).slice(0, 120)}`);
        await page
          .goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 })
          .catch(() => undefined);
      }
    }
  };

  for (const s of SETTINGS) {
    await setTheme(s.theme);
    const { ctx, page } = await newPage(browser, s.size, s.theme, ownerCtx);
    if (PART.includes('bar')) await doBar(page, s.size, s.key);
    if (PART.includes('click') && s.size === 'phone' && (!ONLY || ONLY.test('click')))
      await doClick(page, s.key).catch((e) => {
        totals.failed.push(`${s.key} click: ${String(e).slice(0, 100)}`);
        console.log(`REPORT ${s.key} click FAILED ${String(e).slice(0, 200)}`);
      });
    if (PART.includes('walk'))
      await walkAdmin(page, s.size).catch((e) => {
        totals.failed.push(`${s.key} admin: ${String(e).slice(0, 100)}`);
      });
    await ctx.close().catch(() => undefined);
  }

  await ownerCtx.close();
  console.log(`REPORT totals ${JSON.stringify(totals)}`);
  writeFileSync(
    resolve(OUT, `totals${TAG ? `-${TAG}` : ''}.json`),
    JSON.stringify(totals, null, 1),
  );
});
