import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { runCli } from '../support/cli';
import { hitTestAll, openCollapsed } from '../support/hit-test';
import { over, parseColor, ratio } from '../support/contrast';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * Wave 105, solo reader (D-1001; SEC-AUD-486, 487, 488). Measured base and after on the seeded sweep world:
 *  1. the Reader's top and bottom bar at 375, 320, 1024 and 1280: every child's box, computed flex and min-inline-size,
 *     the bar's scrollWidth against clientWidth, a hit test over each bar with the takers named (touch reach at 375 and
 *     320, box reach at 1024 and 1280);
 *  2. the end card on the four stages (`data-bg` black, grey, white, sepia, set through Reader settings as a member
 *     does), under `sideroom` at 1024 and 375 and under `lounge-light` at 1024: colour and ground of every part, the
 *     ratio from `e2e/support/contrast.ts`, and every box of the card;
 *  3. every `.lib-icon-btn` outside the reader (series page, Browse) at 375 and 1024.
 * After phase, when `figures-base.json` is in the folder: every broken "must not change" or "may change" limit of the
 * task is a problem line with figures, and `compare.json` is written.
 *
 * Data it makes through the API under `p105-rd-<digits>` names (D-942), removed in a `finally`: one published series
 * with one edition of three chapters (the second with a long title). The reader's pictures are answered by
 * `page.route`. It never seeds and keeps nothing in `e2e/.state`.
 *
 * Commander's command:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p105-integ/apps/web/dist --env SR_P105_READER_ON=1 --env SR_P105_READER_PHASE=after --env SR_P105_READER_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-reader -- tests/p105-reader-shots.spec.ts
 */
const ON = process.env.SR_P105_READER_ON === '1';
const PHASE = process.env.SR_P105_READER_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P105_READER_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-reader';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const API = '/api/v1';
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PIXEL =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';

type Bg = 'black' | 'grey' | 'white' | 'sepia';
interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  section: 'sideroom' | 'lounge-light';
  size: string;
  /** Stages whose end card is measured. */
  bgs: Bg[];
  barShot: boolean;
  /** Stages whose end card is shot, and the name of the shot. */
  endShots: Bg[];
  endName: (bg: Bg) => string;
  icons: boolean;
}
const ALL: Bg[] = ['black', 'white', 'sepia', 'grey'];
const CONFIGS: Cfg[] = [
  {
    tag: '1024-sideroom',
    width: 1024,
    height: 768,
    touch: false,
    section: 'sideroom',
    size: '1024',
    bgs: ALL,
    barShot: true,
    endShots: ['black', 'white'],
    endName: (bg) => (bg === 'black' ? 'reader-end-default-1024' : `reader-end-${bg}-1024`),
    icons: true,
  },
  {
    tag: '1280-sideroom',
    width: 1280,
    height: 800,
    touch: false,
    section: 'sideroom',
    size: '1280',
    bgs: [],
    barShot: false,
    endShots: [],
    endName: () => '',
    icons: false,
  },
  {
    tag: '375-sideroom',
    width: 375,
    height: 812,
    touch: true,
    section: 'sideroom',
    size: '375',
    bgs: ALL,
    barShot: true,
    endShots: ['sepia', 'white'],
    endName: (bg) => `reader-end-${bg}-375`,
    icons: true,
  },
  {
    tag: '320-sideroom',
    width: 320,
    height: 640,
    touch: true,
    section: 'sideroom',
    size: '320',
    bgs: [],
    barShot: true,
    endShots: [],
    endName: () => '',
    icons: false,
  },
  {
    tag: '1024-light',
    width: 1024,
    height: 768,
    touch: false,
    section: 'lounge-light',
    size: '1024',
    bgs: ['black', 'white'],
    barShot: false,
    endShots: ['black'],
    endName: () => 'reader-end-light-1024',
    icons: false,
  },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

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
  await ctx.route('**/cdn/p105-*.png', (r) =>
    r.fulfill({ status: 200, contentType: 'image/png', body: Buffer.from(PIXEL, 'base64') }),
  );
  await ctx.route(/\/api\/v1\/library\/chapters\/\d+\/pages(\?.*)?$/, async (r) => {
    const res = await r.fetch();
    if (!res.ok()) return r.fulfill({ response: res });
    const body = (await res.json()) as { pages: { url: string }[] };
    body.pages.forEach((p, i) => (p.url = `${ORIGIN}/cdn/p105-p${i}.png`));
    return r.fulfill({ response: res, json: body });
  });
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  return { ctx, page };
}

async function theme(page: Page, c: Cfg, problems: string[]) {
  const dt = await page.evaluate(
    () => document.querySelector('.lib-root')?.getAttribute('data-theme') ?? null,
  );
  say(`${c.tag}: data-theme ${dt}`);
  if (dt !== c.section) problems.push(`${c.tag}: data-theme is ${dt}, wanted ${c.section}`);
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(500);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`), timeout: 10_000 });
}

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

/** The top and bottom bar: every child's box, flex, min-inline-size; the bar's width; the picker's text. In-page. */
async function barFigure(page: Page, vw: number) {
  return page.evaluate((viewport) => {
    const rnd = (n: number) => Math.round(n * 10) / 10;
    const dump = (bar: Element) => {
      const br = bar.getBoundingClientRect();
      return {
        x: rnd(br.left),
        y: rnd(br.top),
        w: rnd(br.width),
        h: rnd(br.height),
        scrollWidth: (bar as HTMLElement).scrollWidth,
        clientWidth: (bar as HTMLElement).clientWidth,
        children: Array.from(bar.children).map((e, i) => {
          const r = e.getBoundingClientRect();
          const cs = getComputedStyle(e);
          return {
            i,
            key: `${e.tagName.toLowerCase()}.${(e.getAttribute('class') ?? '').split(' ')[0]}${e.getAttribute('aria-label') ? ':' + (e.getAttribute('aria-label') ?? '').slice(0, 18) : ''}`,
            iconBtn: e.classList.contains('lib-icon-btn'),
            x: rnd(r.left),
            y: rnd(r.top),
            w: rnd(r.width),
            h: rnd(r.height),
            right: rnd(r.right),
            flex: `${cs.flexGrow} ${cs.flexShrink} ${cs.flexBasis}`,
            minInline: cs.minWidth,
            minBlock: cs.minHeight,
            pastViewport: r.right > viewport + 0.5 || r.left < -0.5,
          };
        }),
      };
    };
    const top = document.querySelector('header.lib-reader-top') as HTMLElement;
    const foot = document.querySelector('footer.lib-reader-bottom') as HTMLElement | null;
    const sel = top.querySelector('select') as HTMLSelectElement | null;
    let pick: Record<string, unknown> | null = null;
    if (sel) {
      const cs = getComputedStyle(sel);
      const canvas = document.createElement('canvas').getContext('2d')!;
      canvas.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
      const sr = sel.getBoundingClientRect();
      const wrap = sel.parentElement as HTMLElement;
      const wr = wrap.getBoundingClientRect();
      pick = {
        x: rnd(sr.left),
        w: rnd(sr.width),
        h: rnd(sr.height),
        textW: rnd(canvas.measureText(sel.selectedOptions[0]?.textContent ?? '').width),
        padInlineStart: cs.paddingInlineStart,
        padInlineEnd: cs.paddingInlineEnd,
        wrapW: rnd(wr.width),
        name: sel.getAttribute('aria-label') ?? sel.labels?.[0]?.textContent?.slice(0, 30) ?? null,
      };
    }
    return {
      top: dump(top),
      bottom: foot ? dump(foot) : null,
      picker: pick,
      page: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      viewport,
    };
  }, vw);
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

/** The end card's parts: colour, own ground stack, box. */
async function endFigure(page: Page) {
  return page.evaluate(() => {
    const rnd = (n: number) => Math.round(n * 10) / 10;
    const card = document.querySelector('section.lib-endcard') as HTMLElement;
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      return { x: rnd(r.left), y: rnd(r.top), w: rnd(r.width), h: rnd(r.height) };
    };
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
    const next = card.querySelector('button.lib-btn-gold');
    const back = card.querySelector('a.lib-btn');
    const sel = card.querySelector('select');
    const ratingLabel = sel?.closest('label')?.querySelector('span:not(.lib-sr-only)') ?? null;
    const stage = card.closest('[data-bg]') as HTMLElement;
    return {
      dataBg: stage.getAttribute('data-bg'),
      card: box(card),
      children: Array.from(card.children).map((e) => ({
        key: `${e.tagName.toLowerCase()}.${(e.getAttribute('class') ?? '').split(' ')[0]}`,
        ...box(e),
      })),
      eyebrow: part(card.querySelector('.lib-eyebrow')),
      next: part(next),
      back: part(back),
      comments: part(comments ?? null),
      ratingLabel: part(ratingLabel),
      select: sel ? part(sel) : null,
      stageBg: getComputedStyle(stage).backgroundColor,
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
const PARTS = ['eyebrow', 'next', 'back', 'comments', 'ratingLabel', 'select'] as const;

/** Set the stage through Reader settings, as a member does, and read `data-bg` back. */
async function setStage(page: Page, ids: Ids, bg: Bg) {
  await page.goto(`/library/r/${ids.chapterId}/1`, { waitUntil: 'domcontentloaded' });
  await page.locator('select').first().waitFor({ state: 'visible', timeout: 30_000 });
  await holdBars(page);
  await page
    .locator('header.lib-reader-top')
    .getByRole('button', { name: 'Reader settings' })
    .click();
  const group = page.getByRole('radiogroup', { name: /background/i });
  await group.getByRole('radio', { name: new RegExp(`^${bg}$`, 'i') }).click();
  const got = await page.locator('.lib-reader').first().getAttribute('data-bg');
  if (got !== bg) throw new Error(`data-bg is ${got}, wanted ${bg}`);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);
}

async function reachEnd(page: Page, ids: Ids) {
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
}

async function runBars(page: Page, c: Cfg, ids: Ids, fig: Fig, problems: string[]) {
  const tag = `${c.tag} bars`;
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
    const bars = await barFigure(page, c.width);
    const hit = await hitSummary(page, page.locator('header.lib-reader-top'), reach);
    const hitBottom = await hitSummary(page, page.locator('footer.lib-reader-bottom'), reach);
    for (const [n, h] of [
      ['top', hit],
      ['bottom', hitBottom],
    ] as const)
      if (h.notReached)
        problems.push(`${tag}: ${n} bar: ${h.notReached} controls not reached (probe error)`);
    fig['bars'] = { ...bars, hit, hitBottom };
    say(
      `${tag}: top h ${bars.top.h} scroll ${bars.top.scrollWidth}/${bars.top.clientWidth} picker ${JSON.stringify(bars.picker)} hit ${hit.pass}/${hit.fail} bottom h ${bars.bottom?.h} hit ${hitBottom.pass}/${hitBottom.fail}`,
    );
    if (c.barShot) {
      await holdBars(page);
      await page.waitForTimeout(300);
      await shot(page, `reader-bars-${c.size}`);
    }
  } catch (e) {
    fig['bars'] = { missing: String(e).slice(0, 300) };
    problems.push(`${tag}: not reached: ${String(e).slice(0, 300)}`);
  }
}

async function runEnd(page: Page, c: Cfg, ids: Ids, fig: Fig, problems: string[]) {
  for (const bg of c.bgs) {
    const tag = `${c.tag} end ${bg}`;
    try {
      if (bg !== 'black') await setStage(page, ids, bg);
      await reachEnd(page, ids);
      await theme(page, c, problems);
      const e = await endFigure(page);
      if (e.dataBg !== bg) problems.push(`${tag}: data-bg reads ${e.dataBg}`);
      const ratios: Record<string, number | null> = {};
      for (const k of PARTS) {
        const p = e[k];
        ratios[k] = !p
          ? null
          : k === 'select'
            ? textRatio(p.color, [e.stageBg, p.bg])
            : textRatio(p.color, p.grounds.length ? p.grounds : [e.stageBg]);
      }
      fig[`end:${bg}`] = { ...e, ratios };
      say(`${tag}: ${JSON.stringify(ratios)}`);
      if (e.page[0]! > e.page[1]!)
        problems.push(`${tag}: page scrolls sideways ${e.page.join('/')}`);
      if (c.endShots.includes(bg)) await shot(page, c.endName(bg));
    } catch (err) {
      fig[`end:${bg}`] = { missing: String(err).slice(0, 300) };
      problems.push(`${tag}: not reached: ${String(err).slice(0, 300)}`);
    }
  }
}

/** Every `.lib-icon-btn` outside the reader: series page and Browse. */
async function runIcons(page: Page, ids: Ids, fig: Fig) {
  for (const [key, path] of [
    ['series', `/library/s/${ids.seriesId}`],
    ['browse', '/library/browse'],
  ] as const) {
    try {
      await page.goto(path, { waitUntil: 'domcontentloaded' });
      await page.locator('main').first().waitFor();
      await page.waitForTimeout(1200);
      fig[`icons:${key}`] = await page.evaluate(() =>
        Array.from(document.querySelectorAll<HTMLElement>('.lib-icon-btn'))
          .filter((e) => e.getClientRects().length > 0)
          .map((e) => {
            const r = e.getBoundingClientRect();
            const rnd = (n: number) => Math.round(n * 10) / 10;
            return {
              key: `${e.tagName.toLowerCase()}:${e.getAttribute('aria-label') ?? ''}`,
              x: rnd(r.left),
              y: rnd(r.top),
              w: rnd(r.width),
              h: rnd(r.height),
            };
          }),
      );
    } catch (e) {
      fig[`icons:${key}`] = { missing: String(e).slice(0, 200) };
    }
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
    await runBars(page, c, ids, fig, problems);
    await runEnd(page, c, ids, fig, problems);
    if (c.icons) await runIcons(page, ids, fig);
  } finally {
    await ctx.close();
  }
  return fig;
}

const near = (a: number, b: number, d = 0.5) => Math.abs(a - b) <= d;
type B4 = { x: number; y: number; w: number; h: number };
const sameBox = (a: B4, b: B4) =>
  near(a.x, b.x) && near(a.y, b.y) && near(a.w, b.w) && near(a.h, b.h);

function compare(c: Cfg, base: Fig, after: Fig, problems: string[]) {
  const out: Record<string, unknown> = {};
  const tag = c.tag;
  const bb = base['bars'];
  const ab = after['bars'];
  if (bb && ab && !bb.missing && !ab.missing) {
    const d: Record<string, unknown> = {};
    for (const which of ['top', 'bottom'] as const) {
      const b = bb[which];
      const a = ab[which];
      if (!b || !a) continue;
      if (!near(b.h, a.h)) problems.push(`${tag}: ${which} bar height ${b.h} -> ${a.h}`);
      if (b.children.length !== a.children.length)
        problems.push(`${tag}: ${which} bar children ${b.children.length} -> ${a.children.length}`);
      b.children.forEach((bc: any, i: number) => {
        const ac = a.children[i];
        if (!ac) return;
        const icon = bc.iconBtn;
        if (icon && !near(bc.w, ac.w))
          problems.push(`${tag}: ${which} ${bc.key} width ${bc.w} -> ${ac.w}`);
        if (c.width >= 1024) {
          for (const k of ['x', 'y', 'w', 'h'] as const)
            if (!near(bc[k], ac[k]))
              problems.push(
                `${tag}: ${which} ${bc.key} ${k} ${bc[k]} -> ${ac[k]} (must not change)`,
              );
        } else if (icon) {
          if (c.width <= 480 && !near(ac.h, 44))
            problems.push(`${tag}: ${which} ${bc.key} height ${ac.h} after, want 44`);
          if (c.width <= 480 && ac.h < 44 - 0.5)
            problems.push(`${tag}: ${which} ${bc.key} under 44 tall`);
        } else if (c.width === 375) {
          // the bar at 375 except the buttons' height: picker and title keep their boxes
          for (const k of ['w', 'h'] as const)
            if (!near(bc[k], ac[k]))
              problems.push(
                `${tag}: ${which} ${bc.key} ${k} ${bc[k]} -> ${ac[k]} (must not change at 375)`,
              );
        }
      });
      d[which] = b.children.map((bc: any, i: number) => ({
        key: bc.key,
        base: [bc.x, bc.y, bc.w, bc.h, bc.flex, bc.minInline],
        after: a.children[i]
          ? [
              a.children[i].x,
              a.children[i].y,
              a.children[i].w,
              a.children[i].h,
              a.children[i].flex,
              a.children[i].minInline,
              a.children[i].minBlock,
            ]
          : null,
      }));
      d[`${which}Scroll`] = {
        base: [b.scrollWidth, b.clientWidth],
        after: [a.scrollWidth, a.clientWidth],
      };
      if (c.width <= 375 && a.scrollWidth > a.clientWidth)
        problems.push(
          `${tag}: ${which} bar scrollWidth ${a.scrollWidth} > clientWidth ${a.clientWidth}`,
        );
      const past = a.children.filter((x: any) => x.pastViewport).map((x: any) => x.key);
      if (c.width <= 375 && past.length)
        problems.push(`${tag}: ${which} controls past the viewport after: ${past.join(', ')}`);
    }
    if (c.width === 375 && ab.picker && !near(ab.picker.w, 115, 1.5))
      problems.push(`${tag}: picker ${ab.picker.w} wide, wave 104 left 115`);
    if (c.width <= 360 && ab.picker && ab.picker.w < 56 - 0.5)
      problems.push(`${tag}: picker ${ab.picker.w} wide, under 56`);
    d['picker'] = { base: bb.picker, after: ab.picker };
    d['hit'] = {
      topBase: [bb.hit.pass, bb.hit.fail],
      topAfter: [ab.hit.pass, ab.hit.fail],
      bottomBase: [bb.hitBottom.pass, bb.hitBottom.fail],
      bottomAfter: [ab.hitBottom.pass, ab.hitBottom.fail],
      takersAfter: [...ab.hit.rows, ...ab.hitBottom.rows]
        .filter((r: any) => r.status === 'fail')
        .map((r: any) => `${r.key}: ${r.takers.join('; ')}`),
    };
    if (c.touch && c.width <= 375) {
      if (ab.hit.fail) problems.push(`${tag}: top bar hit test ${ab.hit.fail} fails after`);
      if (ab.hitBottom.fail)
        problems.push(`${tag}: bottom bar hit test ${ab.hitBottom.fail} fails after`);
    } else if (ab.hit.fail > bb.hit.fail || ab.hitBottom.fail > bb.hitBottom.fail)
      problems.push(`${tag}: a bar hit test is worse than the base`);
    out['bars'] = d;
  }
  for (const bg of c.bgs) {
    const be = base[`end:${bg}`];
    const ae = after[`end:${bg}`];
    if (!be || !ae || be.missing || ae.missing) continue;
    const d: Record<string, unknown> = { ratios: { base: be.ratios, after: ae.ratios } };
    if (!sameBox(be.card, ae.card)) problems.push(`${tag} end ${bg}: card box moved`);
    be.children.forEach((b: any, i: number) => {
      const a = ae.children[i];
      if (a && !sameBox(b, a)) problems.push(`${tag} end ${bg}: child ${b.key} moved`);
    });
    for (const k of PARTS) {
      if (!be[k] || !ae[k]) continue;
      if (!sameBox(be[k].box, ae[k].box)) problems.push(`${tag} end ${bg}: ${k} box moved`);
      const mayChange = c.section === 'sideroom' && (bg === 'white' || bg === 'sepia');
      if (!mayChange && (be[k].color !== ae[k].color || be[k].bg !== ae[k].bg))
        problems.push(
          `${tag} end ${bg}: ${k} colours changed (${be[k].color}/${be[k].bg} -> ${ae[k].color}/${ae[k].bg})`,
        );
      if (mayChange) {
        const v = ae.ratios[k];
        if (v !== null && v < 4.5)
          problems.push(`${tag} end ${bg}: ${k} reads ${v}:1 after, under 4.5`);
      }
    }
    out[`end:${bg}`] = d;
  }
  for (const key of ['icons:series', 'icons:browse']) {
    const bi = base[key];
    const ai = after[key];
    if (!Array.isArray(bi) || !Array.isArray(ai)) continue;
    if (bi.length !== ai.length)
      problems.push(`${tag} ${key}: ${bi.length} -> ${ai.length} buttons`);
    bi.forEach((b: any, i: number) => {
      if (ai[i] && !sameBox(b, ai[i])) problems.push(`${tag} ${key}: ${b.key} box moved`);
    });
    out[key] = { base: bi, after: ai };
  }
  return out;
}

test(`p105 reader: bar targets at 375 and 320, the end card on white and sepia, ${PHASE}`, async ({
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
      'e2e p105 reader shots',
    ]);
  } catch (e) {
    say(`staff appoint: ${String(e).slice(0, 120)}`);
  }
  await call(octx.request, 'POST', '/sr/age-gate', { over_18: true }).catch(() => undefined);

  const stamp = String(Date.now()).slice(-6);
  const problems: string[] = [];
  const figs: Record<string, Fig> = {};
  const ids: Ids = { seriesId: '', chapterId: '' };
  try {
    const series = await call<{ id: string }>(octx.request, 'POST', '/admin/library/series', {
      kind: 'manga',
      title: `p105-rd-${stamp}`,
      synopsis: 'A series made by the wave 105 reader spec.',
      genres: ['Drama'],
      authors: ['E2E Author'],
      reading_direction: 'ltr',
    });
    ids.seriesId = series.id;
    const edition = (
      await call<{ id: string }>(
        octx.request,
        'POST',
        `/admin/library/series/${ids.seriesId}/editions`,
        { language: 'en', label: 'Official', is_default: true },
      )
    ).id;
    const mkChapter = async (number: number, title: string) => {
      const ch = await call<{ id: string }>(
        octx.request,
        'POST',
        `/admin/library/editions/${edition}/chapters`,
        { number, title, format: 'images' },
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
    ids.chapterId = await mkChapter(1, 'p105-rd first');
    await mkChapter(2, 'p105-rd second chapter with a long title that is wider than the picker');
    await mkChapter(3, 'p105-rd third');
    await call(octx.request, 'POST', `/admin/library/series/${ids.seriesId}/publish`);
    say(`ids ${JSON.stringify(ids)}`);

    for (const c of CONFIGS) {
      try {
        figs[c.tag] = await runConfig(browser, octx, c, ids, problems);
      } catch (e) {
        problems.push(`${c.tag}: ${String(e).slice(0, 600)}`);
      }
    }
  } finally {
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
