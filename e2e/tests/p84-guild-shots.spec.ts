import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';

/**
 * Wave 84, guild (SEC-AUD-360, 361; D-864, D-865, D-866). Local only (SR_P84_GUILD=1).
 * The Owner on the visual sweep's seeded world (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`'s
 * "seed the sweep world"; that spec is never edited), as `p83-guild-shots.spec.ts` does.
 *
 *   SR_P84_TAG   file-name prefix and log tag: `base` (before) or `after`.
 *   SR_P84_OUT   the folder of the shots and `measure.json`.
 *   SR_P84_PART  comma list of: people (Members, Invites, 375 Lounge Light/Dark, 1280 Editorial), click (the phone
 *                click-through, after only), small (voice and text channel title, sliders, a select, Compare levels),
 *                admin (the three Admin pages). Default: all.
 *
 * Every control in a Members or Invites row is logged with its box, whether it is inside the viewport and the plan's hit
 * test (D-856): at 375 with touch, `elementFromPoint(cx, cy - 21)` and `(cx, cy + 21)` return the control, a node in it
 * or its own label.
 */
const ON = process.env.SR_P84_GUILD === '1';
const OUT =
  process.env.SR_P84_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p84-guild';
const TAG = process.env.SR_P84_TAG ?? 'after';
const PARTS = new Set((process.env.SR_P84_PART ?? 'people,click,small,admin').split(','));
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
const SETTINGS: { size: Size; theme: string }[] = [
  { size: 'phone', theme: 'lounge-light' },
  { size: 'phone', theme: 'lounge-dark' },
  { size: 'desktop', theme: 'lounge-editorial' },
];
const NAV = '[role="dialog"] nav ul li > button';

test.skip(!ON, 'local visual check only');
test.setTimeout(1_800_000);

interface World {
  guildId: string;
  channelId: string;
  voiceChannelId: string | null;
}
const loadWorld = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

interface Ctl {
  kind: string;
  cls: string;
  label: string;
  box: number[];
  inView: boolean;
  hitTop: boolean;
  hitBottom: boolean;
}
interface Measured {
  vw: number;
  theme: string | null;
  doc: number;
  scroller: string | null;
  scrollerOver: number;
  scrollerHeight: number;
  scrollerClient: number;
  tables: { sel: string; over: number; w: number; h: number }[];
  ctls: Ctl[];
  ctlsFailing: number;
  chips: { label: string; box: number[] }[];
  selects: { cls: string; h: number; top: number }[];
  sliders: { accent: string; h: number }[];
  titles: string[];
  firstCol: { text: string; h: number; lines: number; w: number }[];
  wide: { tag: string; cls: string; text: string; sw: number; cw: number }[];
  boxes: { sel: string; x: number; y: number; w: number; h: number }[];
}

/** In the page: everything logged for one view at one setting. */
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
    if (e.clientWidth === 0) return false;
    const cs = getComputedStyle(e);
    if (cs.display === 'none') return false;
    return /(auto|scroll|overlay)/.test(cs.overflowY) && e.clientWidth * e.clientHeight >= vp * 0.2;
  });
  cands.sort((a, b) => b.scrollHeight - a.scrollHeight);
  const sc = cands[0] ?? null;
  if (sc) sc.scrollTop = 0;
  const scEl: HTMLElement = sc ?? (document.scrollingElement as HTMLElement);
  const themed = document.querySelector('[data-theme]');
  const root: ParentNode = document.body;

  const tables = Array.from(root.querySelectorAll<HTMLElement>('table.ss-table')).map((t) => {
    const r = t.getBoundingClientRect();
    return {
      sel: sel(t),
      over: t.scrollWidth - t.clientWidth,
      w: Math.round(r.width),
      h: Math.round(r.height),
    };
  });

  // Controls of a Members or an Invites row, with the hit test.
  const ctls: Ctl[] = [];
  const chips: Measured['chips'] = [];
  const hitOk = (el: HTMLElement, x: number, y: number) => {
    const h = document.elementFromPoint(x, y);
    if (!h) return false;
    if (h === el || el.contains(h)) return true;
    const lab =
      el.closest('label') ?? (el.id ? document.querySelector(`label[for="${el.id}"]`) : null);
    return !!lab && (h === lab || lab.contains(h));
  };
  const rows = Array.from(root.querySelectorAll<HTMLElement>('table.ss-table tbody tr')).slice(
    0,
    4,
  );
  for (const row of rows) {
    for (const e of Array.from(
      row.querySelectorAll<HTMLElement>('button, input, a[href], select'),
    )) {
      if (!visible(e)) continue;
      e.scrollIntoView({ block: 'center', inline: 'nearest' });
      const r = e.getBoundingClientRect();
      const cx = r.left + r.width / 2;
      const cy = r.top + r.height / 2;
      const label =
        e.getAttribute('aria-label') ||
        (e.textContent ?? '').trim() ||
        (e as HTMLInputElement).type;
      ctls.push({
        kind: e.tagName.toLowerCase() + (e instanceof HTMLInputElement ? `:${e.type}` : ''),
        cls: typeof e.className === 'string' ? e.className.slice(0, 40) : '',
        label: label.slice(0, 50),
        box: [
          Math.round(r.left),
          Math.round(r.top),
          Math.round(r.width),
          Math.round(r.height * 10) / 10,
        ],
        inView: r.left >= 0 && r.right <= vw && r.top >= 0 && r.bottom <= vh,
        hitTop: hitOk(e, cx, cy - 21),
        hitBottom: hitOk(e, cx, cy + 21),
      });
    }
    for (const c of Array.from(row.querySelectorAll<HTMLElement>('.ss-role-chip'))) {
      const r = c.getBoundingClientRect();
      chips.push({
        label: (c.textContent ?? '').trim().slice(0, 30),
        box: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)],
      });
    }
  }
  if (sc) sc.scrollTop = 0;
  const ctlsFailing = phone ? ctls.filter((c) => !c.inView || !c.hitTop || !c.hitBottom).length : 0;

  const selects = Array.from(root.querySelectorAll<HTMLElement>('select'))
    .filter(visible)
    .map((e) => {
      const r = e.getBoundingClientRect();
      return {
        cls: e.className.slice(0, 40),
        h: Math.round(r.height * 10) / 10,
        top: Math.round(r.top),
      };
    });
  const sliders = Array.from(root.querySelectorAll<HTMLInputElement>('input[type="range"]'))
    .filter(visible)
    .map((e) => ({
      accent: getComputedStyle(e).accentColor,
      h: Math.round(e.getBoundingClientRect().height * 10) / 10,
    }));
  const titles = Array.from(root.querySelectorAll<HTMLElement>('h2.ss-title')).map(
    (e) => e.textContent ?? '',
  );
  const firstCol = Array.from(
    root.querySelectorAll<HTMLElement>('table.sup-compare tbody tr > :first-child'),
  ).map((e) => {
    const r = e.getBoundingClientRect();
    const lh = parseFloat(getComputedStyle(e).lineHeight) || 18;
    return {
      text: (e.textContent ?? '').trim().slice(0, 40),
      h: Math.round(r.height),
      lines: Math.round((r.height - 16) / lh),
      w: Math.round(r.width),
    };
  });

  const wide: Measured['wide'] = [];
  for (const e of Array.from(document.querySelectorAll<HTMLElement>('body *'))) {
    if (e.clientWidth < 2 || e.scrollWidth <= e.clientWidth + 1) continue;
    if (e.closest('.ss-sr, .ss-bridge-sr')) continue;
    if (!visible(e)) continue;
    const own = Array.from(e.childNodes).some(
      (n) => n.nodeType === 3 && (n.textContent ?? '').trim() !== '',
    );
    if (!own) continue;
    if (getComputedStyle(e).textOverflow === 'ellipsis') continue;
    if (wide.length < 30)
      wide.push({
        tag: e.tagName.toLowerCase(),
        cls: typeof e.className === 'string' ? e.className.slice(0, 40) : '',
        text: (e.textContent ?? '').trim().slice(0, 40),
        sw: e.scrollWidth,
        cw: e.clientWidth,
      });
  }

  const top0 = sc ? sc.getBoundingClientRect().top : 0;
  const left0 = sc ? sc.getBoundingClientRect().left : 0;
  const boxes: Measured['boxes'] = [];
  const cards = Array.from(
    root.querySelectorAll<HTMLElement>(
      '[class*="card"], .ss-danger-zone, .ss-form, .ss-table, .ss-toolbar, .ss-group, .ss-split, .ss-title, .ss-select, [role=tabpanel]',
    ),
  );
  const put = (e: Element, i: number) => {
    if (boxes.length >= 220) return;
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
    theme: themed?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - vw,
    scroller: sc ? sel(sc) : null,
    scrollerOver: sc ? sc.scrollWidth - sc.clientWidth : 0,
    scrollerHeight: scEl.scrollHeight,
    scrollerClient: scEl.clientHeight,
    tables,
    ctls,
    ctlsFailing,
    chips,
    selects,
    sliders,
    titles,
    firstCol,
    wide,
    boxes,
  };
}

function scrollScroller(frac: number) {
  const de = document.documentElement;
  const vp = de.clientWidth * de.clientHeight;
  const cands = Array.from(document.querySelectorAll<HTMLElement>('body *')).filter((e) => {
    if (e.clientWidth === 0) return false;
    const cs = getComputedStyle(e);
    if (cs.display === 'none') return false;
    return /(auto|scroll|overlay)/.test(cs.overflowY) && e.clientWidth * e.clientHeight >= vp * 0.2;
  });
  cands.sort((a, b) => b.scrollHeight - a.scrollHeight);
  const sc = cands[0] ?? (document.scrollingElement as HTMLElement);
  sc.scrollTop = (sc.scrollHeight - sc.clientHeight) * frac;
}

const idle = async (page: Page, ms = 3000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.waitForTimeout(500);
};

async function newPage(browser: Browser, size: Size, theme: string, base: BrowserContext) {
  const phone = size === 'phone';
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: SIZES[size],
    storageState: await base.storageState(),
    bypassCSP: true,
    permissions: ['clipboard-read', 'clipboard-write'],
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

test('SEC-AUD-360 and 361: Members and Invites as stacked rows, five small items', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const world = loadWorld();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const totals = { shots: 0, lines: 0, failed: [] as string[] };

  const setTheme = async (theme: string) => {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    console.log(
      `REPORT account theme set ${theme}, read back: ${back.settings?.user_prefs?.theme}`,
    );
  };

  // At least two invites (D-863: the world has one).
  {
    const list = await o.get(`/api/v1/guilds/${world.guildId}/invites`);
    const have = list.ok() ? ((await list.json()) as unknown[]).length : 0;
    if (have < 2)
      await apiPost(o, `/channels/${world.channelId}/invites`, { max_uses: 5 }).catch((e) =>
        console.log(`REPORT second invite not made: ${String(e).slice(0, 120)}`),
      );
    console.log(`REPORT invites before: ${have}`);
    const ms = await o.get(`/api/v1/guilds/${world.guildId}/members?limit=50`);
    console.log(`REPORT members: ${ms.ok() ? ((await ms.json()) as unknown[]).length : 'n/a'}`);
  }

  const look = async (page: Page, id: string, size: Size, extra: Record<string, unknown> = {}) => {
    await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
    const m = await page.evaluate(measureInPage, size === 'phone');
    const stem = `${TAG}-${id}-${size === 'phone' ? '375' : '1280'}-${m.theme ?? 'no-theme'}`;
    const shots: string[] = [];
    const shoot = async (suffix: string) => {
      const f = `${stem}${suffix}.png`;
      await page.screenshot({ path: resolve(OUT, f), timeout: 10_000 });
      shots.push(f);
      totals.shots++;
    };
    await shoot('');
    if (m.scrollerHeight > 1.5 * m.scrollerClient) {
      await page.evaluate(scrollScroller, 0.5);
      await page.waitForTimeout(250);
      await shoot('-mid');
      await page.evaluate(scrollScroller, 1);
      await page.waitForTimeout(250);
      await shoot('-end');
      await page.evaluate(scrollScroller, 0);
    }
    appendFileSync(jsonl, `${JSON.stringify({ id, tag: TAG, size, ...extra, ...m, shots })}\n`);
    totals.lines++;
    console.log(
      `REPORT ${TAG} ${size} ${m.theme} ${id}: doc ${m.doc} scroller ${m.scrollerOver} tables ${JSON.stringify(m.tables.map((t) => t.over))} ctls ${m.ctls.length} failing ${m.ctlsFailing} selects ${JSON.stringify(m.selects.map((s) => s.h))} sliders ${JSON.stringify(m.sliders)} titles ${JSON.stringify(m.titles)} firstCol ${JSON.stringify(m.firstCol.map((f) => f.lines))} wide ${m.wide.length} shots ${shots.length}`,
    );
    return m;
  };

  /** Open a page of a settings shell (a server's settings or Admin) by its navigation label. */
  const openNav = async (page: Page, path: string, label: RegExp) => {
    await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
    await idle(page, 4000);
    const labels = await page
      .locator(NAV)
      .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
    const i = labels.findIndex((l) => label.test(l));
    if (i < 0) throw new Error(`no nav item ${label}: ${labels.join(' | ')}`);
    await page
      .locator(NAV)
      .nth(i)
      .evaluate((el) => (el as HTMLElement).click());
    await idle(page, 2500);
    return labels[i] ?? '';
  };

  const guard = async (what: string, fn: () => Promise<void>) => {
    try {
      await fn();
    } catch (e) {
      totals.failed.push(`${what}: ${String(e).slice(0, 160)}`);
      console.log(`REPORT FAILED ${what}: ${String(e).slice(0, 200)}`);
    }
  };

  const settingsPath = `/settings/server/${world.guildId}`;
  const channelPath = (id: string) => `/settings/server/${world.guildId}/channels?channel=${id}`;

  for (const { size, theme } of SETTINGS) {
    await setTheme(theme);
    const { ctx, page } = await newPage(browser, size, theme, ownerCtx);
    if (PARTS.has('people')) {
      for (const [id, re] of [
        ['members', /^Members$/i],
        ['invites', /^Invites$/i],
      ] as const)
        await guard(`${size} ${theme} ${id}`, async () => {
          await openNav(page, settingsPath, re);
          await look(page, id, size);
        });
    }
    if (PARTS.has('click') && size === 'phone') {
      await guard(`${size} ${theme} click-through`, async () => {
        await openNav(page, settingsPath, /^Members$/i);
        const shot = async (name: string) => {
          await page.screenshot({ path: resolve(OUT, `${TAG}-click-${name}-375-${theme}.png`) });
          totals.shots++;
        };
        const rect = (loc: ReturnType<Page['locator']>) =>
          loc.first().evaluate((e) => {
            const r = e.getBoundingClientRect();
            const de = document.documentElement;
            return {
              box: [
                Math.round(r.left),
                Math.round(r.top),
                Math.round(r.width),
                Math.round(r.height),
              ],
              inside:
                r.left >= 0 &&
                r.right <= de.clientWidth &&
                r.top >= 0 &&
                r.bottom <= de.clientHeight,
            };
          });
        const row = page.locator('table.ss-table tbody tr').first();
        const actions = row.locator('.ss-actions button');
        await actions.first().scrollIntoViewIfNeeded();
        // Roles
        await actions.first().tap();
        await page.waitForTimeout(700);
        const dlg = page.locator('[role="dialog"]').last();
        console.log(`REPORT click ${theme} roles dialog ${JSON.stringify(await rect(dlg))}`);
        await shot('roles-open');
        const close = dlg.getByRole('button', { name: /^(cancel|close|done)$/i });
        if (await close.count()) await close.first().tap();
        else await page.keyboard.press('Escape');
        await page.waitForTimeout(500);
        console.log(
          `REPORT click ${theme} roles dialog closed: ${await page.locator('[role="dialog"]').count()} dialogs left (the settings shell is one)`,
        );
        // The row menu
        await row.scrollIntoViewIfNeeded();
        await actions.nth(1).tap();
        await page.waitForTimeout(500);
        console.log(
          `REPORT click ${theme} row menu ${JSON.stringify(await rect(page.locator('[role="menu"]')))}`,
        );
        await shot('menu-open');
        await page.keyboard.press('Escape');
        await page.waitForTimeout(300);
        // The row checkbox, tapped 18 px below its centre (inside the 44 px target, outside the drawn box)
        const cb = row.locator('input[type="checkbox"]').first();
        await cb.scrollIntoViewIfNeeded();
        const b = (await cb.boundingBox())!;
        await page.touchscreen.tap(b.x + b.width / 2, b.y + b.height / 2 + 18);
        await page.waitForTimeout(400);
        console.log(
          `REPORT click ${theme} checkbox checked: ${await cb.isChecked()}, bulk bar: ${await page.getByRole('button', { name: /clear selection/i }).count()}`,
        );
        await shot('checked');
        if (await cb.isChecked())
          await page.touchscreen.tap(b.x + b.width / 2, b.y + b.height / 2 - 18);
        // Invites: copy a link
        await openNav(page, settingsPath, /^Invites$/i);
        const copy = page
          .locator('table.ss-table tbody tr')
          .first()
          .locator('.ss-actions button')
          .first();
        await copy.scrollIntoViewIfNeeded();
        const before = (await copy.textContent())?.trim();
        await copy.tap();
        await page.waitForTimeout(500);
        console.log(
          `REPORT click ${theme} copy link: "${before}" -> "${(await copy.textContent())?.trim()}"`,
        );
        await shot('invite-copied');
      });
    }
    if (PARTS.has('small')) {
      if (world.voiceChannelId) {
        await guard(`${size} ${theme} voice`, async () => {
          await page.goto(channelPath(world.voiceChannelId!), {
            waitUntil: 'domcontentloaded',
            timeout: 15_000,
          });
          await idle(page, 4000);
          await look(page, 'chan-voice-overview', size);
        });
      }
      await guard(`${size} ${theme} text`, async () => {
        await page.goto(channelPath(world.channelId), {
          waitUntil: 'domcontentloaded',
          timeout: 15_000,
        });
        await idle(page, 4000);
        await look(page, 'chan-text-overview', size);
      });
      await guard(`${size} ${theme} select tab`, async () => {
        await openNav(page, settingsPath, /^Overview$/i);
        await look(page, 'ss-overview', size);
      });
      await guard(`${size} ${theme} compare`, async () => {
        await openNav(page, settingsPath, /^Server Support$/i);
        await look(page, 'ss-support-compare', size);
      });
    }
    await ctx.close().catch(() => undefined);
  }

  if (PARTS.has('admin')) {
    for (const size of ['phone', 'desktop'] as const) {
      await setTheme('lounge-dark');
      const { ctx, page } = await newPage(browser, size, 'lounge-dark', ownerCtx);
      for (const [id, re] of [
        ['accounts', /^Accounts$/i],
        ['limits', /^Limits/i],
        ['audit', /Platform audit log/i],
      ] as const)
        await guard(`${size} admin ${id}`, async () => {
          await openNav(page, '/admin/dashboard', re);
          await look(page, `admin-${id}`, size);
        });
      await ctx.close().catch(() => undefined);
    }
  }

  await ownerCtx.close();
  console.log(`REPORT totals ${JSON.stringify(totals)}`);
  writeFileSync(resolve(OUT, `totals-${TAG}.json`), JSON.stringify(totals, null, 1));
});
