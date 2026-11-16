import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-349 (wave 83; D-854): a look pass over every tab of the member's Settings. The Owner on the visual sweep's
 * seeded world (`e2e/.state/sweep/owner.json`, made by `page-sweep.spec.ts -g "seed the sweep world"`; this spec never
 * edits that spec). The tab list is read from the page (`[role=dialog] nav ul li > button`), each tab is clicked.
 *
 * Three settings per tab: 375x812 (touch, a mobile user agent) in Lounge Light and in Lounge Dark, and 1280x800 in
 * Editorial. The main theme is set with the account's settings (PATCH) and localStorage, and `data-theme` is read back
 * before each shot and written into the file name. Per tab: the viewport shot and, where the content is taller than two
 * viewports, one scrolled to its middle and one to its end. With each tab and setting one JSON line in `measure.json`:
 * sideways scroll of the document and of the tab's scroller; at 375 every visible control under 44 px tall; every
 * element whose own text is wider than its box and has no ellipsis; the boxes of the tab's cards and their direct
 * children (for the "nothing else moved" check of a second round).
 *
 * Local only (SR_P83_SETTINGS=1); shots and measure.json go to SR_P83_SETTINGS_OUT; SR_P83_ONLY is a regex over tab ids;
 * SR_P83_PAGES=1 also shoots and measures /support and /support/leaderboards (Lounge Dark, 375 and 1280) for the shared
 * sheet's before/after check; SR_P83_TAG names the run (file names get it, so a before and an after can sit together).
 */
const ON = process.env.SR_P83_SETTINGS === '1';
const OUT =
  process.env.SR_P83_SETTINGS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p83-settings';
const ONLY = process.env.SR_P83_ONLY ? new RegExp(process.env.SR_P83_ONLY) : null;
const TAG = process.env.SR_P83_TAG ? `${process.env.SR_P83_TAG}-` : '';
const PAGES = process.env.SR_P83_PAGES === '1';
const DOMSEL = process.env.SR_P83_DOM ?? '';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const LIMIT_MS = 25_000;

interface Setting {
  key: string;
  theme: string;
  size: { width: number; height: number };
  phone: boolean;
}
const SETTINGS: Setting[] = [
  { key: 'phone-light', theme: 'lounge-light', size: { width: 375, height: 812 }, phone: true },
  { key: 'phone-dark', theme: 'lounge-dark', size: { width: 375, height: 812 }, phone: true },
  {
    key: 'desk-editorial',
    theme: 'lounge-editorial',
    size: { width: 1280, height: 800 },
    phone: false,
  },
  // Only for SR_P83_PAGES: /support and the Leaderboards at 1280 in Lounge Dark (the tabs are not walked here).
  { key: 'desk-dark', theme: 'lounge-dark', size: { width: 1280, height: 800 }, phone: false },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(2_400_000);

const slug = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 40);

interface Box {
  sel: string;
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Measure {
  theme: string | null;
  doc: number;
  scroller: string | null;
  scrollerOver: number;
  scrollerH: number;
  scrollerClientH: number;
  innerH: number;
  small: { up: string[]; tag: string; cls: string; text: string; box: number[] }[];
  clipped: { tag: string; cls: string; text: string; sw: number; cw: number }[];
  boxes: Box[];
  low: {
    tag: string;
    cls: string;
    text: string;
    ratio: number;
    fg: string;
    bg: string;
    size: number;
  }[];
  dom: { sel: string; html: string }[];
}

/** In the page: everything the table needs, taken with the tab's scroller at the top. */
function measureInPage(arg: { phone: boolean; domSel: string }): Measure {
  const phone = arg.phone;
  const de = document.documentElement;
  const dlg = document.querySelector<HTMLElement>('[role="dialog"]') ?? document.body;
  const sel = (e: Element) => {
    const cls = typeof e.className === 'string' ? e.className.trim().split(/\s+/).slice(0, 3) : [];
    return `${e.tagName.toLowerCase()}${cls.length ? `.${cls.join('.')}` : ''}`;
  };
  const vis = (e: Element) => {
    const r = e.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return false;
    return (
      (e as HTMLElement).checkVisibility?.({ checkOpacity: false, checkVisibilityCSS: true }) ??
      true
    );
  };
  // The tab's scroller: the largest vertical scroller in the dialog that is not the sidebar nav.
  const scrollers = Array.from(dlg.querySelectorAll<HTMLElement>('*'))
    .filter((e) => {
      if (e.closest('nav')) return false;
      const cs = getComputedStyle(e);
      return /(auto|scroll|overlay)/.test(cs.overflowY) && e.clientWidth * e.clientHeight > 20000;
    })
    .sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight);
  const sc = scrollers[0] ?? null;
  const scTop = sc ? sc.getBoundingClientRect().top - sc.scrollTop : 0;
  const scLeft = sc ? sc.getBoundingClientRect().left : 0;
  const root = sc ?? dlg;

  const small: Measure['small'] = [];
  if (phone) {
    const ctl =
      'button, a[href], input, select, textarea, [role=switch], [role=tab], [role=checkbox], [role=radio]';
    for (const e of Array.from(root.querySelectorAll<HTMLElement>(ctl))) {
      if (!vis(e)) continue;
      if (e.closest('nav')) continue;
      const r = e.getBoundingClientRect();
      if (r.height >= 43.5) continue;
      const tag = e.tagName.toLowerCase();
      if (
        tag === 'a' &&
        e.closest('p, li > span, .us-muted') &&
        getComputedStyle(e).display === 'inline'
      )
        continue;
      if (tag === 'input') {
        const type = (e as HTMLInputElement).type;
        if (type === 'checkbox' || type === 'radio') {
          const lab = e.closest('label');
          if (lab && lab.getBoundingClientRect().height >= 43.5) continue;
        }
        if (type === 'hidden') continue;
      }
      const text = (
        e.getAttribute('aria-label') ??
        e.textContent ??
        (e as HTMLInputElement).placeholder ??
        ''
      )
        .trim()
        .replace(/\s+/g, ' ')
        .slice(0, 40);
      const up: string[] = [];
      for (let a = e.parentElement, n = 0; a && n < 3; a = a.parentElement, n++) up.push(sel(a));
      small.push({
        up,
        tag,
        cls: typeof e.className === 'string' ? e.className.slice(0, 60) : '',
        text,
        box: [
          Math.round(r.left),
          Math.round(r.top - scTop),
          Math.round(r.width),
          Math.round(r.height * 10) / 10,
        ],
      });
    }
  }

  const clipped: Measure['clipped'] = [];
  for (const e of Array.from(root.querySelectorAll<HTMLElement>('*'))) {
    if (e.closest('nav')) continue;
    if (e.clientWidth === 0 || e.scrollWidth <= e.clientWidth + 1) continue;
    const cs = getComputedStyle(e);
    if (cs.display === 'none' || cs.display === 'inline') continue;
    if (cs.textOverflow === 'ellipsis') continue;
    if (/(auto|scroll)/.test(cs.overflowX)) continue;
    if (e.classList.contains('us-sr')) continue;
    const own = Array.from(e.childNodes).some(
      (n) => n.nodeType === 3 && (n.textContent ?? '').trim().length > 0,
    );
    if (!own && !/^(input|select|textarea|button|pre|code)$/i.test(e.tagName)) continue;
    clipped.push({
      tag: e.tagName.toLowerCase(),
      cls: typeof e.className === 'string' ? e.className.slice(0, 60) : '',
      text: (e.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 40),
      sw: e.scrollWidth,
      cw: e.clientWidth,
    });
  }

  const boxes: Measure['boxes'] = [];
  const cards = Array.from(
    root.querySelectorAll<HTMLElement>('.us-card, .sto-mount, .us-preview, section'),
  );
  const seen = new Set<Element>();
  const add = (e: Element) => {
    if (seen.has(e) || !vis(e)) return;
    seen.add(e);
    const r = e.getBoundingClientRect();
    boxes.push({
      sel: sel(e),
      x: Math.round((r.left - scLeft) * 10) / 10,
      y: Math.round((r.top - scTop) * 10) / 10,
      w: Math.round(r.width * 10) / 10,
      h: Math.round(r.height * 10) / 10,
    });
  };
  for (const c of cards) {
    add(c);
    for (const k of Array.from(c.children)) add(k);
  }

  // Text under 4.5:1 (3:1 for large text): the text colour, with the opacity of its ancestors, over the first opaque background.
  const parse = (c: string) => {
    const m = /(rgba?|color)\(([^)]+)\)/.exec(c);
    if (!m) return null;
    const k = m[1] === 'color' ? 255 : 1;
    const v = m[2]!
      .replace('srgb', '')
      .split(/[ ,/]+/)
      .filter(Boolean)
      .map((x, i) => (i < 3 ? Number(x) * k : Number(x)));
    return {
      r: v[0]!,
      g: v[1]!,
      b: v[2]!,
      a: v[3] === undefined || Number.isNaN(v[3]) ? 1 : v[3]!,
    };
  };
  const lum = (c: { r: number; g: number; b: number }) => {
    const f = (x: number) => {
      const y = x / 255;
      return y <= 0.03928 ? y / 12.92 : Math.pow((y + 0.055) / 1.055, 2.4);
    };
    return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
  };
  const low: Measure['low'] = [];
  const seenLow = new Set<string>();
  for (const e of Array.from(root.querySelectorAll<HTMLElement>('*'))) {
    if (e.closest('nav') || !vis(e)) continue;
    const own = Array.from(e.childNodes)
      .filter((n) => n.nodeType === 3)
      .map((n) => (n.textContent ?? '').trim())
      .join(' ')
      .trim();
    if (!own) continue;
    const cs = getComputedStyle(e);
    let op = 1;
    for (let a: HTMLElement | null = e; a; a = a.parentElement)
      op *= Number(getComputedStyle(a).opacity);
    const fg = parse(cs.color);
    if (!fg) continue;
    const layers: { r: number; g: number; b: number; a: number }[] = [];
    for (let a: HTMLElement | null = e; a; a = a.parentElement) {
      const bg = parse(getComputedStyle(a).backgroundColor);
      if (bg && bg.a > 0) layers.push(bg);
      if (bg && bg.a >= 1) break;
    }
    let base = { r: 255, g: 255, b: 255 };
    for (const l of layers.reverse())
      base = {
        r: l.r * l.a + base.r * (1 - l.a),
        g: l.g * l.a + base.g * (1 - l.a),
        b: l.b * l.a + base.b * (1 - l.a),
      };
    const k = fg.a * op;
    const mix = {
      r: fg.r * k + base.r * (1 - k),
      g: fg.g * k + base.g * (1 - k),
      b: fg.b * k + base.b * (1 - k),
    };
    const l1 = lum(mix);
    const l2 = lum(base);
    const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
    const size = parseFloat(cs.fontSize);
    const large = size >= 24 || (size >= 18.66 && Number(cs.fontWeight) >= 700);
    if (ratio >= (large ? 3 : 4.5)) continue;
    const key = sel(e) + own.slice(0, 20);
    if (seenLow.has(key)) continue;
    seenLow.add(key);
    low.push({
      tag: e.tagName.toLowerCase(),
      cls: typeof e.className === 'string' ? e.className.slice(0, 60) : '',
      text: own.replace(/\s+/g, ' ').slice(0, 40),
      ratio: Math.round(ratio * 100) / 100,
      fg: cs.color,
      bg: `rgb(${Math.round(base.r)},${Math.round(base.g)},${Math.round(base.b)})`,
      size,
    });
  }
  const dom: Measure['dom'] = [];
  if (arg.domSel) {
    for (const e of Array.from(root.querySelectorAll<HTMLElement>(arg.domSel)).slice(0, 4)) {
      const par = e.parentElement ?? e;
      dom.push({ sel: sel(e), html: par.outerHTML.replace(/\s+/g, ' ').slice(0, 900) });
    }
  }

  const themed =
    dlg.closest('[data-theme]') ??
    dlg.querySelector('[data-theme]') ??
    document.querySelector('[data-theme]');
  return {
    theme: themed?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - de.clientWidth,
    scroller: sc ? sel(sc) : null,
    scrollerOver: sc ? sc.scrollWidth - sc.clientWidth : 0,
    scrollerH: sc ? sc.scrollHeight : de.scrollHeight,
    scrollerClientH: sc ? sc.clientHeight : de.clientHeight,
    innerH: window.innerHeight,
    small,
    clipped,
    boxes,
    low,
    dom,
  };
}

/** Scroll the tab's scroller to a fraction (0 top, 0.5 middle, 1 end); returns false when there is none. */
async function scrollTo(page: Page, frac: number) {
  return page.evaluate((f) => {
    const dlg = document.querySelector<HTMLElement>('[role="dialog"]') ?? document.body;
    const sc = Array.from(dlg.querySelectorAll<HTMLElement>('*'))
      .filter((e) => {
        if (e.closest('nav')) return false;
        return (
          /(auto|scroll|overlay)/.test(getComputedStyle(e).overflowY) &&
          e.clientWidth * e.clientHeight > 20000
        );
      })
      .sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight)[0];
    if (!sc) return false;
    sc.scrollTop = (sc.scrollHeight - sc.clientHeight) * f;
    return true;
  }, frac);
}

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

async function newPage(browser: Browser, s: Setting, base: BrowserContext) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: s.size,
    storageState: await base.storageState(),
    bypassCSP: true,
    ...(s.phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((theme) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({ state: { prefs: { theme, zoom: 100 } }, version: 2 }),
      );
    } catch {
      /* no storage */
    }
  }, s.theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(8000);
  return { ctx, page };
}

test('SEC-AUD-349: every tab of the member Settings, shot and measured', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, `${TAG}measure.json`);
  if (!ONLY) writeFileSync(jsonl, '');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const totals: Record<string, { tabs: number; shots: number; failed: string[] }> = {};

  for (const s of SETTINGS) {
    totals[s.key] = { tabs: 0, shots: 0, failed: [] };
    const tot = totals[s.key]!;
    // The main theme: the account's settings (PATCH) and localStorage (set in `newPage`), read back.
    await apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: s.theme, zoom: 100 } },
    });
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    console.log(`REPORT ${s.key} account theme read back: ${back.settings?.user_prefs?.theme}`);
    let { ctx, page } = await newPage(browser, s, ownerCtx);
    const fresh = async () => {
      await ctx.close().catch(() => undefined);
      ({ ctx, page } = await newPage(browser, s, ownerCtx));
    };

    const shoot = async (id: string, theme: string | null, part: string) => {
      const file = `${TAG}${id}-${s.key}-${theme ?? 'unknown'}${part}.png`;
      await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
      tot.shots++;
      return file;
    };

    const labelsOf = () =>
      page
        .locator('[role="dialog"] nav ul li > button')
        .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
    const path = '/settings/user/account';
    let labels: string[] = [];
    try {
      await withLimit('list', async () => {
        await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await idle(page, 4000);
        labels = await labelsOf();
      });
    } catch (e) {
      console.log(`REPORT ${s.key}: tab list not read: ${String(e).slice(0, 120)}`);
      await ctx.close();
      continue;
    }
    console.log(`REPORT ${s.key}: ${labels.length} tabs: ${labels.join(' | ')}`);

    for (let i = 0; i < (s.key === 'desk-dark' ? 0 : labels.length); i++) {
      const base = `${String(i + 1).padStart(2, '0')}-${slug(labels[i] ?? '')}`;
      if (ONLY && !ONLY.test(base)) continue;
      try {
        await withLimit(base, async () => {
          const btns = page.locator('[role="dialog"] nav ul li > button');
          if ((await btns.count()) !== labels.length) {
            await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
            await idle(page, 3000);
          }
          await btns.nth(i).evaluate((el) => (el as HTMLElement).click());
          await idle(page, 2500);
          await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
          await scrollTo(page, 0);
          await page.waitForTimeout(150);
          const m = await page.evaluate(measureInPage, { phone: s.phone, domSel: DOMSEL });
          const files = [await shoot(base, m.theme, '')];
          if (m.scrollerH > 2 * m.innerH) {
            if (await scrollTo(page, 0.5)) {
              await page.waitForTimeout(150);
              files.push(await shoot(base, m.theme, '-mid'));
            }
            if (await scrollTo(page, 1)) {
              await page.waitForTimeout(150);
              files.push(await shoot(base, m.theme, '-end'));
            }
            await scrollTo(page, 0);
          }
          const line = {
            id: base,
            tab: labels[i],
            setting: s.key,
            wantTheme: s.theme,
            url: page.url().replace(ORIGIN, ''),
            ...m,
            files,
          };
          appendFileSync(jsonl, `${JSON.stringify(line)}\n`);
          tot.tabs++;
          console.log(
            `REPORT ${s.key} ${base}: doc ${m.doc} scroller ${m.scrollerOver} theme ${m.theme}${m.theme === s.theme ? '' : ' MISMATCH'} small ${m.small.length} clipped ${m.clipped.length} shots ${files.length}`,
          );
        });
      } catch (e) {
        tot.failed.push(base);
        appendFileSync(
          jsonl,
          `${JSON.stringify({ id: base, setting: s.key, ok: false, error: String(e).slice(0, 200) })}\n`,
        );
        console.log(`REPORT ${s.key} ${base}: NOT MEASURED ${String(e).slice(0, 120)}`);
        await fresh();
        await page
          .goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 })
          .catch(() => undefined);
      }
    }

    // The shared sheet's other loaders: Support and the Leaderboards (Lounge Dark, both sizes).
    if (PAGES && (s.key === 'phone-dark' || s.key === 'desk-dark')) {
      for (const p of ['/support', '/support/leaderboards']) {
        const id = `page-${slug(p)}`;
        try {
          await withLimit(id, async () => {
            await page.goto(p, { waitUntil: 'domcontentloaded', timeout: 15_000 });
            await idle(page, 4000);
            const m = await page.evaluate(measureInPage, { phone: s.phone, domSel: DOMSEL });
            const root = await page.evaluate(() => {
              const r = Array.from(document.querySelectorAll<HTMLElement>('main *, body > div *'))
                .filter((e) => /^(h1|h2|h3|p|button|a|section|article|li)$/i.test(e.tagName))
                .slice(0, 400)
                .map((e) => {
                  const b = e.getBoundingClientRect();
                  return [
                    e.tagName.toLowerCase(),
                    typeof e.className === 'string' ? e.className.slice(0, 40) : '',
                    Math.round(b.left * 10) / 10,
                    Math.round((b.top + window.scrollY) * 10) / 10,
                    Math.round(b.width * 10) / 10,
                    Math.round(b.height * 10) / 10,
                  ];
                });
              return r;
            });
            const file = await shoot(id, m.theme, '');
            appendFileSync(
              jsonl,
              `${JSON.stringify({ id, setting: s.key, url: p, doc: m.doc, theme: m.theme, files: [file], pageBoxes: root })}\n`,
            );
            console.log(
              `REPORT ${s.key} ${id}: doc ${m.doc} theme ${m.theme} boxes ${root.length}`,
            );
          });
        } catch (e) {
          tot.failed.push(id);
          console.log(`REPORT ${s.key} ${id}: NOT MEASURED ${String(e).slice(0, 120)}`);
          await fresh();
        }
      }
    }
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
  console.log(`REPORT totals ${JSON.stringify(totals)}`);
  writeFileSync(resolve(OUT, `${TAG}totals.json`), JSON.stringify(totals, null, 1));
});
