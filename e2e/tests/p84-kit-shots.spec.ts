import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';

/**
 * SEC-AUD-351, 352 and 353 (wave 84; D-856, D-857): the kit's 44 px touch targets on a phone, a disabled choice row's
 * text, a segmented control's labels. The Owner on the visual sweep's seeded world (`e2e/.state/sweep`, made by
 * `page-sweep.spec.ts -g "seed the sweep world"`; that spec is never edited). Local only.
 *
 *   SR_P84_KIT=1            run
 *   SR_P84_BASE=1           the base case: the run's folder is `<out>/base` and its files carry `base-` (default `after`)
 *   SR_P84_KIT_OUT          the out folder (default C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p84-kit)
 *   SR_P84_PART             a comma list of: hit, contrast, seg, wide (default all four)
 *   SR_P84_ONLY             a regex over tab / view ids
 *
 *  - hit: 375x812, touch, mobile user agent, Lounge Dark (PATCH and localStorage, `data-theme` read back). The tabs of
 *    the member's Settings, of a server's settings, of a role's editor and of a text channel's settings are walked as
 *    the wave 83 specs walk them; on each tab every visible `button, a[href], input, select, textarea, [role=switch],
 *    [role=tab], [role=checkbox], [role=radio]` is put through the plan's hit test (`elementFromPoint` 21 px above and
 *    below the box's centre returns the control, a node inside it or its own label; a link inside a paragraph is left
 *    out). One JSON line per tab in `hit.jsonl`: how many pass, how many fail, the failing ones.
 *  - contrast: Streamer Mode's and Appearance's disabled rows at 1280 in Lounge Light, Lounge Dark and Editorial: the
 *    label's and the description's colour, the opacity of their ancestors, the pixels behind, the ratio; the boxes of
 *    the row, the control and the label. `contrast.jsonl`.
 *  - seg: the segmented controls of Privacy & Safety, Profiles and Activity Privacy at 375 (Lounge Dark) and 1280
 *    (Editorial): each pill's box, how many lines its text takes and the gap between the text and the pill's edge.
 *    `seg.jsonl`, and a shot of each.
 *  - wide: the wide shot check. The signed-out sign-in and invite pages, the member's and a server's settings pages,
 *    the two dialogs, the Lounge, one Admin page, the Library's, the Cinema's and the Arcade's front page: a shot of
 *    each and the boxes of every visible element (`wide.jsonl`) for the "nothing moved at 1280" comparison.
 */
const ON = process.env.SR_P84_KIT === '1';
const BASE = process.env.SR_P84_BASE === '1';
const TAG = BASE ? 'base' : 'after';
const OUT_ROOT =
  process.env.SR_P84_KIT_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p84-kit';
const OUT = resolve(OUT_ROOT, TAG);
const PARTS = new Set((process.env.SR_P84_PART ?? 'hit,contrast,seg,wide').split(','));
const ONLY = process.env.SR_P84_ONLY ? new RegExp(process.env.SR_P84_ONLY) : null;
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
const LIMIT_MS = 30_000;
const NAV = '[role="dialog"] nav ul li > button';

test.skip(!ON, 'local visual check only');
test.setTimeout(2_400_000);

interface World {
  guildId: string;
  channelId: string;
  inviteCode: string | null;
}
const world = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

const log = (file: string, line: Record<string, unknown>) => {
  mkdirSync(OUT, { recursive: true });
  appendFileSync(resolve(OUT, file), `${JSON.stringify(line)}\n`);
};
const slug = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 40);

type Prefs = { theme: string; zoom: number; sectionThemes?: Record<string, string> };

async function setPrefs(ownerCtx: BrowserContext, prefs: Prefs) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', { settings: { user_prefs: prefs } });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string; sectionThemes?: Record<string, string> } };
  };
  console.log(
    `REPORT account theme set ${prefs.theme}, read back: ${back.settings?.user_prefs?.theme} ${JSON.stringify(back.settings?.user_prefs?.sectionThemes ?? {})}`,
  );
}

async function newPage(
  browser: Browser,
  size: Size,
  base: BrowserContext | null,
  prefs: Prefs | null,
) {
  const phone = size === 'phone';
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: SIZES[size],
    ...(base ? { storageState: await base.storageState() } : {}),
    bypassCSP: true,
    ...(phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  if (prefs)
    await ctx.addInitScript((p) => {
      try {
        localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
      } catch {
        /* no storage */
      }
    }, prefs);
  const page = await ctx.newPage();
  page.setDefaultTimeout(8000);
  await page.route(/sweep-preview\.invalid|\/cdn\/external\//, (r: Route) =>
    r.fulfill({ status: 200, contentType: 'image/png', body: Buffer.alloc(0) }),
  );
  return { ctx, page };
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

async function hit(loc: Locator, size: Size, force = false) {
  const el = loc.first();
  await el.scrollIntoViewIfNeeded({ timeout: 4000 }).catch(() => undefined);
  if (size === 'phone') await el.tap({ force, timeout: 5000 });
  else await el.click({ force, timeout: 5000 });
  await el.page().waitForTimeout(450);
}

const go = async (page: Page, path: string) => {
  await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
  await idle(page, 4000);
};

// ---------------------------------------------------------------------------------------------------------------
// In the page
// ---------------------------------------------------------------------------------------------------------------

interface Failing {
  tag: string;
  type: string;
  role: string;
  cls: string;
  text: string;
  box: number[];
  up: string[];
  top: string;
  bottom: string;
  kind: string;
}
interface HitResult {
  theme: string | null;
  pass: number;
  fail: number;
  kinds: Record<string, { pass: number; fail: number }>;
  failing: Failing[];
}

/** The plan's hit test, over the controls the tab shows. */
function hitTestInPage(): HitResult {
  const sel = (e: Element | null) => {
    if (!e) return 'null';
    const cls = typeof e.className === 'string' ? e.className.trim().split(/\s+/).slice(0, 2) : [];
    return `${e.tagName.toLowerCase()}${cls.length ? `.${cls.join('.')}` : ''}`;
  };
  const dlg = document.querySelector<HTMLElement>('[role="dialog"]') ?? document.body;
  const ctl =
    'button, a[href], input, select, textarea, [role=switch], [role=tab], [role=checkbox], [role=radio]';
  const kindOf = (e: HTMLElement) => {
    const tag = e.tagName.toLowerCase();
    const cls = typeof e.className === 'string' ? e.className : '';
    const type = (e as HTMLInputElement).type ?? '';
    if (tag === 'input') {
      if (/segInput/.test(cls)) return 'segmented-pill';
      if (type === 'range') return 'range';
      if (type === 'checkbox' && e.getAttribute('role') === 'switch') return 'switch';
      if (type === 'checkbox' || type === 'radio') return type;
      return 'text-field';
    }
    if (tag === 'select' || tag === 'textarea') return tag;
    if (/triBtn/.test(cls)) return 'permission-tri';
    if (/swatch/.test(cls)) return 'swatch';
    if (/navToggle/.test(cls)) return 'settings-nav-toggle';
    if (e.getAttribute('role') === 'tab') return 'tab';
    if (/trigger/.test(cls) && e.closest('[class*="control"]')) return 'select-trigger';
    if (tag === 'a') return 'link';
    if (tag === 'button') {
      if (/_link_|link/.test(cls) && !/ss-|us-/.test(cls)) return 'button-link';
      if (/(^|\s|_)(sm|md|lg)(_|\s|$)/.test(cls)) return 'kit-button';
      return 'button-other';
    }
    return tag;
  };
  const vis = (e: HTMLElement) => {
    const r = e.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) return false;
    return e.checkVisibility?.({ checkOpacity: false, checkVisibilityCSS: true }) ?? true;
  };
  const kinds: HitResult['kinds'] = {};
  const failing: Failing[] = [];
  let pass = 0;
  let fail = 0;
  const seen = new Set<Element>();
  for (const e of Array.from(dlg.querySelectorAll<HTMLElement>(ctl))) {
    if (seen.has(e)) continue;
    seen.add(e);
    if (!vis(e)) continue;
    const tag = e.tagName.toLowerCase();
    const type = (e as HTMLInputElement).type ?? '';
    if (type === 'hidden') continue;
    if (
      tag === 'a' &&
      e.closest('p, li > span, .us-muted') &&
      getComputedStyle(e).display === 'inline'
    )
      continue;
    e.scrollIntoView({ block: 'center', inline: 'nearest' });
    const r = e.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const labels = Array.from((e as HTMLInputElement).labels ?? []);
    const ok = (y: number) => {
      const t = document.elementFromPoint(cx, y);
      return {
        ok: !!t && (t === e || e.contains(t) || labels.some((l) => l === t || l.contains(t))),
        sel: sel(t),
      };
    };
    const a = ok(cy - 21);
    const b = ok(cy + 21);
    const kind = kindOf(e);
    const k = (kinds[kind] ??= { pass: 0, fail: 0 });
    if (a.ok && b.ok) {
      pass++;
      k.pass++;
      continue;
    }
    fail++;
    k.fail++;
    const up: string[] = [];
    for (let p = e.parentElement, n = 0; p && n < 3; p = p.parentElement, n++) up.push(sel(p));
    failing.push({
      tag,
      type,
      role: e.getAttribute('role') ?? '',
      cls: typeof e.className === 'string' ? e.className.slice(0, 80) : '',
      text: (
        e.getAttribute('aria-label') ??
        e.textContent ??
        (e as HTMLInputElement).placeholder ??
        ''
      )
        .trim()
        .replace(/\s+/g, ' ')
        .slice(0, 40),
      box: [
        Math.round(r.left),
        Math.round(r.top),
        Math.round(r.width * 10) / 10,
        Math.round(r.height * 10) / 10,
      ],
      up,
      top: a.sel,
      bottom: b.sel,
      kind,
    });
  }
  for (const e of Array.from(document.querySelectorAll<HTMLElement>('*')))
    if (e.scrollTop > 0) e.scrollTop = 0;
  const themed = dlg.closest('[data-theme]') ?? document.querySelector('[data-theme]');
  return { theme: themed?.getAttribute('data-theme') ?? null, pass, fail, kinds, failing };
}

interface TextContrast {
  text: string;
  ratio: number;
  fg: string;
  bg: string;
  opacityChain: number;
}
interface DisabledRow {
  kind: string;
  label: TextContrast | null;
  desc: TextContrast | null;
  inputOpacity: number;
  cursor: string;
  rowBox: number[];
  inputBox: number[];
  labelBox: number[];
  theme: string | null;
}

/** Every disabled checkbox, radio and switch: the label's and the description's contrast, and the row's boxes. */
function disabledRowsInPage(): DisabledRow[] {
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
  const contrast = (e: HTMLElement): TextContrast => {
    const cs = getComputedStyle(e);
    let op = 1;
    for (let a: HTMLElement | null = e; a; a = a.parentElement)
      op *= Number(getComputedStyle(a).opacity);
    const fg = parse(cs.color) ?? { r: 0, g: 0, b: 0, a: 1 };
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
    return {
      text: (e.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 50),
      ratio: Math.round(((Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05)) * 100) / 100,
      fg: cs.color,
      bg: `rgb(${Math.round(base.r)},${Math.round(base.g)},${Math.round(base.b)})`,
      opacityChain: Math.round(op * 1000) / 1000,
    };
  };
  const box = (e: Element) => {
    const r = e.getBoundingClientRect();
    return [
      Math.round(r.left * 10) / 10,
      Math.round(r.top * 10) / 10,
      Math.round(r.width * 10) / 10,
      Math.round(r.height * 10) / 10,
    ];
  };
  const out: DisabledRow[] = [];
  const themed = document.querySelector('[data-theme]');
  for (const i of Array.from(document.querySelectorAll<HTMLInputElement>('input:disabled'))) {
    if (i.type !== 'checkbox' && i.type !== 'radio') continue;
    if (/segInput/.test(typeof i.className === 'string' ? i.className : '')) continue;
    const lab = i.labels?.[0];
    if (!lab || i.getBoundingClientRect().width < 2) continue;
    const span = lab.querySelector<HTMLElement>('span');
    const desc = lab.querySelector<HTMLElement>('[id$="-desc"]');
    out.push({
      kind: i.getAttribute('role') === 'switch' ? 'switch' : i.type,
      label: span ? contrast(span) : null,
      desc: desc ? contrast(desc) : null,
      inputOpacity: Number(getComputedStyle(i).opacity),
      cursor: getComputedStyle(lab).cursor,
      rowBox: box(i.parentElement ?? i),
      inputBox: box(i),
      labelBox: box(lab),
      theme: themed?.getAttribute('data-theme') ?? null,
    });
  }
  return out;
}

interface SegPill {
  text: string;
  box: number[];
  lines: number;
  gapStart: number;
  gapEnd: number;
  gapTop: number;
  gapBottom: number;
}
/** Every segmented control on the page: its pills, the lines of each pill's text, the gap to the pill's edges. */
function segmentedInPage(): { group: number[]; label: string; pills: SegPill[] }[] {
  const out: { group: number[]; label: string; pills: SegPill[] }[] = [];
  for (const g of Array.from(document.querySelectorAll<HTMLElement>('[role="radiogroup"]'))) {
    const pills = Array.from(g.children).filter(
      (c): c is HTMLElement =>
        c.tagName === 'LABEL' && !!c.querySelector('input[type="radio"][class*="segInput"]'),
    );
    if (pills.length === 0) continue;
    const gr = g.getBoundingClientRect();
    out.push({
      group: [
        Math.round(gr.left * 10) / 10,
        Math.round(gr.top * 10) / 10,
        Math.round(gr.width * 10) / 10,
        Math.round(gr.height * 10) / 10,
      ],
      label: g.getAttribute('aria-label') ?? '',
      pills: pills.map((p) => {
        const span = p.querySelector('span') ?? p;
        const range = document.createRange();
        range.selectNodeContents(span);
        const rects = Array.from(range.getClientRects());
        const lines = new Set(rects.map((r) => Math.round(r.top / 4))).size;
        const tr = range.getBoundingClientRect();
        const pr = p.getBoundingClientRect();
        const r1 = (n: number) => Math.round(n * 10) / 10;
        return {
          text: (span.textContent ?? '').trim().slice(0, 40),
          box: [r1(pr.left), r1(pr.top), r1(pr.width), r1(pr.height)],
          lines,
          gapStart: r1(tr.left - pr.left),
          gapEnd: r1(pr.right - tr.right),
          gapTop: r1(tr.top - pr.top),
          gapBottom: r1(pr.bottom - tr.bottom),
        };
      }),
    });
  }
  return out;
}

/** The boxes of every visible element (up to 900, in document order) and the page's sideways scroll. */
function layoutInPage(): { doc: number; themes: string[]; boxes: (string | number)[][] } {
  const de = document.documentElement;
  const boxes: (string | number)[][] = [];
  for (const e of Array.from(document.body.querySelectorAll<HTMLElement>('*'))) {
    if (boxes.length >= 900) break;
    const r = e.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) continue;
    const cls = typeof e.className === 'string' ? e.className.trim().split(/\s+/).slice(0, 2) : [];
    boxes.push([
      `${e.tagName.toLowerCase()}${cls.length ? `.${cls.join('.')}` : ''}`,
      Math.round(r.left * 10) / 10,
      Math.round((r.top + window.scrollY) * 10) / 10,
      Math.round(r.width * 10) / 10,
      Math.round(r.height * 10) / 10,
    ]);
  }
  const themes = Array.from(
    new Set(
      Array.from(document.querySelectorAll('[data-theme]')).map(
        (e) => e.getAttribute('data-theme') ?? '',
      ),
    ),
  );
  return { doc: de.scrollWidth - de.clientWidth, themes, boxes };
}

// ---------------------------------------------------------------------------------------------------------------
// hit: the walk
// ---------------------------------------------------------------------------------------------------------------

const totals = {
  hit: {} as Record<string, { pass: number; fail: number }>,
  tabs: 0,
  failed: [] as string[],
  shots: 0,
};

async function walkTabs(
  page: Page,
  area: string,
  path: string,
  tabSel: string,
  opts: { prepare?: () => Promise<void>; direct?: boolean } = {},
) {
  await go(page, path);
  if (opts.prepare) await opts.prepare();
  const labels = await page
    .locator(tabSel)
    .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
  console.log(`REPORT ${area}: ${labels.length} tabs: ${labels.join(' | ')}`);
  for (let i = 0; i < labels.length; i++) {
    const id = `${area}-${String(i + 1).padStart(2, '0')}-${slug(labels[i] ?? '')}`;
    if (ONLY && !ONLY.test(id)) continue;
    try {
      await withLimit(id, async () => {
        if ((await page.locator(tabSel).count()) !== labels.length) {
          await go(page, path);
          if (opts.prepare) await opts.prepare();
        }
        await page
          .locator(tabSel)
          .nth(i)
          .evaluate((el) => (el as HTMLElement).click());
        await idle(page, 2500);
        await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
        const m = await page.evaluate(hitTestInPage);
        log('hit.jsonl', { id, area, tab: labels[i], url: page.url().replace(ORIGIN, ''), ...m });
        totals.tabs++;
        for (const [k, v] of Object.entries(m.kinds)) {
          const t = (totals.hit[k] ??= { pass: 0, fail: 0 });
          t.pass += v.pass;
          t.fail += v.fail;
        }
        console.log(
          `REPORT ${id}: theme ${m.theme} pass ${m.pass} fail ${m.fail} ${JSON.stringify(m.kinds)}`,
        );
      });
    } catch (e) {
      totals.failed.push(id);
      log('hit.jsonl', { id, ok: false, error: String(e).slice(0, 200) });
      console.log(`REPORT ${id}: NOT MEASURED ${String(e).slice(0, 120)}`);
      await go(page, path).catch(() => undefined);
      if (opts.prepare) await opts.prepare().catch(() => undefined);
    }
  }
}

test('SEC-AUD-351: the hit test over every tab', async ({ browser }) => {
  test.skip(!PARTS.has('hit'), 'part off');
  mkdirSync(OUT, { recursive: true });
  if (!ONLY) writeFileSync(resolve(OUT, 'hit.jsonl'), '');
  const w = world();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const prefs: Prefs = { theme: 'lounge-dark', zoom: 100 };
  await setPrefs(ownerCtx, prefs);
  const { ctx, page } = await newPage(browser, 'phone', ownerCtx, prefs);

  await walkTabs(page, 'user', '/settings/user/account', NAV);
  await walkTabs(page, 'ss', `/settings/server/${w.guildId}`, NAV);

  // A role's editor (its Permissions tab has the three-state buttons).
  if (!ONLY || ONLY.test('role')) {
    const LONG_ROLE = 'A very long role name that keeps going past the end of one line on a phone';
    const have = await ownerCtx.request.get(`/api/v1/guilds/${w.guildId}/roles`);
    if (!have.ok() || !(await have.text()).includes(LONG_ROLE))
      await apiPost(ownerCtx.request, `/guilds/${w.guildId}/roles`, {
        name: LONG_ROLE,
        color: 0x2ecc71,
      }).catch((e) => console.log(`REPORT long role not made: ${String(e).slice(0, 120)}`));
    await walkTabs(page, 'role', `/settings/server/${w.guildId}/roles`, '[role="tab"]', {
      prepare: async () => {
        await page
          .locator('button.ss-name', { hasText: 'A very long role' })
          .first()
          .click({ timeout: 6000 })
          .catch(() => undefined);
        await idle(page, 2000);
      },
    });
  }
  if (!ONLY || ONLY.test('chan'))
    await walkTabs(
      page,
      'chan-text',
      `/settings/server/${w.guildId}/channels?channel=${w.channelId}`,
      '.ss-tabs [role="tab"]',
    );

  await ctx.close();
  await ownerCtx.close();
  console.log(`REPORT hit totals ${JSON.stringify(totals)}`);
  writeFileSync(resolve(OUT, 'hit-totals.json'), JSON.stringify(totals, null, 1));
});

// ---------------------------------------------------------------------------------------------------------------
// contrast (item 2)
// ---------------------------------------------------------------------------------------------------------------

test('SEC-AUD-352: a disabled choice row, its text', async ({ browser }) => {
  test.skip(!PARTS.has('contrast'), 'part off');
  mkdirSync(OUT, { recursive: true });
  writeFileSync(resolve(OUT, 'contrast.jsonl'), '');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  for (const theme of ['lounge-light', 'lounge-dark', 'lounge-editorial']) {
    const prefs: Prefs = { theme, zoom: 100 };
    await setPrefs(ownerCtx, prefs);
    const { ctx, page } = await newPage(browser, 'desktop', ownerCtx, prefs);
    for (const re of [/Streamer/i, /^Appearance$/i, /Privacy/i, /Notifications/i]) {
      try {
        await withLimit(String(re), async () => {
          await go(page, '/settings/user/account');
          await page
            .locator(NAV)
            .filter({ hasText: re })
            .first()
            .evaluate((el) => (el as HTMLElement).click());
          await idle(page, 2500);
          const rows = await page.evaluate(disabledRowsInPage);
          const file = `contrast-${slug(String(re))}-${theme}.png`;
          if (rows.length > 0 && /Streamer|Appearance/i.test(String(re)))
            await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
          log('contrast.jsonl', { tab: String(re), theme, rows, shot: rows.length ? file : null });
          console.log(`REPORT contrast ${theme} ${String(re)}: ${rows.length} disabled rows`);
        });
      } catch (e) {
        console.log(`REPORT contrast ${theme} ${String(re)}: ${String(e).slice(0, 120)}`);
      }
    }
    await ctx.close();
  }
  await ownerCtx.close();
});

// ---------------------------------------------------------------------------------------------------------------
// seg (item 3)
// ---------------------------------------------------------------------------------------------------------------

test('SEC-AUD-353: a segmented control, its labels', async ({ browser }) => {
  test.skip(!PARTS.has('seg'), 'part off');
  mkdirSync(OUT, { recursive: true });
  writeFileSync(resolve(OUT, 'seg.jsonl'), '');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  for (const [size, theme] of [
    ['phone', 'lounge-dark'],
    ['desktop', 'lounge-editorial'],
  ] as [Size, string][]) {
    const prefs: Prefs = { theme, zoom: 100 };
    await setPrefs(ownerCtx, prefs);
    const { ctx, page } = await newPage(browser, size, ownerCtx, prefs);
    for (const re of [/Privacy/i, /^Profiles?$/i, /Activity/i]) {
      try {
        await withLimit(String(re), async () => {
          await go(page, '/settings/user/account');
          const btn = page.locator(NAV).filter({ hasText: re }).first();
          const name = ((await btn.textContent()) ?? '').trim();
          await btn.evaluate((el) => (el as HTMLElement).click());
          await idle(page, 2500);
          const groups = await page.evaluate(segmentedInPage);
          const shot = `seg-${slug(name)}-${size === 'phone' ? '375' : '1280'}-${theme}.png`;
          await page.screenshot({ path: resolve(OUT, shot), timeout: 10_000 });
          totals.shots++;
          log('seg.jsonl', { tab: name, size, theme, groups, shot });
          console.log(`REPORT seg ${size} ${name}: ${groups.length} controls`);
        });
      } catch (e) {
        console.log(`REPORT seg ${size} ${String(re)}: ${String(e).slice(0, 120)}`);
      }
    }
    await ctx.close();
  }
  await ownerCtx.close();
});

// ---------------------------------------------------------------------------------------------------------------
// wide: the shot check
// ---------------------------------------------------------------------------------------------------------------

interface View {
  id: string;
  sizes?: Size[];
  themes?: string[];
  /** A section page: set the section's theme explicitly. */
  section?: boolean;
  run: (page: Page, size: Size, w: World) => Promise<void>;
}

const tabAt = (path: string, re: RegExp) => async (page: Page) => {
  await go(page, path);
  await page
    .locator(NAV)
    .filter({ hasText: re })
    .first()
    .evaluate((el) => (el as HTMLElement).click());
  await idle(page, 2500);
};
const lastMsg = (p: Page) =>
  p
    .locator('.lng-msg')
    .filter({ has: p.getByRole('button', { name: /^Open profile of / }) })
    .last();
const listPath = (size: Size, w: World) =>
  size === 'phone' ? `/channels/${w.guildId}` : `/channels/${w.guildId}/${w.channelId}`;

const VIEWS: View[] = [
  { id: 'my-account', run: (p) => tabAt('/settings/user/account', /^My Account$/i)(p) },
  { id: 'notifications', run: (p) => tabAt('/settings/user/account', /^Notifications$/i)(p) },
  { id: 'voice-video', run: (p) => tabAt('/settings/user/account', /^Voice/i)(p) },
  { id: 'streamer-mode', run: (p) => tabAt('/settings/user/account', /^Streamer/i)(p) },
  {
    id: 'server-overview',
    run: (p, _s, w) => tabAt(`/settings/server/${w.guildId}`, /^Overview$/i)(p),
  },
  {
    id: 'role-permissions',
    run: async (p, _s, w) => {
      await go(p, `/settings/server/${w.guildId}/roles`);
      await p.locator('button.ss-name').first().click({ timeout: 6000 });
      await idle(p, 2000);
      await p
        .locator('[role="tab"]', { hasText: /Permissions/i })
        .first()
        .click({ timeout: 6000 });
      await idle(p, 2000);
    },
  },
  {
    id: 'server-members',
    run: (p, _s, w) => tabAt(`/settings/server/${w.guildId}`, /^Members$/i)(p),
  },
  {
    id: 'create-channel',
    run: async (p, s, w) => {
      await go(p, listPath(s, w));
      await hit(p.getByRole('button', { name: /^Create Channel in / }), s, true);
    },
  },
  {
    id: 'invite-people',
    run: async (p, s, w) => {
      await go(p, listPath(s, w));
      await hit(p.locator('.lng-guild-header'), s);
      await hit(p.getByRole('menuitem', { name: /Invite People|Invite to Server/ }), s);
    },
  },
  {
    id: 'lounge-channel',
    run: async (p, s, w) => {
      await go(p, `/channels/${w.guildId}/${w.channelId}`);
      if (s === 'desktop')
        await lastMsg(p)
          .hover()
          .catch(() => undefined);
      await p.waitForTimeout(400);
    },
  },
  {
    id: 'admin-limits',
    run: tabAt('/admin/dashboard', /^Limits/i),
  },
  // The three rooms' front pages in their shipped themes (set explicitly, read back from the page).
  { id: 'library', section: true, themes: ['lounge-dark'], run: (p) => go(p, '/library') },
  { id: 'cinema', section: true, themes: ['lounge-dark'], run: (p) => go(p, '/cinema') },
  { id: 'arcade', section: true, themes: ['lounge-dark'], run: (p) => go(p, '/games') },
];

test('the wide shot check', async ({ browser }) => {
  test.skip(!PARTS.has('wide'), 'part off');
  mkdirSync(OUT, { recursive: true });
  if (!ONLY) writeFileSync(resolve(OUT, 'wide.jsonl'), '');
  const w = world();
  const sizeName = (s: Size) => (s === 'phone' ? '375' : '1280');
  let shots = 0;

  // Signed out: the sign-in page, the join page and the invite page, at both sizes.
  const outPages: [string, string][] = [
    ['signed-out-login', '/login'],
    ['signed-out-join', '/join/abcd1234'],
    ...(w.inviteCode
      ? ([['signed-out-invite', `/invite/${w.inviteCode}`]] as [string, string][])
      : []),
  ];
  for (const size of ['phone', 'desktop'] as Size[]) {
    const { ctx, page } = await newPage(browser, size, null, null);
    for (const [id, path] of outPages) {
      if (ONLY && !ONLY.test(id)) continue;
      try {
        await withLimit(id, async () => {
          await go(page, path);
          const lay = await page.evaluate(layoutInPage);
          const file = `${id}-${sizeName(size)}.png`;
          await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
          shots++;
          log('wide.jsonl', { id, size, theme: 'signed-out', file, ...lay });
          console.log(`REPORT wide ${file}: doc ${lay.doc} themes ${lay.themes.join(',')}`);
        });
      } catch (e) {
        console.log(`REPORT wide ${id} ${size}: ${String(e).slice(0, 120)}`);
      }
    }
    await ctx.close();
  }

  // Signed in.
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const runs: [Size, string][] = [
    ['phone', 'lounge-dark'],
    ['phone', 'lounge-light'],
    ['desktop', 'lounge-dark'],
  ];
  for (const [size, theme] of runs) {
    const prefs: Prefs = {
      theme,
      zoom: 100,
      sectionThemes: { library: 'sideroom', cinema: 'sideroom', arcade: 'lounge-editorial' },
    };
    await setPrefs(ownerCtx, prefs);
    let { ctx, page } = await newPage(browser, size, ownerCtx, prefs);
    for (const v of VIEWS) {
      if (ONLY && !ONLY.test(v.id)) continue;
      if (v.sizes && !v.sizes.includes(size)) continue;
      if (v.themes && !v.themes.includes(theme)) continue;
      try {
        await withLimit(v.id, async () => {
          await v.run(page, size, w);
          await page.waitForTimeout(300);
          const lay = await page.evaluate(layoutInPage);
          const shown = v.section
            ? lay.themes.join('+') || 'no-theme'
            : (lay.themes[0] ?? 'no-theme');
          const file = `${v.id}-${sizeName(size)}-${slug(theme)}.png`;
          await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
          shots++;
          log('wide.jsonl', { id: v.id, size, theme, shown, file, ...lay });
          console.log(`REPORT wide ${file}: doc ${lay.doc} data-theme ${shown}`);
        });
      } catch (e) {
        console.log(`REPORT wide ${v.id} ${size} ${theme}: ${String(e).slice(0, 120)}`);
        await ctx.close().catch(() => undefined);
        ({ ctx, page } = await newPage(browser, size, ownerCtx, prefs));
      }
    }
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
  console.log(`REPORT wide shots ${shots}`);
});
