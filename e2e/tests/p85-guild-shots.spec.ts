import { appendFileSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * Wave 85, guild (SEC-AUD-369, 370; D-873). Local only (SR_P85_GUILD=1).
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`'s "seed the sweep world";
 * that spec is never edited), as `p84-guild-shots.spec.ts` does.
 *
 *   SR_P85_TAG   file-name prefix and log tag: `base` (before) or `after`.
 *   SR_P85_OUT   the folder of the shots and `measure.json`.
 *   SR_P85_BASE  1: the base case (measure only what is wrong; same shots, tag `base`).
 *   SR_P85_PART  comma list of: compare (Server Support at 375 Light/Dark, 1280 Editorial; the Leaderboards at 375),
 *                fields (Admin > Library catalog and Invite graph at 375 Light/Dark and 1280; one server-settings tab).
 *
 * Compare levels: which element scrolls, its overflow, each column's width, the label column's lines, the sticky
 * column's computed ground, and whether a figure shows through it; shots at scroll 0 and at the end.
 * Fields: every text input / select / button of the form with its box, the computed inline-size, the `size`
 * attribute, and the D-856 hit test (touch, `elementFromPoint(cx, cy -/+ 21)`).
 */
const ON = process.env.SR_P85_GUILD === '1';
const OUT =
  process.env.SR_P85_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p85-guild';
const TAG = process.env.SR_P85_TAG ?? (process.env.SR_P85_BASE === '1' ? 'base' : 'after');
const PARTS = new Set((process.env.SR_P85_PART ?? 'compare,fields').split(','));
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
test.setTimeout(1_200_000);

interface World {
  guildId: string;
  channelId: string;
}
const loadWorld = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

/** In the page: Compare levels' scroller, columns and the sticky column's ground. */
function measureCompare() {
  const de = document.documentElement;
  const t = document.querySelector<HTMLElement>('table.sup-compare');
  const themed = document.querySelector('[data-theme]');
  if (!t) return { theme: themed?.getAttribute('data-theme') ?? null, found: false };
  t.scrollIntoView({ block: 'center' });
  const card = t.closest<HTMLElement>('.ss-card');
  const cr = card?.getBoundingClientRect();
  const cs = getComputedStyle(t);
  const lh = (e: HTMLElement) => parseFloat(getComputedStyle(e).lineHeight) || 18;
  const cols = Array.from(t.querySelectorAll<HTMLElement>('thead th')).map((th) => {
    const r = th.getBoundingClientRect();
    const c = getComputedStyle(th);
    return {
      text: (th.textContent ?? '').trim().slice(0, 14),
      w: Math.round(r.width),
      pad: `${c.paddingInlineStart}/${c.paddingInlineEnd}`,
      current: th.getAttribute('aria-current'),
      weight: c.fontWeight,
      color: c.color,
    };
  });
  const labels = Array.from(t.querySelectorAll<HTMLElement>('tbody tr > :first-child')).map((e) => {
    const r = e.getBoundingClientRect();
    return {
      text: (e.textContent ?? '').trim().slice(0, 30),
      lines: Math.round((r.height - 16) / lh(e)),
      w: Math.round(r.width),
    };
  });
  const first = t.querySelector<HTMLElement>('tbody tr > :first-child');
  const sc = (e: Element | null) => {
    const out: string[] = [];
    for (let n: Element | null = e; n && n !== document.body; n = n.parentElement) {
      const c = getComputedStyle(n);
      if (/(auto|scroll)/.test(c.overflowX))
        out.push(
          `${n.tagName.toLowerCase()}.${String(n.className).slice(0, 40)}:${n.scrollWidth - n.clientWidth}`,
        );
    }
    return out;
  };
  return {
    found: true,
    theme: themed?.getAttribute('data-theme') ?? null,
    vw: de.clientWidth,
    doc: de.scrollWidth - de.clientWidth,
    tableOver: t.scrollWidth - t.clientWidth,
    tableClient: t.clientWidth,
    tableDisplay: cs.display,
    tableOverflowX: cs.overflowX,
    scrollingAncestors: sc(t),
    cardContentW: card ? Math.round(cr!.width - 34) : null,
    cols,
    labels,
    stickyPos: first ? getComputedStyle(first).position : null,
    stickyBg: first ? getComputedStyle(first).backgroundColor : null,
    cardBg: card ? getComputedStyle(card).backgroundColor : null,
    tableBgImage: cs.backgroundImage.slice(0, 80),
    labelInlineStart: first
      ? Math.round(first.getBoundingClientRect().left - (cr?.left ?? 0))
      : null,
  };
}

/** Scroll Compare levels' box to its end (or back to 0) and read what shows through the label column. */
function scrollCompare(to: string) {
  const t = document.querySelector<HTMLElement>('table.sup-compare')!;
  t.scrollLeft = to === 'end' ? t.scrollWidth : 0;
  const first = t.querySelector<HTMLElement>('tbody tr > :first-child')!;
  const r = first.getBoundingClientRect();
  const x = r.left + r.width / 2;
  // What is drawn at the middle of the label cell's inline-end half: the label column (or text in it), never another cell.
  const probes = Array.from(t.querySelectorAll<HTMLElement>('tbody tr')).map((row) => {
    const f = row.firstElementChild as HTMLElement;
    const fr = f.getBoundingClientRect();
    const h = document.elementFromPoint(fr.right - 3, fr.top + fr.height / 2);
    return h === f || f.contains(h);
  });
  const last = t.querySelector<HTMLElement>('thead th:last-child')!;
  const lr = last.getBoundingClientRect();
  const tr = t.getBoundingClientRect();
  return {
    scrollLeft: Math.round(t.scrollLeft),
    labelLeft: Math.round(r.left),
    x: Math.round(x),
    labelColumnOnTop: probes.every(Boolean),
    lastColumnWhole: lr.left >= tr.left - 1 && lr.right <= tr.right + 1,
    lastColumnRight: Math.round(lr.right),
    tableRight: Math.round(tr.right),
  };
}

/** In the page: the controls of the first form that holds a text-like input and a select. */
function measureForm() {
  const de = document.documentElement;
  const vw = de.clientWidth;
  const form =
    Array.from(document.querySelectorAll<HTMLElement>('form.ss-toolbar, .ss-toolbar')).find(
      (f) =>
        f.querySelector('input:not([type=checkbox])') && f.querySelector('select, [role=combobox]'),
    ) ?? null;
  const themed = document.querySelector('[data-theme]');
  if (!form) return { found: false, theme: themed?.getAttribute('data-theme') ?? null };
  form.scrollIntoView({ block: 'center' });
  const hitOk = (el: HTMLElement, x: number, y: number) => {
    const h = document.elementFromPoint(x, y);
    if (!h) return false;
    if (h === el || el.contains(h)) return true;
    const lab =
      el.closest('label') ?? (el.id ? document.querySelector(`label[for="${el.id}"]`) : null);
    return !!lab && (h === lab || lab.contains(h));
  };
  const fr = form.getBoundingClientRect();
  const card = form.closest<HTMLElement>('.ss-card, [class*=card]');
  const ctls = Array.from(
    form.querySelectorAll<HTMLElement>(
      'input:not([type=checkbox]), select, button, [role=combobox]',
    ),
  )
    .filter((e) => e.getBoundingClientRect().width > 0)
    .map((e) => {
      const r = e.getBoundingClientRect();
      const cs = getComputedStyle(e);
      const wrap = e.parentElement ? getComputedStyle(e.parentElement) : null;
      const cx = r.left + r.width / 2;
      const cy = r.top + r.height / 2;
      return {
        tag: e.tagName.toLowerCase() + (e instanceof HTMLInputElement ? `:${e.type}` : ''),
        cls: String(e.className).slice(0, 50),
        name: (e.getAttribute('aria-label') || e.textContent || '').trim().slice(0, 30),
        x: Math.round(r.left * 10) / 10,
        right: Math.round(r.right * 10) / 10,
        w: Math.round(r.width * 10) / 10,
        h: Math.round(r.height * 10) / 10,
        inlineSize: cs.inlineSize,
        maxInlineSize: cs.maxInlineSize,
        flex: cs.flex,
        sizeAttr: e.getAttribute('size'),
        parent: e.parentElement
          ? `${e.parentElement.tagName.toLowerCase()}.${String(e.parentElement.className).slice(0, 40)}`
          : '',
        parentInline: wrap ? `${wrap.display} flex:${wrap.flex} w:${wrap.inlineSize}` : '',
        hitTop: hitOk(e, cx, cy - 21),
        hitBottom: hitOk(e, cx, cy + 21),
        inView: r.left >= 0 && r.right <= vw,
      };
    });
  return {
    found: true,
    theme: themed?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - vw,
    form: `${form.tagName.toLowerCase()}.${String(form.className).slice(0, 40)}`,
    formBox: [Math.round(fr.left), Math.round(fr.width)],
    cardBox: card
      ? [
          Math.round(card.getBoundingClientRect().left),
          Math.round(card.getBoundingClientRect().width),
        ]
      : null,
    ctls,
  };
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

test('SEC-AUD-369 and 370: Compare levels on a phone, a toolbar text field', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const world = loadWorld();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const totals = { shots: 0, failed: [] as string[] };
  const log = (id: string, size: Size, data: unknown) => {
    appendFileSync(jsonl, `${JSON.stringify({ id, tag: TAG, size, data })}\n`);
    console.log(`REPORT ${TAG} ${size} ${id} ${JSON.stringify(data).slice(0, 1800)}`);
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
  const shoot = async (page: Page, name: string, size: Size, theme: string) => {
    await page.screenshot({
      path: resolve(OUT, `${TAG}-${name}-${size === 'phone' ? '375' : '1280'}-${theme}.png`),
      timeout: 10_000,
    });
    totals.shots++;
  };
  const guard = async (what: string, fn: () => Promise<void>) => {
    try {
      await fn();
    } catch (e) {
      totals.failed.push(`${what}: ${String(e).slice(0, 160)}`);
      console.log(`REPORT FAILED ${what}: ${String(e).slice(0, 200)}`);
    }
  };
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
  };

  for (const { size, theme } of SETTINGS) {
    await setTheme(theme);
    const { ctx, page } = await newPage(browser, size, theme, ownerCtx);
    if (PARTS.has('compare')) {
      await guard(`${size} ${theme} compare`, async () => {
        await openNav(page, `/settings/server/${world.guildId}`, /^Server Support$/i);
        await page.evaluate(() => document.fonts?.ready);
        const m = await page.evaluate(measureCompare);
        log('compare', size, m);
        await shoot(page, 'compare-start', size, theme);
        if (size === 'phone') {
          const end = await page.evaluate(scrollCompare, 'end');
          await page.waitForTimeout(250);
          log('compare-end', size, end);
          await shoot(page, 'compare-end', size, theme);
          await page.evaluate(scrollCompare, 'start');
          // The tab's own scroller must not scroll sideways.
          const tab = await page.evaluate(() => {
            const de = document.documentElement;
            let worst = 0;
            for (const e of Array.from(document.querySelectorAll<HTMLElement>('[role=dialog] *'))) {
              if (/(auto|scroll)/.test(getComputedStyle(e).overflowY) && e.clientWidth > 300)
                worst = Math.max(worst, e.scrollWidth - e.clientWidth);
            }
            return { doc: de.scrollWidth - de.clientWidth, tabScroller: worst };
          });
          log('compare-tab', size, tab);
        }
      });
      if (size === 'phone') {
        await guard(`${size} ${theme} leaderboards`, async () => {
          await page.goto('/support/leaderboards', {
            waitUntil: 'domcontentloaded',
            timeout: 15_000,
          });
          await idle(page, 4000);
          const doc = await page.evaluate(() => {
            const de = document.documentElement;
            return {
              doc: de.scrollWidth - de.clientWidth,
              theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
              boxes: Array.from(
                document.querySelectorAll<HTMLElement>('main *, [class*=card], table'),
              )
                .slice(0, 60)
                .map((e) => {
                  const r = e.getBoundingClientRect();
                  return [
                    Math.round(r.left),
                    Math.round(r.top),
                    Math.round(r.width),
                    Math.round(r.height),
                  ];
                }),
            };
          });
          log('leaderboards', size, doc);
          await shoot(page, 'leaderboards', size, theme);
        });
      }
    }
    if (PARTS.has('fields')) {
      for (const [id, re] of [
        ['library', /Library catalog/i],
        ['invite-graph', /Invite graph/i],
      ] as const)
        await guard(`${size} ${theme} ${id}`, async () => {
          await openNav(page, '/admin/dashboard', re);
          const m = await page.evaluate(measureForm);
          log(`form-${id}`, size, m);
          await shoot(page, `form-${id}`, size, theme);
        });
    }
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
  console.log(`REPORT totals ${JSON.stringify(totals)}`);
});
