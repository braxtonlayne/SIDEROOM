import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { runCli } from '../support/cli';
import { hitTestAll, openCollapsed } from '../support/hit-test';
import { over, parseColor, ratio } from '../support/contrast';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * Wave 104, solo reader (D-998; SEC-AUD-472, 473, 474). Three things in the Library, measured base and after:
 *  1. the Reader's top bar at 375 (and 320) is no wider than the viewport and every control on it is whole: each
 *     child's box and computed flex, the bar's scrollWidth against clientWidth, the controls that end past the
 *     viewport, a hit test over the bar, and the chevron's colour against the select's ground and against the bar;
 *  2. the Reader's end card under `lounge-light` takes the bars' dark tokens: computed colour and ground of the
 *     comments button, the rating field and its text, with the ratio from `e2e/support/contrast.ts`, and the nearest
 *     ancestor that carries one of the four elements of the bars' token block;
 *  3. a `label.lib-select`'s text keeps a gap from its field (`labelGap`, as `p103-lounge-shots` measures it).
 * "Must not change" rows: the bar at 1024 (every box), the bar's height and every bar button's size at any width,
 * the bottom bar, the end card under `sideroom`, the end card's boxes under any theme, a `.lib-select` field's
 * height and type, no `.lib-select` row that newly wraps, the Browse toolbar's `.lib-sort`.
 *
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; never
 * edited). Data it makes through the API under `p104-rd-<digits>` names (D-942), removed in a `finally`: one published
 * series with two editions (Official: three chapters, the second with a long title; Fan edition: one chapter, so
 * Edition shows) and one private reading list. The reader's pictures are answered by `page.route`.
 *
 * Phases: `SR_P104_READER_PHASE=base` records the tree as it is (shots get a `base-` prefix, figures go to
 * `figures-base.json`); the default `after` records the same and, when `figures-base.json` is in the folder, compares
 * and writes `compare.json`. Local only (SR_P104_READER_ON=1); SR_P104_READER_OUT is the folder of shots and figures.
 * From e2e/, on a stack with the sweep's world (`SR_ADMIN_REQUIRE_REAUTH=0`, `DATABASE_URL` and `SR_CLI_BIN` set):
 *   SR_E2E_BASE_URL=http://localhost:5804 SR_P104_READER_ON=1 SR_P104_READER_PHASE=base
 *   SR_P104_READER_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p104-reader
 *   npx playwright test tests/p104-reader-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P104_READER_ON === '1';
const PHASE = process.env.SR_P104_READER_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P104_READER_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p104-reader';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const API = '/api/v1';
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PIXEL =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  section: 'sideroom' | 'lounge-light';
  size: string;
  /** Which shots this config takes. */
  shots: string[];
  /** Run the label-gap pages (series, lists, my library, rankings, review, browse). */
  labels: boolean;
}
const CONFIGS: Cfg[] = [
  {
    tag: '1024-sideroom',
    width: 1024,
    height: 768,
    touch: false,
    section: 'sideroom',
    size: '1024',
    shots: ['reader-bars', 'reader-end', 'series-edition'],
    labels: true,
  },
  {
    tag: '375-sideroom',
    width: 375,
    height: 812,
    touch: true,
    section: 'sideroom',
    size: '375',
    shots: ['reader-bars', 'lists-visibility'],
    labels: true,
  },
  {
    tag: '1024-light',
    width: 1024,
    height: 768,
    touch: false,
    section: 'lounge-light',
    size: '1024',
    shots: ['reader-bars', 'reader-end'],
    labels: false,
  },
  {
    tag: '375-light',
    width: 375,
    height: 812,
    touch: true,
    section: 'lounge-light',
    size: '375',
    shots: ['reader-bars', 'reader-end'],
    labels: false,
  },
  {
    tag: '320-sideroom',
    width: 320,
    height: 640,
    touch: true,
    section: 'sideroom',
    size: '320',
    shots: [],
    labels: false,
  },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const r1 = (n: number) => Math.round(n * 10) / 10;

async function call<T = any>(
  request: APIRequestContext,
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE',
  path: string,
  data?: unknown,
): Promise<T> {
  const headers: Record<string, string> = { Origin: ORIGIN };
  if (method !== 'GET') {
    const csrf = await request.get(`${API}/sr/csrf`, { headers: { Origin: ORIGIN } });
    headers['X-SR-CSRF'] = ((await csrf.json()) as { token: string }).token;
  }
  const res = await request.fetch(`${API}${path}`, {
    method,
    headers,
    ...(data !== undefined ? { data } : {}),
  });
  if (!res.ok()) throw new Error(`${method} ${path} -> ${res.status()}: ${await res.text()}`);
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

interface Ids {
  seriesId: string;
  chapterId: string;
  listId: string;
}
interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
type Fig = Record<string, any>;

async function setPrefs(ctx: BrowserContext, section: string) {
  await apiPatch(ctx.request, '/users/@me/settings', {
    settings: {
      user_prefs: { theme: 'lounge-dark', zoom: 100, sectionThemes: { library: section } },
    },
  });
  const back = (await (await ctx.request.get(`${API}/users/@me/settings`)).json()) as {
    settings?: { user_prefs?: { sectionThemes?: Record<string, string> } };
  };
  expect(back.settings?.user_prefs?.sectionThemes?.['library']).toBe(section);
}

async function newPage(browser: Browser, owner: BrowserContext, c: Cfg) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.width, height: c.height },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((sec) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({
          state: { prefs: { theme: 'lounge-dark', zoom: 100, sectionThemes: { library: sec } } },
          version: 2,
        }),
      );
    } catch {
      /* no storage */
    }
  }, c.section);
  await ctx.route('**/cdn/p104-*.png', (r) =>
    r.fulfill({ status: 200, contentType: 'image/png', body: Buffer.from(PIXEL, 'base64') }),
  );
  await ctx.route(/\/api\/v1\/library\/chapters\/\d+\/pages(\?.*)?$/, async (r) => {
    const res = await r.fetch();
    if (!res.ok()) return r.fulfill({ response: res });
    const body = (await res.json()) as { pages: { url: string }[] };
    body.pages.forEach((p, i) => (p.url = `${ORIGIN}/cdn/p104-p${i}.png`));
    return r.fulfill({ response: res, json: body });
  });
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  return { ctx, page };
}

async function theme(page: Page, c: Cfg, problems: string[]) {
  const dt = await page
    .getByRole('main')
    .first()
    .evaluate((e) => e.closest('[data-theme]')?.getAttribute('data-theme') ?? null)
    .catch(() => null);
  const dt2 =
    dt ??
    (await page.evaluate(
      () => document.querySelector('.lib-root')?.getAttribute('data-theme') ?? null,
    ));
  say(`${c.tag}: data-theme ${dt2}`);
  if (dt2 !== c.section) problems.push(`${c.tag}: data-theme is ${dt2}, wanted ${c.section}`);
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(500);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`), timeout: 10_000 });
}
const themeTag = (c: Cfg) => (c.section === 'sideroom' ? 'sideroom' : 'light');

/* ------------------------------------------------------------------------------------------------ the reader */

async function holdBars(page: Page) {
  await page.evaluate(() => {
    const w = window as unknown as { __holdBars?: number };
    if (w.__holdBars) return;
    const poke = () =>
      document
        .querySelector('header[aria-label]')
        ?.dispatchEvent(new MouseEvent('mousemove', { bubbles: true }));
    poke();
    w.__holdBars = window.setInterval(poke, 400);
  });
}

async function barsShown(page: Page) {
  return page.evaluate(() => {
    const h = document.querySelector('header[aria-label]');
    const f = document.querySelector('header[aria-label] ~ footer');
    const ok = (e: Element | null) =>
      !!e && getComputedStyle(e).opacity === '1' && getComputedStyle(e).pointerEvents !== 'none';
    return { top: ok(h), bottom: ok(f) };
  });
}

/** Step 1, item 1 and 4: the top bar's children, its width, the chevron. In-page, one call. */
async function barFigure(page: Page, vw: number) {
  const m = await page.evaluate((viewport) => {
    const rnd = (n: number) => Math.round(n * 10) / 10;
    const bar = document.querySelector('header.lib-reader-top') as HTMLElement;
    const foot = document.querySelector('footer.lib-reader-bottom') as HTMLElement | null;
    const br = bar.getBoundingClientRect();
    const children = Array.from(bar.children).map((e, i) => {
      const r = e.getBoundingClientRect();
      const cs = getComputedStyle(e);
      return {
        i,
        key: `${e.tagName.toLowerCase()}.${(e.getAttribute('class') ?? '').split(' ')[0]}${e.getAttribute('aria-label') ? ':' + (e.getAttribute('aria-label') ?? '').slice(0, 18) : ''}`,
        x: rnd(r.left),
        y: rnd(r.top),
        w: rnd(r.width),
        h: rnd(r.height),
        right: rnd(r.right),
        flex: `${cs.flexGrow} ${cs.flexShrink} ${cs.flexBasis}`,
        minInline: cs.minWidth,
        maxInline: cs.maxWidth,
        pastViewport: r.right > viewport + 0.5,
      };
    });
    const sel = bar.querySelector('select') as HTMLSelectElement | null;
    let chevron: { color: string; ground: string; barGround: string } | null = null;
    let selBox: { x: number; w: number; h: number; textW: number; text: string } | null = null;
    if (sel) {
      const wrap = sel.parentElement as HTMLElement;
      const ac = getComputedStyle(wrap, '::after');
      const cs = getComputedStyle(sel);
      chevron = {
        color: ac.borderRightColor,
        ground: cs.backgroundColor,
        barGround: getComputedStyle(bar).backgroundColor,
      };
      const canvas = document.createElement('canvas').getContext('2d')!;
      canvas.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
      const text = sel.selectedOptions[0]?.textContent ?? '';
      const sr = sel.getBoundingClientRect();
      selBox = {
        x: rnd(sr.left),
        w: rnd(sr.width),
        h: rnd(sr.height),
        textW: rnd(canvas.measureText(text).width),
        text,
      };
    }
    const stage = bar.closest('[data-bg]');
    return {
      bar: {
        x: rnd(br.left),
        w: rnd(br.width),
        h: rnd(br.height),
        scrollWidth: bar.scrollWidth,
        clientWidth: bar.clientWidth,
      },
      children,
      sel: selBox,
      chevron,
      stageBg: stage ? getComputedStyle(stage).backgroundColor : null,
      bottom: foot
        ? (() => {
            const r = foot.getBoundingClientRect();
            return {
              x: rnd(r.left),
              w: rnd(r.width),
              h: rnd(r.height),
              children: Array.from(foot.children).map((e) => {
                const q = e.getBoundingClientRect();
                return { x: rnd(q.left), w: rnd(q.width), h: rnd(q.height) };
              }),
            };
          })()
        : null,
      page: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      viewport,
    };
  }, vw);
  let chevronFig: Fig | null = null;
  if (m.chevron) {
    const ground = parseColor(m.chevron.ground);
    const barG = over(
      parseColor(m.chevron.barGround),
      over(parseColor(m.stageBg ?? 'rgb(0,0,0)'), { r: 0, g: 0, b: 0, a: 1 }),
    );
    chevronFig = {
      color: m.chevron.color,
      ground: m.chevron.ground,
      onSelectGround: r1(ratio(over(parseColor(m.chevron.color), ground), ground)),
      onBar: r1(ratio(over(parseColor(m.chevron.color), barG), barG)),
    };
  }
  return { ...m, chevron: chevronFig };
}

async function hitSummary(page: Page, root: Locator, reach: 'touch' | 'box') {
  const opened = await openCollapsed(page, root);
  const rows = await hitTestAll(page, root, { reach });
  return {
    pass: rows.filter((r) => r.status === 'pass').length,
    fail: rows.filter((r) => r.status === 'fail').length,
    notReached: rows.filter((r) => r.status === 'not-reached').length,
    skipped: rows.filter((r) => r.status === 'skipped').length,
    rows: rows.map((r) => ({
      key: `${r.tag} "${r.name}"`,
      status: r.status,
      reason: r.reason,
      takers: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map(
          (p) =>
            `${p.at}: ${p.taker ? `${p.taker.tag} "${p.taker.name}" ${p.taker.path}` : 'null'}`,
        ),
    })),
    opened: opened.length,
  };
}

/** Step 1, item 2: the end card's colours and boxes. */
async function endFigure(page: Page) {
  return page.evaluate(() => {
    const rnd = (n: number) => Math.round(n * 10) / 10;
    const card = document.querySelector('section.lib-endcard') as HTMLElement;
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      return { x: rnd(r.left), y: rnd(r.top), w: rnd(r.width), h: rnd(r.height) };
    };
    // the first ancestor with a non-transparent background (alpha laid over the ones below is the spec's job)
    const stack = (e: Element) => {
      const out: string[] = [];
      for (let x: Element | null = e; x; x = x.parentElement) {
        const bg = getComputedStyle(x).backgroundColor;
        if (bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent') out.push(bg);
        if (/^rgb\(/.test(bg) || /, 1\)$/.test(bg)) break;
      }
      return out;
    };
    const part = (e: Element | null) =>
      e
        ? {
            text: (e.textContent ?? '').trim().slice(0, 30),
            color: getComputedStyle(e).color,
            bg: getComputedStyle(e).backgroundColor,
            grounds: stack(e),
            box: box(e),
          }
        : null;
    const comments = Array.from(card.querySelectorAll('button')).find((b) =>
      /comment/i.test(b.textContent ?? ''),
    );
    const sel = card.querySelector('select');
    const ratingLabel = sel?.closest('label')?.querySelector('span:not(.lib-sr-only)') ?? null;
    const tokenOwner = (() => {
      const names = ['lib-reader-top', 'lib-reader-bottom', 'lib-sync-notice', 'lib-pdf-bar'];
      for (let x: Element | null = card; x; x = x.parentElement)
        if (names.some((n) => x!.classList.contains(n))) return x.className;
      return null;
    })();
    const eyebrow = card.querySelector('.lib-eyebrow');
    const caught = card.querySelector('p:not(.lib-eyebrow)');
    return {
      card: box(card),
      children: Array.from(card.children).map((e) => ({
        key: `${e.tagName.toLowerCase()}.${(e.getAttribute('class') ?? '').split(' ')[0]}`,
        ...box(e),
      })),
      comments: part(comments ?? null),
      select: sel
        ? {
            ...part(sel)!,
            color: getComputedStyle(sel).color,
            bg: getComputedStyle(sel).backgroundColor,
          }
        : null,
      ratingLabel: part(ratingLabel),
      eyebrow: part(eyebrow),
      caught: part(caught),
      cardColor: getComputedStyle(card).color,
      uiText: getComputedStyle(card).getPropertyValue('--ui-text').trim(),
      uiBgRaised: getComputedStyle(card).getPropertyValue('--ui-bg-raised').trim(),
      tokenOwner,
      stageBg: getComputedStyle(card.closest('[data-bg]') ?? card).backgroundColor,
      page: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
    };
  });
}

function groundOf(grounds: string[], fallback = 'rgb(255,255,255)') {
  let g = parseColor(fallback);
  for (const s of [...grounds].reverse()) g = over(parseColor(s), g);
  return g;
}
function textRatio(color: string, grounds: string[]) {
  const g = groundOf(grounds);
  return r1(ratio(over(parseColor(color), g), g));
}

async function runReader(page: Page, c: Cfg, ids: Ids, fig: Fig, problems: string[]) {
  const tag = `${c.tag} reader`;
  const reach = c.touch ? 'touch' : 'box';
  try {
    await page.goto(`/library/r/${ids.chapterId}/1`, { waitUntil: 'domcontentloaded' });
    await page.locator('select').first().waitFor({ state: 'visible', timeout: 30_000 });
    await holdBars(page);
    await theme(page, c, problems);
    await page.waitForTimeout(700);
    const shown = await barsShown(page);
    if (!shown.top || !shown.bottom)
      problems.push(`${tag}: the bars are not shown (${JSON.stringify(shown)})`);
    const bar = await barFigure(page, c.width);
    const hit = await hitSummary(page, page.locator('header.lib-reader-top'), reach);
    const hitBottom = await hitSummary(page, page.locator('footer.lib-reader-bottom'), reach);
    const after = await barsShown(page);
    if (!after.top || !after.bottom)
      problems.push(`${tag}: the bars hid during the hit test (${JSON.stringify(after)})`);
    for (const [n, h] of [
      ['top', hit],
      ['bottom', hitBottom],
    ] as const)
      if (h.notReached)
        problems.push(`${tag}: ${n} bar: ${h.notReached} controls not reached (probe error)`);
    fig['bar'] = { ...bar, hit, hitBottom };
    say(
      `${tag}: bar ${JSON.stringify(bar.bar)} past ${JSON.stringify(bar.children.filter((x) => x.pastViewport).map((x) => x.key))} hit ${hit.pass}/${hit.fail}/${hit.notReached}/${hit.skipped}`,
    );
    if (c.shots.includes('reader-bars')) {
      await holdBars(page);
      await page.waitForTimeout(300);
      await shot(page, `reader-bars-${c.size}-${themeTag(c)}`);
    }
  } catch (e) {
    fig['bar'] = { missing: String(e).slice(0, 300) };
    problems.push(`${tag}: bar not reached: ${String(e).slice(0, 300)}`);
  }

  // the end card
  try {
    await page.goto(`/library/r/${ids.chapterId}/1`, { waitUntil: 'domcontentloaded' });
    let end = false;
    for (let i = 0; i < 8 && !end; i++) {
      end = await page
        .locator('section.lib-endcard select')
        .first()
        .isVisible()
        .catch(() => false);
      if (!end) {
        for (let k = 0; k < 3; k++) await page.keyboard.press('ArrowRight');
        await page.waitForTimeout(800);
      }
    }
    if (!end) throw new Error('the end card did not show a rating select');
    await page.waitForTimeout(600);
    await theme(page, c, problems);
    const e = await endFigure(page);
    const comments = e.comments;
    const sel = e.select;
    fig['end'] = {
      ...e,
      ratios: {
        comments: comments
          ? textRatio(comments.color, comments.grounds.length ? comments.grounds : [e.stageBg])
          : null,
        selectText: sel ? textRatio(sel.color, [e.stageBg, sel.bg]) : null,
        ratingLabel: e.ratingLabel ? textRatio(e.ratingLabel.color, [e.stageBg]) : null,
        eyebrow: e.eyebrow ? textRatio(e.eyebrow.color, [e.stageBg]) : null,
      },
    };
    say(
      `${tag}: end ${JSON.stringify({ ratios: fig['end'].ratios, owner: e.tokenOwner, uiText: e.uiText })}`,
    );
    if (e.page[0]! > e.page[1]!)
      problems.push(`${tag}: end card page scrolls sideways ${e.page.join('/')}`);
    if (c.shots.includes('reader-end')) await shot(page, `reader-end-${c.size}-${themeTag(c)}`);
  } catch (err) {
    fig['end'] = { missing: String(err).slice(0, 300) };
    problems.push(`${tag}: end card not reached: ${String(err).slice(0, 300)}`);
  }
}

/* ------------------------------------------------------------------------------------------------- the labels */

interface LabelFig {
  text: string;
  gap: number | null;
  lines: number;
  label: Box;
  select: Box;
  fontSize: string;
  next: Box | null;
}

/** Every `label.lib-select` under main: the gap between the label text's right edge and the field's left edge. */
async function labelFigures(page: Page): Promise<Record<string, LabelFig>> {
  return page.evaluate(() => {
    const rnd = (n: number) => Math.round(n * 10) / 10;
    const out: Record<string, LabelFig> = {};
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      return { x: rnd(r.left), y: rnd(r.top), w: rnd(r.width), h: rnd(r.height) };
    };
    const labels = Array.from(document.querySelectorAll<HTMLElement>('label.lib-select')).filter(
      (l) => l.getClientRects().length > 0,
    );
    labels.forEach((label, n) => {
      const sel = label.querySelector('select') as HTMLSelectElement;
      const textSpan = Array.from(label.children).find(
        (c) => c.tagName === 'SPAN' && !c.contains(sel),
      );
      let right = -1;
      let lines = 0;
      if (textSpan && !textSpan.classList.contains('lib-sr-only')) {
        const rg = document.createRange();
        rg.selectNodeContents(textSpan);
        const rects = Array.from(rg.getClientRects()).filter((r) => r.width > 0);
        lines = new Set(rects.map((r) => Math.round(r.top))).size;
        for (const r of rects) right = Math.max(right, r.right);
      }
      const sr = sel.getBoundingClientRect();
      const sib = label.nextElementSibling;
      const key = `${(textSpan?.textContent ?? label.getAttribute('class') ?? '').trim().slice(0, 20)}#${n}`;
      out[key] = {
        text: (textSpan?.textContent ?? '').trim(),
        gap: right >= 0 ? rnd(sr.left - right) : null,
        lines,
        label: box(label),
        select: box(sel),
        fontSize: getComputedStyle(sel).fontSize,
        next: sib ? box(sib) : null,
      };
    });
    return out;
  });
}

async function runLabels(page: Page, c: Cfg, ids: Ids, fig: Fig, problems: string[]) {
  const pages: { key: string; open: () => Promise<void>; shot?: string }[] = [
    {
      key: 'series',
      shot: c.shots.includes('series-edition')
        ? `series-edition-${c.size}-${themeTag(c)}`
        : undefined,
      open: async () => {
        await page.goto(`/library/s/${ids.seriesId}`, { waitUntil: 'domcontentloaded' });
        await page.getByLabel('Edition').and(page.locator('select')).first().waitFor();
      },
    },
    {
      key: 'review',
      open: async () => {
        await page.goto(`/library/s/${ids.seriesId}`, { waitUntil: 'domcontentloaded' });
        await page.getByRole('tab', { name: 'Reviews' }).click();
        await page.getByRole('button', { name: /Write a review|Edit your review/ }).click();
        await page
          .getByLabel('Rating with this review')
          .and(page.locator('select'))
          .first()
          .waitFor();
      },
    },
    {
      key: 'rankings',
      open: async () => {
        await page.goto('/library/rankings', { waitUntil: 'domcontentloaded' });
        await page.getByLabel('Kind').and(page.locator('select')).first().waitFor();
      },
    },
    {
      key: 'mylibrary',
      open: async () => {
        await page.goto('/library/me', { waitUntil: 'domcontentloaded' });
        await page.getByLabel('Sort by').and(page.locator('select')).first().waitFor();
      },
    },
    {
      key: 'lists-new',
      shot: c.shots.includes('lists-visibility')
        ? `lists-visibility-${c.size}-${themeTag(c)}`
        : undefined,
      open: async () => {
        await page.goto('/library/lists', { waitUntil: 'domcontentloaded' });
        await page.getByRole('button', { name: 'New list' }).first().click();
        await page.getByLabel('Who can see it').and(page.locator('select')).first().waitFor();
      },
    },
    {
      key: 'lists-edit',
      open: async () => {
        await page.goto(`/library/lists/${ids.listId}`, { waitUntil: 'domcontentloaded' });
        await page.getByRole('button', { name: 'Edit list' }).first().click();
        await page.getByLabel('Who can see it').and(page.locator('select')).first().waitFor();
      },
    },
  ];
  for (const p of pages) {
    try {
      await p.open();
      await page.waitForLoadState('networkidle', { timeout: 5000 }).catch(() => undefined);
      await page.waitForTimeout(600);
      const labels = await labelFigures(page);
      const sc = await page.evaluate(() => [
        document.documentElement.scrollWidth,
        document.documentElement.clientWidth,
      ]);
      fig[`labels:${p.key}`] = { labels, page: sc };
      say(
        `${c.tag} ${p.key}: ${JSON.stringify(Object.fromEntries(Object.entries(labels).map(([k, v]) => [k, v.gap])))}`,
      );
      if (sc[0]! > sc[1]!)
        problems.push(`${c.tag} ${p.key}: the page scrolls sideways ${sc.join('/')}`);
      if (p.shot) {
        await page.locator('label.lib-select').first().scrollIntoViewIfNeeded();
        await shot(page, p.shot);
      }
    } catch (e) {
      fig[`labels:${p.key}`] = { missing: String(e).slice(0, 200) };
      problems.push(`${c.tag} ${p.key}: not reached: ${String(e).slice(0, 200)}`);
    }
  }
  // the Browse toolbar's `.lib-sort` is not `.lib-select`: it must not have moved
  try {
    await page.goto('/library/browse', { waitUntil: 'domcontentloaded' });
    await page.getByLabel('Sort by').and(page.locator('select')).first().waitFor();
    await page.waitForTimeout(800);
    fig['browse-sort'] = await page.evaluate(() => {
      const rnd = (n: number) => Math.round(n * 10) / 10;
      const s = document.querySelector('.lib-sort') as HTMLElement | null;
      const sel = s?.querySelector('select') ?? null;
      const b = (e: Element | null) => {
        if (!e) return null;
        const r = e.getBoundingClientRect();
        return { x: rnd(r.left), y: rnd(r.top), w: rnd(r.width), h: rnd(r.height) };
      };
      return { sort: b(s), select: b(sel) };
    });
  } catch (e) {
    fig['browse-sort'] = { missing: String(e).slice(0, 200) };
  }
}

/* ------------------------------------------------------------------------------------------------- the test */

async function runConfig(
  browser: Browser,
  owner: BrowserContext,
  c: Cfg,
  ids: Ids,
  problems: string[],
) {
  await setPrefs(owner, c.section);
  const { ctx, page } = await newPage(browser, owner, c);
  const fig: Fig = {};
  try {
    await runReader(page, c, ids, fig, problems);
    if (c.labels) await runLabels(page, c, ids, fig, problems);
  } finally {
    await ctx.close();
  }
  return fig;
}

const near = (a: number, b: number, d = 0.5) => Math.abs(a - b) <= d;

function compare(c: Cfg, base: Fig, after: Fig, problems: string[]) {
  const out: Record<string, unknown> = {};
  const tag = c.tag;
  const ba = base['bar'];
  const aa = after['bar'];
  if (ba && aa && !ba.missing && !aa.missing) {
    const d: Record<string, unknown> = {};
    d['barHeight'] = { base: ba.bar.h, after: aa.bar.h };
    if (!near(ba.bar.h, aa.bar.h)) problems.push(`${tag}: bar height ${ba.bar.h} -> ${aa.bar.h}`);
    ba.children.forEach((b: any, i: number) => {
      const a = aa.children[i];
      if (!a) return;
      // every bar button's size: all but the title (i = 1) and the picker (i = 2)
      if (b.key.startsWith('button') || b.key.startsWith('a')) {
        if (!near(a.w, b.w) || !near(a.h, b.h))
          problems.push(`${tag}: ${b.key} size ${b.w}x${b.h} -> ${a.w}x${a.h}`);
      }
      if (!c.touch && c.width >= 1024) {
        for (const k of ['x', 'y', 'w', 'h'] as const)
          if (!near(a[k], b[k]))
            problems.push(
              `${tag}: bar child ${b.key} ${k} ${b[k]} -> ${a[k]} (must not change at ${c.width})`,
            );
      }
    });
    d['children'] = ba.children.map((b: any, i: number) => ({
      key: b.key,
      base: [b.x, b.w, b.flex],
      after: [aa.children[i]?.x, aa.children[i]?.w, aa.children[i]?.flex],
    }));
    if (!c.touch && aa.sel && aa.sel.w > 220.5)
      problems.push(`${tag}: picker ${aa.sel.w} wide, over the 220 cap`);
    if (c.touch) {
      const past = aa.children.filter((x: any) => x.pastViewport).map((x: any) => x.key);
      d['past'] = {
        base: ba.children.filter((x: any) => x.pastViewport).map((x: any) => x.key),
        after: past,
      };
      if (past.length)
        problems.push(`${tag}: controls end past the viewport after: ${past.join(', ')}`);
      if (aa.bar.scrollWidth > aa.bar.clientWidth)
        problems.push(
          `${tag}: bar scrollWidth ${aa.bar.scrollWidth} > clientWidth ${aa.bar.clientWidth}`,
        );
      if (aa.sel && aa.sel.w < 96 - 0.5) problems.push(`${tag}: picker ${aa.sel.w} wide, under 96`);
      if (aa.hit.fail)
        problems.push(
          `${tag}: top bar hit test ${aa.hit.fail} fails: ${JSON.stringify(aa.hit.rows.filter((r: any) => r.status === 'fail'))}`,
        );
    }
    if (aa.hit.fail > ba.hit.fail || aa.hit.pass < ba.hit.pass)
      problems.push(
        `${tag}: top bar hit ${ba.hit.pass}/${ba.hit.fail} -> ${aa.hit.pass}/${aa.hit.fail}`,
      );
    if (ba.bottom && aa.bottom) {
      for (const k of ['x', 'w', 'h'] as const)
        if (!near(ba.bottom[k], aa.bottom[k]))
          problems.push(`${tag}: bottom bar ${k} ${ba.bottom[k]} -> ${aa.bottom[k]}`);
      if (aa.hitBottom.fail > ba.hitBottom.fail) problems.push(`${tag}: bottom bar hit worse`);
    }
    d['chevron'] = { base: ba.chevron, after: aa.chevron };
    out['bar'] = d;
  }
  const be = base['end'];
  const ae = after['end'];
  if (be && ae && !be.missing && !ae.missing) {
    const d: Record<string, unknown> = { ratios: { base: be.ratios, after: ae.ratios } };
    for (const k of ['x', 'y', 'w', 'h'] as const)
      if (!near(be.card[k], ae.card[k]))
        problems.push(`${tag}: end card ${k} ${be.card[k]} -> ${ae.card[k]}`);
    be.children.forEach((b: any, i: number) => {
      const a = ae.children[i];
      if (!a) return;
      for (const k of ['x', 'y', 'w', 'h'] as const)
        if (!near(a[k], b[k]))
          problems.push(`${tag}: end card child ${b.key} ${k} ${b[k]} -> ${a[k]}`);
    });
    if (c.section === 'sideroom') {
      for (const k of ['comments', 'select', 'ratingLabel', 'eyebrow'] as const)
        if (be[k] && ae[k] && (be[k].color !== ae[k].color || be[k].bg !== ae[k].bg))
          problems.push(
            `${tag}: end card ${k} colours changed under sideroom (${be[k].color}/${be[k].bg} -> ${ae[k].color}/${ae[k].bg})`,
          );
    } else {
      for (const k of ['comments', 'selectText', 'ratingLabel', 'eyebrow'] as const) {
        const v = ae.ratios[k];
        if (v !== null && v < 4.5)
          problems.push(`${tag}: end card ${k} reads ${v}:1 after, under 4.5`);
      }
    }
    out['end'] = d;
  }
  for (const key of Object.keys(after).filter((k) => k.startsWith('labels:'))) {
    const a = after[key];
    const b = base[key];
    if (!a || !b || a.missing || b.missing) continue;
    const d: Record<string, unknown> = {};
    for (const [lk, av] of Object.entries<LabelFig>(a.labels)) {
      const bv = (b.labels as Record<string, LabelFig>)[lk];
      if (!bv) {
        problems.push(`${tag} ${key} ${lk}: not in the base`);
        continue;
      }
      d[lk] = {
        gap: [bv.gap, av.gap],
        lines: [bv.lines, av.lines],
        label: [bv.label, av.label],
        select: [bv.select, av.select],
        next: [bv.next, av.next],
      };
      if (!near(av.select.h, bv.select.h))
        problems.push(`${tag} ${key} ${lk}: field height ${bv.select.h} -> ${av.select.h}`);
      if (av.fontSize !== bv.fontSize)
        problems.push(`${tag} ${key} ${lk}: font ${bv.fontSize} -> ${av.fontSize}`);
      if (av.lines > bv.lines)
        problems.push(`${tag} ${key} ${lk}: label text wraps (${bv.lines} -> ${av.lines} lines)`);
      if (av.label.h > bv.label.h + 0.5)
        problems.push(
          `${tag} ${key} ${lk}: the row is taller (${bv.label.h} -> ${av.label.h}): it wrapped`,
        );
      if (av.gap !== null && bv.gap !== null && av.gap < 6 - 0.01)
        problems.push(`${tag} ${key} ${lk}: gap ${av.gap} after (base ${bv.gap}), under 6`);
    }
    out[key] = d;
  }
  const bs = base['browse-sort'];
  const as = after['browse-sort'];
  if (bs?.sort && as?.sort)
    for (const k of ['x', 'y', 'w', 'h'] as const)
      if (!near(bs.sort[k], as.sort[k]))
        problems.push(`${tag}: Browse .lib-sort ${k} ${bs.sort[k]} -> ${as.sort[k]}`);
  return out;
}

test(`p104 reader: the top bar at 375, the end card under a light theme, the label gap, ${PHASE}`, async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true);
  try {
    runCli([
      'staff',
      'appoint',
      owner.username,
      '--role',
      'Senior Staff',
      '--reason',
      'e2e p104 reader shots',
    ]);
  } catch (e) {
    say(`staff appoint: ${String(e).slice(0, 120)}`);
  }
  await call(octx.request, 'POST', '/sr/age-gate', { over_18: true }).catch(() => undefined);

  const stamp = String(Date.now()).slice(-6);
  const problems: string[] = [];
  const figs: Record<string, Fig> = {};
  const ids: Ids = { seriesId: '', chapterId: '', listId: '' };
  try {
    const series = await call<{ id: string }>(octx.request, 'POST', '/admin/library/series', {
      kind: 'manga',
      title: `p104-rd-${stamp}`,
      synopsis: 'A series made by the wave 104 reader spec.',
      genres: ['Drama'],
      authors: ['E2E Author'],
      reading_direction: 'ltr',
    });
    ids.seriesId = series.id;
    const mkEdition = async (language: string, label: string, isDefault: boolean) =>
      (
        await call<{ id: string }>(
          octx.request,
          'POST',
          `/admin/library/series/${ids.seriesId}/editions`,
          {
            language,
            label,
            is_default: isDefault,
          },
        )
      ).id;
    const edition = await mkEdition('en', 'Official', true);
    const edition2 = await mkEdition('fr', 'Fan edition', false);
    const mkChapter = async (editionId: string, number: number, title: string) => {
      const ch = await call<{ id: string }>(
        octx.request,
        'POST',
        `/admin/library/editions/${editionId}/chapters`,
        {
          number,
          title,
          format: 'images',
        },
      );
      await call(
        octx.request,
        'PUT',
        `/admin/library/chapters/${ch.id}/pages`,
        [0, 1, 2].map((i) => ({
          key: `library/${ids.seriesId}/${ch.id}/00${i + 1}.webp`,
          width: 200,
          height: 300,
          variants: {},
        })),
      );
      await call(octx.request, 'POST', `/admin/library/chapters/${ch.id}/publish`);
      return ch.id;
    };
    ids.chapterId = await mkChapter(edition, 1, 'p104-rd first');
    await mkChapter(
      edition,
      2,
      'p104-rd second chapter with a long title that is wider than the picker',
    );
    await mkChapter(edition, 3, 'p104-rd third');
    await mkChapter(edition2, 1, 'p104-rd fan');
    await call(octx.request, 'POST', `/admin/library/series/${ids.seriesId}/publish`);
    const list = await call<{ id: string }>(octx.request, 'POST', '/library/lists', {
      name: `p104-rd-list-${stamp}`,
      visibility: 'private',
    });
    ids.listId = list.id;
    say(`ids ${JSON.stringify(ids)}`);

    for (const c of CONFIGS) {
      try {
        figs[c.tag] = await runConfig(browser, octx, c, ids, problems);
      } catch (e) {
        problems.push(`${c.tag}: ${String(e).slice(0, 600)}`);
      }
    }
  } finally {
    if (ids.listId)
      await call(octx.request, 'DELETE', `/library/lists/${ids.listId}`).catch((e) =>
        say(`list delete: ${String(e).slice(0, 160)}`),
      );
    if (ids.seriesId) {
      await call(octx.request, 'POST', `/admin/library/series/${ids.seriesId}/unpublish`).catch(
        () => undefined,
      );
      await call(octx.request, 'DELETE', `/admin/library/series/${ids.seriesId}`).catch((e) =>
        say(`series delete: ${String(e).slice(0, 160)}`),
      );
    }
    await octx.close();
  }
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Fig>;
    const cmp: Record<string, unknown> = {};
    for (const c of CONFIGS)
      if (base[c.tag] && figs[c.tag]) cmp[c.tag] = compare(c, base[c.tag]!, figs[c.tag]!, problems);
    writeFileSync(resolve(OUT, 'compare.json'), JSON.stringify(cmp, null, 1));
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  say(`problems: ${JSON.stringify(problems)}`);
  if (PHASE === 'after') expect(problems).toEqual([]);
});
