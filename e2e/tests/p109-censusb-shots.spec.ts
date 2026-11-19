import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import {
  CONTROL_SELECTOR,
  MENU_ITEM_SELECTOR,
  hitTestAll,
  openCollapsed,
  type ControlRow,
} from '../support/hit-test';

/**
 * Wave 109, census censusb (MEASURE ONLY; SEC-AUD-537 pages 7 and 9, SEC-AUD-535, SEC-AUD-530's Arcade and Study bars).
 * No app file is changed; every run is `shoot base`.
 *
 * Parts (SR_P109_CENSUSB_PART):
 *  1  The Library list (`/library/lists/<listId>`, `/library/lists`) and the Arcade's and Study's own bars
 *     (`.arc-roombar-link` on `/games`, `.stu-bar-link` on `/study`).
 *  2  Admin > Accounts, Limits & settings, Audit log; the `.ss-select` / select / combobox controls there read against
 *     the field contract (D-1024); and Server settings > Members, the one place `.ss-select` is used in the app.
 *  3  Tools with a `.ud-btn` (part A): Hashes, Regex tester, URL encode, Dice, Coin flip, Wheel.
 *  4  Tools with a `.ud-btn` (part B): Loot tables, Private notes, Pomodoro, Stopwatch, Countdown, Programmer calculator.
 *
 * Every page: 375 x 812 touch (the default 44 px reach), once at 1024 x 768 with `reach: 'box'`. `openCollapsed` then
 * `hitTestAll` over the page's root; at 375 up to three one-tap overlays (menus, selects, one Create/New/Add/Edit button)
 * are opened and tested with their items. Nothing is submitted. A section page is shown in its shipped theme, read back
 * (`data-theme`). For each tool at 375 the layout of every `.ud-btn` row is measured with the real CSS and again with the
 * phone rule that grows `.ud-btn` taken off (an injected style, nothing else changed): the "before" twin of this tree.
 * Rows are written whole to `figures-base-p<part>.json`; one viewport shot per page and width (`base-` prefix).
 *
 * It never seeds and makes nothing; it reads `e2e/.state/sweep/world.json` (extras.listId). It imports no /src module.
 * Local only (SR_P109_CENSUSB_ON=1). The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p109-integ/apps/web/dist --env SR_P109_CENSUSB_ON=1 --env SR_P109_CENSUSB_PHASE=after --env SR_P109_CENSUSB_PART=1 --env SR_P109_CENSUSB_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-censusb -- tests/p109-censusb-shots.spec.ts
 */
const ON = process.env.SR_P109_CENSUSB_ON === '1';
const PHASE = process.env.SR_P109_CENSUSB_PHASE === 'after' ? 'after' : 'base';
const PART = Number(process.env.SR_P109_CENSUSB_PART ?? '1');
const OUT =
  process.env.SR_P109_CENSUSB_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-censusb';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const BUDGET_MS = 330_000;
const SECTIONS = {
  library: 'sideroom',
  cinema: 'sideroom',
  'live-tv': 'sideroom-broadcast',
  arcade: 'lounge-editorial',
};

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const r1 = (n: number) => Math.round(n * 10) / 10;
const msg = (e: unknown) => String(e).replace(/\s+/g, ' ').slice(0, 220);
interface World {
  guildId: string;
  channelId: string;
  extras?: { listId?: string; listName?: string };
}
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

interface Cfg {
  tag: string;
  w: number;
  h: number;
  touch: boolean;
}
const C375: Cfg = { tag: '375', w: 375, h: 812, touch: true };
const C1024: Cfg = { tag: '1024', w: 1024, h: 768, touch: false };
const CFGS = [C375, C1024];

// ---- figures --------------------------------------------------------------------------------------------------------

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface RowFig {
  page: string;
  cfg: string;
  state: string;
  index: number;
  tag: string;
  role: string | null;
  type: string;
  cls: string;
  name: string;
  status: string;
  reason: string | null;
  edge: string | null;
  box: Box | null;
  offset: number | null;
  taker: { tag: string; role: string | null; name: string; path: string; at: string } | null;
  path: string;
}
interface SelectFig {
  page: string;
  cfg: string;
  tag: string;
  cls: string;
  name: string;
  ssSelect: boolean;
  box: Box;
  restBorder: string;
  hoverBorder: string | null;
  borderWidth: string;
  focusOutline: string;
  outlineColor: string;
  outlineOffset: string;
  borderToken: string;
  faintToken: string;
  focusToken: string;
  verdict: { rest: boolean; hover: boolean; focus: boolean; height44: boolean | null };
}
interface LayoutFig {
  tool: string;
  state: string;
  cfg: string;
  parentCls: string;
  parentBox: Box;
  parentBefore: Box | null;
  kids: { tag: string; cls: string; name: string; box: Box; before: Box | null }[];
  lines: number;
  linesBefore: number | null;
  flags: string[];
}
const rowsOut: RowFig[] = [];
const selectsOut: SelectFig[] = [];
const layoutOut: LayoutFig[] = [];
const notes: string[] = [];
const notCovered: string[] = [];
const sideways: Record<string, number[]> = {};
const themes: Record<string, (string | null)[]> = {};
const extraFigs: Record<string, unknown> = {};
const started = Date.now();

const flush = () => {
  mkdirSync(OUT, { recursive: true });
  writeFileSync(
    resolve(OUT, `figures-${PHASE}-p${PART}.json`),
    JSON.stringify(
      {
        rows: rowsOut,
        selects: selectsOut,
        layouts: layoutOut,
        notes,
        notCovered,
        sideways,
        themes,
        extras: extraFigs,
      },
      null,
      1,
    ),
  );
};

// ---- helpers --------------------------------------------------------------------------------------------------------

async function newPage(browser: Browser, owner: BrowserContext, c: Cfg) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.w, height: c.h },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript(
    (p) => {
      try {
        localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
      } catch {
        /* no storage */
      }
    },
    { theme: 'lounge-light', zoom: 100, sectionThemes: SECTIONS },
  );
  const page = await ctx.newPage();
  page.setDefaultTimeout(15_000);
  page.on('pageerror', (e) => notes.push(`pageerror: ${msg(e)}`));
  return { ctx, page };
}

async function settle(page: Page, ms = 5000) {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page
    .waitForFunction(() => document.querySelectorAll('[aria-busy="true"]').length === 0, null, {
      timeout: 6000,
    })
    .catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(600);
}

const shotName = (n: string, c: Cfg) => `${PHASE === 'base' ? 'base-' : ''}${n}-${c.tag}.png`;

async function shot(page: Page, name: string, c: Cfg) {
  mkdirSync(OUT, { recursive: true });
  await page.evaluate(() => window.scrollTo(0, 0)).catch(() => undefined);
  await page.mouse.move(2, 2).catch(() => undefined);
  const s = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  sideways[`${name}-${c.tag}`] = s;
  if (s[0]! > s[1]!) notes.push(`${name} ${c.tag}: the page scrolls sideways (${s[0]} > ${s[1]})`);
  themes[`${name}-${c.tag}`] = await page.evaluate(() => [
    document.documentElement.getAttribute('data-theme'),
    document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
  ]);
  await page.screenshot({ path: resolve(OUT, shotName(name, c)), timeout: 15_000 });
}

const bx = (b: { x: number; y: number; width: number; height: number }): Box => ({
  x: r1(b.x),
  y: r1(b.y),
  w: r1(b.width),
  h: r1(b.height),
});

/** One root: openCollapsed, hitTestAll (touch reach at 375, 'box' at 1024); keeps every row; `keep` filters rows. */
async function testRoot(
  page: Page,
  root: Locator,
  pageName: string,
  state: string,
  c: Cfg,
  also?: string,
  keep?: (index: number) => Promise<boolean>,
) {
  await openCollapsed(page, root);
  const sel = also ? `${CONTROL_SELECTOR},${also}` : CONTROL_SELECTOR;
  const rows: ControlRow[] = await hitTestAll(page, root, {
    ...(c.touch ? {} : { reach: 'box' as const }),
    ...(also ? { also } : {}),
  });
  const meta = await root.locator(sel).evaluateAll((els) =>
    els.map((e) => ({
      cls: (e.getAttribute('class') ?? '').trim().slice(0, 90),
      type: e.getAttribute('type') ?? '',
    })),
  );
  let pass = 0;
  let fail = 0;
  let nr = 0;
  let skipped = 0;
  for (const r of rows) {
    if (keep && !(await keep(r.index))) continue;
    if (r.status === 'pass') pass++;
    else if (r.status === 'fail') fail++;
    else if (r.status === 'not-reached') nr++;
    else skipped++;
    const bad = r.result?.points.find((p) => !p.ok);
    const b = r.result?.box ?? null;
    const row: RowFig = {
      page: pageName,
      cfg: c.tag,
      state,
      index: r.index,
      tag: r.tag,
      role: r.role,
      type: meta[r.index]?.type ?? r.type ?? '',
      cls: meta[r.index]?.cls ?? '',
      name: r.name,
      status: r.status,
      reason: r.reason ?? null,
      edge: r.edge,
      box: b ? bx(b) : null,
      offset: r.result?.offset ?? null,
      taker: bad?.taker
        ? {
            tag: bad.taker.tag,
            role: bad.taker.role,
            name: bad.taker.name,
            path: bad.taker.path,
            at: bad.at,
          }
        : null,
      path: r.path,
    };
    rowsOut.push(row);
    if (r.status === 'fail' || r.status === 'not-reached')
      say(
        `  ${r.status.toUpperCase()} ${pageName} ${c.tag} [${state}] ${r.tag}${r.role ? `[${r.role}]` : ''} "${r.name}" {${row.cls}} ${
          row.box
            ? `${Math.round(row.box.w)}x${Math.round(row.box.h)} at y${Math.round(row.box.y)}`
            : '-'
        } edge ${row.edge} ${row.taker ? `taker ${row.taker.at}: ${row.taker.tag}${row.taker.role ? `[${row.taker.role}]` : ''} "${row.taker.name}" ${row.taker.path}` : (row.reason ?? '')}`,
      );
  }
  say(
    `${pageName} ${c.tag} [${state}]: ${pass + fail + nr + skipped} rows, pass ${pass}, fail ${fail}, not-reached ${nr}, skipped ${skipped}`,
  );
  if (nr) notes.push(`${pageName} ${c.tag} [${state}]: ${nr} rows not reached (a probe error)`);
}

const DANGER =
  /create role|delete|remove|ban|kick|suspend|revoke|reset|purge|prune|leave|disconnect|sign out|log out|erase|wipe|start over|lock/i;
const OVERLAY_ALSO = `${MENU_ITEM_SELECTOR},[role=option]`;
const overlayOf = (page: Page) =>
  page
    .locator('[role=menu],[role=listbox],[role=dialog]')
    .filter({ hasNot: page.locator('main') })
    .filter({ visible: true });

/** Up to three things one tap opens inside `root` (popups, comboboxes, one Create/New/Add/Edit button). 375 only. */
async function overlays(page: Page, root: Locator, pageName: string, url: string, c: Cfg) {
  const cands = root.locator(
    'button[aria-haspopup]:not([aria-haspopup="false"]),[role=combobox],button',
  );
  const n = Math.min(await cands.count(), 80);
  const picked: { loc: Locator; name: string }[] = [];
  const seen = new Set<string>();
  let dialogButtons = 0;
  for (let i = 0; i < n && picked.length < 3; i++) {
    const loc = cands.nth(i);
    const info = await loc
      .evaluate((el) => {
        const popup = el.getAttribute('aria-haspopup');
        const txt = (el.getAttribute('aria-label') ?? el.textContent ?? '')
          .trim()
          .replace(/\s+/g, ' ')
          .slice(0, 40);
        const r = el.getBoundingClientRect();
        return {
          popup: popup !== null && popup !== 'false',
          combo: el.getAttribute('role') === 'combobox',
          name: txt,
          shown: r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== 'hidden',
          disabled: (el as HTMLButtonElement).disabled === true,
          expanded: el.getAttribute('aria-expanded'),
        };
      })
      .catch(() => null);
    if (!info || !info.shown || info.disabled || info.expanded === 'true') continue;
    if (DANGER.test(info.name)) continue;
    const isPopup = info.popup || info.combo;
    const isDialogBtn = !isPopup && /^(create|new|add|invite|edit|rename)\b/i.test(info.name);
    if (!isPopup && !(isDialogBtn && dialogButtons === 0)) continue;
    if (isDialogBtn) dialogButtons++;
    if (seen.has(info.name)) continue;
    seen.add(info.name);
    picked.push({ loc, name: info.name });
  }
  if (picked.length === 0) notes.push(`${pageName}: nothing to open with one tap`);
  for (const p of picked) {
    try {
      const before = await overlayOf(page).count();
      await p.loc.scrollIntoViewIfNeeded({ timeout: 5000 });
      await root.locator(CONTROL_SELECTOR).evaluateAll((els) => {
        for (const e of els) e.setAttribute('data-p109-seen', '1');
      });
      await p.loc.evaluate((el) => (el as HTMLElement).click());
      await page.waitForTimeout(600);
      const ov = overlayOf(page);
      if ((await ov.count()) <= before) {
        notes.push(`${pageName}: "${p.name}" opened no overlay (new controls tested in the page)`);
        await testRoot(
          page,
          root,
          pageName,
          `after tap: ${p.name} (new controls only)`,
          c,
          undefined,
          async (i) =>
            (await root.locator(CONTROL_SELECTOR).nth(i).getAttribute('data-p109-seen')) === null,
        );
        continue;
      }
      await testRoot(page, ov.last(), pageName, `overlay: ${p.name}`, c, OVERLAY_ALSO);
      await page.keyboard.press('Escape');
      await page.waitForTimeout(400);
      if (
        (await overlayOf(page).count()) > before ||
        !page.url().includes(new URL(url, ORIGIN).pathname)
      ) {
        await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
        await settle(page);
      }
    } catch (e) {
      notes.push(`${pageName}: overlay "${p.name}" failed: ${msg(e)}`);
      await page
        .goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 })
        .catch(() => undefined);
      await settle(page);
    }
  }
}

async function visit(page: Page, url: string, ready?: (p: Page) => Promise<unknown>) {
  const res = await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 45_000 });
  if ((res?.status() ?? 200) >= 400) notes.push(`${url}: HTTP ${res?.status()}`);
  await page
    .getByRole('main')
    .first()
    .waitFor({ timeout: 30_000 })
    .catch(() => notes.push(`${url}: no <main>`));
  if (ready) await ready(page).catch((e) => notes.push(`${url}: ready failed: ${msg(e)}`));
  await settle(page);
}

// ---- the field contract: selects ------------------------------------------------------------------------------------

const SELECT_SEL = 'select, [role=combobox], .ss-select';

async function readSelects(page: Page, root: Locator, pageName: string, c: Cfg) {
  const all = root.locator(SELECT_SEL);
  const n = Math.min(await all.count(), 30);
  const read = (loc: Locator) =>
    loc.evaluate((f) => {
      const cv = document.createElement('canvas');
      cv.width = cv.height = 1;
      const g = cv.getContext('2d', { willReadFrequently: true })!;
      const norm = (css: string): string => {
        g.clearRect(0, 0, 1, 1);
        g.fillStyle = '#000';
        g.fillStyle = css;
        g.fillRect(0, 0, 1, 1);
        const d = g.getImageData(0, 0, 1, 1).data;
        return `rgb(${d[0]}, ${d[1]}, ${d[2]})`;
      };
      const cs = getComputedStyle(f);
      const probe = document.createElement('span');
      document.body.appendChild(probe);
      const tok = (k: string) => {
        probe.style.color = '';
        probe.style.color = cs.getPropertyValue(k).trim() || 'transparent';
        return norm(getComputedStyle(probe).color);
      };
      const faint = tok('--ui-text-faint');
      const focus = tok('--ui-focus');
      const border = tok('--ui-border');
      probe.remove();
      const r = f.getBoundingClientRect();
      const labels = (f as HTMLInputElement).labels;
      const name =
        f.getAttribute('aria-label') ||
        (labels && labels[0]?.textContent) ||
        (f.textContent ?? '').slice(0, 30) ||
        f.id ||
        '';
      return {
        tag: f.tagName.toLowerCase(),
        cls: (typeof f.className === 'string' ? f.className : '').trim().slice(0, 90),
        ss: f.classList.contains('ss-select'),
        name: name.trim().replace(/\s+/g, ' ').slice(0, 40),
        box: { x: r.left, y: r.top, width: r.width, height: r.height },
        borderColor: norm(cs.borderTopColor),
        borderWidth: cs.borderTopWidth,
        outlineStyle: cs.outlineStyle,
        outlineWidth: cs.outlineWidth,
        outlineColor: norm(cs.outlineColor),
        outlineOffset: cs.outlineOffset,
        faint,
        focus,
        border,
        visible: r.width > 0 && r.height > 0 && cs.visibility !== 'hidden',
      };
    });
  let count = 0;
  for (let i = 0; i < n; i++) {
    const f = all.nth(i);
    if (!(await f.isVisible().catch(() => false))) continue;
    await f.scrollIntoViewIfNeeded().catch(() => undefined);
    await page.mouse.move(2, 2);
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    await page.waitForTimeout(80);
    // the kit Select draws its box on the wrapper around its trigger button (the ring is :focus-within): read the wrapper
    const isTrigger = (await f.evaluate((e) => e.tagName.toLowerCase())) === 'button';
    const tgt = isTrigger ? f.locator('xpath=..') : f;
    const rest = await read(tgt);
    if (!rest.visible) continue;
    let hover: string | null = null;
    try {
      await f.hover({ timeout: 3000 });
      await page.waitForTimeout(120);
      hover = (await read(tgt)).borderColor;
    } catch {
      /* covered */
    }
    await page.mouse.move(2, 2);
    await page.keyboard.press('Tab').catch(() => undefined);
    await f.focus().catch(() => undefined);
    await page.waitForTimeout(150);
    const foc = await read(tgt);
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    const fig: SelectFig = {
      page: pageName,
      cfg: c.tag,
      tag: rest.tag,
      cls: rest.cls,
      name: rest.name,
      ssSelect: rest.ss,
      box: bx(rest.box),
      restBorder: rest.borderColor,
      hoverBorder: hover,
      borderWidth: rest.borderWidth,
      focusOutline: `${foc.outlineStyle} ${foc.outlineWidth}`,
      outlineColor: foc.outlineColor,
      outlineOffset: foc.outlineOffset,
      borderToken: rest.border,
      faintToken: rest.faint,
      focusToken: rest.focus,
      verdict: {
        rest: rest.borderColor !== rest.border,
        hover: hover === rest.faint,
        focus:
          foc.outlineStyle === 'solid' &&
          foc.outlineWidth === '2px' &&
          foc.outlineOffset === '1px' &&
          foc.outlineColor === rest.focus,
        height44: c.touch ? rest.box.height >= 43.5 : null,
      },
    };
    selectsOut.push(fig);
    count++;
    say(
      `SELECT ${pageName} ${c.tag} <${fig.tag}> {${fig.cls}} "${fig.name}" ${fig.box.w}x${fig.box.h}: rest border ${fig.restBorder} (token ${fig.borderToken}) ${fig.verdict.rest ? 'ok' : 'PLAIN'}; hover ${fig.hoverBorder} (faint ${fig.faintToken}) ${fig.verdict.hover ? 'ok' : 'NO'}; focus ${fig.focusOutline} offset ${fig.outlineOffset} ${fig.outlineColor} (focus ${fig.focusToken}) ${fig.verdict.focus ? 'ok' : 'NO'}${c.touch ? `; 44 px ${fig.verdict.height44 ? 'ok' : 'NO'}` : ''}`,
    );
  }
  say(
    `SELECTS ${pageName} ${c.tag}: ${count} read, .ss-select present: ${await root.locator('.ss-select').count()}`,
  );
}

// ---- part 1: Library list, Arcade and Study bars --------------------------------------------------------------------

async function libraryPart(browser: Browser, owner: BrowserContext) {
  const w = world();
  const listId = w.extras?.listId;
  const pages: { name: string; url: string | null }[] = [
    { name: 'lib-list', url: listId ? `/library/lists/${listId}` : null },
    { name: 'lib-lists', url: '/library/lists' },
  ];
  for (const c of CFGS) {
    const { ctx, page } = await newPage(browser, owner, c);
    try {
      for (const p of pages) {
        if (!p.url) {
          notCovered.push(`${p.name}: no extras.listId in world.json`);
          continue;
        }
        try {
          await visit(page, p.url);
          await shot(page, p.name, c);
          const main = page.getByRole('main').first();
          await testRoot(page, main, p.name, 'page', c);
          if (c.touch) await overlays(page, main, p.name, p.url, c);
        } catch (e) {
          notCovered.push(`${p.name} ${c.tag}: ${msg(e)}`);
        }
        flush();
      }
    } finally {
      await ctx.close();
    }
  }
}

async function barsPart(browser: Browser, owner: BrowserContext) {
  const bars: { name: string; url: string; root: (p: Page) => Locator; ready: string }[] = [
    {
      name: 'bar-arcade',
      url: '/games',
      root: (p) => p.locator('header.arc-roombar').first(),
      ready: '.arc-roombar-link',
    },
    {
      name: 'bar-study',
      url: '/study',
      root: (p) => p.locator('.stu-bar').first(),
      ready: '.stu-bar-link',
    },
  ];
  for (const c of CFGS) {
    const { ctx, page } = await newPage(browser, owner, c);
    try {
      for (const b of bars) {
        try {
          await visit(page, b.url, (p) => p.locator(b.ready).first().waitFor({ timeout: 20_000 }));
          const root = b.root(page);
          if ((await root.count()) === 0) {
            notCovered.push(`${b.name} ${c.tag}: bar not found`);
            continue;
          }
          await shot(page, b.name, c);
          extraFigs[`${b.name}-${c.tag}`] = await root.evaluate((bar) => {
            const rd = (n: number) => Math.round(n * 10) / 10;
            const box = (e: Element) => {
              const r = e.getBoundingClientRect();
              return { x: rd(r.left), y: rd(r.top), w: rd(r.width), h: rd(r.height) };
            };
            return {
              bar: box(bar),
              barClass: bar.className,
              links: Array.from(bar.querySelectorAll('a')).map((a) => ({
                text: (a.textContent ?? '').trim().slice(0, 30),
                cls: a.className,
                box: box(a),
                paddingBlock: getComputedStyle(a).paddingBlockStart,
              })),
            };
          });
          say(`${b.name} ${c.tag}: ${JSON.stringify(extraFigs[`${b.name}-${c.tag}`])}`);
          await testRoot(page, root, b.name, 'bar', c);
          await testRoot(
            page,
            page.locator('header:not(main header), nav:not(main nav)').first(),
            `${b.name}-chrome`,
            'first chrome header/nav',
            c,
          );
        } catch (e) {
          notCovered.push(`${b.name} ${c.tag}: ${msg(e)}`);
        }
        flush();
      }
    } finally {
      await ctx.close();
    }
  }
}

// ---- part 2: Admin --------------------------------------------------------------------------------------------------

async function adminPart(browser: Browser, owner: BrowserContext) {
  const w = world();
  const pages: { name: string; url: string; ready: (p: Page) => Promise<unknown> }[] = [
    {
      name: 'admin-accounts',
      url: '/admin/accounts',
      ready: (p) =>
        p.waitForSelector('main table tbody tr, main [role=row], main li', { timeout: 25_000 }),
    },
    {
      name: 'admin-limits',
      url: '/admin/limits',
      ready: (p) => p.getByRole('main').first().waitFor(),
    },
    {
      name: 'admin-audit-log',
      url: '/admin/audit-log',
      ready: (p) => p.getByRole('main').first().waitFor(),
    },
    {
      name: 'members-ss-select',
      url: `/settings/server/${w.guildId}/members`,
      ready: (p) => p.waitForSelector('main table tbody tr', { timeout: 25_000 }),
    },
  ];
  for (const c of CFGS) {
    const { ctx, page } = await newPage(browser, owner, c);
    try {
      for (const p of pages) {
        if (Date.now() - started > BUDGET_MS) {
          notCovered.push(`${p.name} ${c.tag}: the time budget was used up`);
          continue;
        }
        try {
          await visit(page, p.url, p.ready);
          const main = page.getByRole('main').first();
          await shot(page, p.name, c);
          if (p.name === 'admin-accounts') {
            extraFigs[`accounts-${c.tag}`] = await main.evaluate((m) => {
              const rd = (n: number) => Math.round(n * 10) / 10;
              const rows = Array.from(m.querySelectorAll('table tbody tr'));
              const first = rows[0];
              const r = first?.getBoundingClientRect();
              const table = m.querySelector('table');
              return {
                tableRows: rows.length,
                firstRow: r ? { y: rd(r.top), w: rd(r.width), h: rd(r.height) } : null,
                tableDisplay: table ? getComputedStyle(table).display : null,
                rowButtons: first
                  ? Array.from(first.querySelectorAll('button,a[href]')).map((b) => {
                      const q = b.getBoundingClientRect();
                      return {
                        name: (b.getAttribute('aria-label') ?? b.textContent ?? '')
                          .trim()
                          .slice(0, 30),
                        w: rd(q.width),
                        h: rd(q.height),
                      };
                    })
                  : [],
              };
            });
            say(`accounts ${c.tag}: ${JSON.stringify(extraFigs[`accounts-${c.tag}`])}`);
          }
          await readSelects(page, main, p.name, c);
          await testRoot(page, main, p.name, 'page', c);
          if (c.touch && p.name !== 'members-ss-select')
            await overlays(page, main, p.name, p.url, c);
        } catch (e) {
          notCovered.push(`${p.name} ${c.tag}: ${msg(e)}`);
          say(`NOT COVERED ${p.name} ${c.tag}: ${msg(e)}`);
        }
        flush();
      }
    } finally {
      await ctx.close();
    }
  }
}

// ---- parts 3 and 4: tools -------------------------------------------------------------------------------------------

const drawerOf = (page: Page) => page.getByRole('dialog', { name: 'Drawer', exact: true });

async function openTool(page: Page, query: string, pick: RegExp) {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(drawerOf(page)).toBeVisible({ timeout: 1000 });
  }).toPass({ timeout: 20_000 });
  await page.keyboard.type(`>${query}`);
  await page.waitForTimeout(600);
  await page.getByRole('option', { name: pick }).first().click();
  await page.waitForTimeout(900);
}

async function closeTool(page: Page) {
  for (
    let i = 0;
    i < 3 &&
    (await drawerOf(page)
      .isVisible()
      .catch(() => false));
    i++
  ) {
    await page.keyboard.press('Escape');
    await page.waitForTimeout(250);
  }
}

interface ToolDef {
  id: string;
  part: number;
  query: string;
  pick: RegExp;
  /** Tap the first primary button for a second state (a result on screen). */
  tap: boolean;
}
const TOOLS: ToolDef[] = [
  { id: 'hash', part: 3, query: 'hash', pick: /^hashes/i, tap: false },
  { id: 'regex', part: 3, query: 'regex', pick: /regex tester/i, tap: false },
  { id: 'url', part: 3, query: 'url', pick: /url encode/i, tap: false },
  { id: 'dice', part: 3, query: 'dice', pick: /^dice/i, tap: true },
  { id: 'coin', part: 3, query: 'coin', pick: /coin flip/i, tap: true },
  { id: 'wheel', part: 3, query: 'wheel', pick: /^wheel/i, tap: true },
  { id: 'loot', part: 4, query: 'loot', pick: /loot tables/i, tap: true },
  { id: 'notes', part: 4, query: 'notes', pick: /private notes/i, tap: false },
  { id: 'pomodoro', part: 4, query: 'pomodoro', pick: /pomodoro/i, tap: true },
  { id: 'stopwatch', part: 4, query: 'stopwatch', pick: /stopwatch/i, tap: true },
  { id: 'countdown', part: 4, query: 'countdown', pick: /countdown/i, tap: false },
  { id: 'programmer', part: 4, query: 'programmer', pick: /programmer/i, tap: false },
];

const RULE_OFF =
  '.ud-btn{min-block-size:32px !important}' +
  '.ud-root label:has(> input[type="checkbox"]),.ud-timer label:has(> input[type="checkbox"]){min-block-size:0 !important}';

/** The layout of every `.ud-btn` row in the drawer (parent boxes and every child), as it is now. */
async function measureRows(drawer: Locator) {
  return drawer.evaluate((root) => {
    const rd = (n: number) => Math.round(n * 10) / 10;
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      return { x: rd(r.left), y: rd(r.top), w: rd(r.width), h: rd(r.height) };
    };
    const parents = new Set<Element>();
    for (const b of Array.from(root.querySelectorAll('.ud-btn'))) {
      if (b.getClientRects().length && b.parentElement) parents.add(b.parentElement);
    }
    const out = Array.from(parents).map((p, i) => {
      const cs = getComputedStyle(p);
      const kids = Array.from(p.children)
        .filter((k) => k.getClientRects().length > 0)
        .map((k) => ({
          tag: k.tagName.toLowerCase(),
          cls: (typeof k.className === 'string' ? k.className : '').trim().slice(0, 60),
          name: (k.getAttribute('aria-label') ?? k.textContent ?? '')
            .trim()
            .replace(/\s+/g, ' ')
            .slice(0, 24),
          box: box(k),
        }));
      return {
        idx: i,
        parentCls: (typeof p.className === 'string' ? p.className : p.tagName).trim().slice(0, 80),
        parentDisplay: cs.display,
        parentWrap: cs.flexWrap,
        parentBox: box(p),
        kids,
        lines: new Set(kids.map((k) => Math.round(k.box.y))).size,
      };
    });
    const results = Array.from(
      root.querySelectorAll('.ud-out, .ud-clock, [role=status], output, .ud-result'),
    )
      .filter((e) => e.getClientRects().length > 0)
      .map((e) => ({
        cls: (typeof e.className === 'string' ? e.className : e.tagName).trim().slice(0, 40),
        box: box(e),
      }));
    const rootEl = root.querySelector('.ud-root, .ud-timer');
    return {
      rows: out,
      results,
      toolBox: rootEl ? box(rootEl) : null,
      drawerBox: box(root),
    };
  });
}

async function layoutOf(page: Page, drawer: Locator, tool: string, state: string, c: Cfg) {
  const now = await measureRows(drawer);
  let before: Awaited<ReturnType<typeof measureRows>> | null = null;
  if (c.touch) {
    await page.evaluate((css) => {
      const s = document.createElement('style');
      s.id = 'p109-rule-off';
      s.textContent = css;
      document.head.appendChild(s);
    }, RULE_OFF);
    await page.waitForTimeout(150);
    before = await measureRows(drawer);
    await page.evaluate(() => document.getElementById('p109-rule-off')?.remove());
    await page.waitForTimeout(150);
  }
  for (const row of now.rows) {
    const b = before?.rows.find((r) => r.idx === row.idx) ?? null;
    const flags: string[] = [];
    const btns = row.kids.filter((k) => k.cls.includes('ud-btn'));
    const others = row.kids.filter((k) => !k.cls.includes('ud-btn') && k.tag !== 'button');
    const maxOther = Math.max(0, ...others.map((k) => k.box.h));
    const maxBtn = Math.max(0, ...btns.map((k) => k.box.h));
    if (c.touch && others.length && maxBtn > maxOther + 0.6)
      flags.push(`button ${maxBtn} taller than its neighbour ${maxOther}`);
    if (b && row.lines > b.lines)
      flags.push(`row breaks in two: ${b.lines} line(s) -> ${row.lines}`);
    if (b && row.parentBox.h - b.parentBox.h > 0.6)
      flags.push(
        `row grew ${r1(row.parentBox.h - b.parentBox.h)} px (${b.parentBox.h} -> ${row.parentBox.h})`,
      );
    layoutOut.push({
      tool,
      state,
      cfg: c.tag,
      parentCls: row.parentCls,
      parentBox: row.parentBox,
      parentBefore: b?.parentBox ?? null,
      kids: row.kids.map((k) => ({
        ...k,
        before: b?.kids.find((q) => q.cls === k.cls && q.name === k.name)?.box ?? null,
      })),
      lines: row.lines,
      linesBefore: b?.lines ?? null,
      flags,
    });
    say(
      `LAYOUT ${tool} ${c.tag} [${state}] row {${row.parentCls}} ${row.parentDisplay}${row.parentWrap === 'wrap' ? ' wrap' : ''}: box ${row.parentBox.w}x${row.parentBox.h} at y${row.parentBox.y}` +
        `${b ? ` (before ${b.parentBox.w}x${b.parentBox.h})` : ''}; kids ${row.kids.map((k) => `${k.tag}${k.cls.includes('ud-btn') ? '.btn' : ''} "${k.name}" ${k.box.w}x${k.box.h}`).join(' | ')}; lines ${row.lines}${b ? ` (before ${b.lines})` : ''}${flags.length ? ` FLAGS: ${flags.join('; ')}` : ''}`,
    );
  }
  if (c.touch) {
    for (const r of now.results) {
      const bef = before?.results.find((q) => q.cls === r.cls);
      const bottom = r.box.y + r.box.h;
      const bb = bef ? bef.box.y + bef.box.h : null;
      say(
        `RESULT ${tool} ${c.tag} [${state}] {${r.cls}} y${r.box.y} bottom ${r1(bottom)}${bb !== null ? ` (before ${r1(bb)})` : ''}${bottom > c.h && (bb === null || bb <= c.h) ? ' PUSHED OFF THE FIRST SCREEN' : bottom > c.h ? ' (off the first screen in the before twin too)' : ''}`,
      );
    }
  }
  extraFigs[`${tool}-${c.tag}-${state}`] = {
    toolBox: now.toolBox,
    toolBoxBefore: before?.toolBox ?? null,
    drawerBox: now.drawerBox,
    results: now.results,
    resultsBefore: before?.results ?? null,
  };
  say(
    `TOOLBOX ${tool} ${c.tag} [${state}]: tool ${JSON.stringify(now.toolBox)}${before ? ` before ${JSON.stringify(before.toolBox)}` : ''}; drawer ${JSON.stringify(now.drawerBox)}`,
  );
}

async function toolsPart(browser: Browser, owner: BrowserContext) {
  const w = world();
  const tools = TOOLS.filter((t) => t.part === PART);
  for (const c of CFGS) {
    const { ctx, page } = await newPage(browser, owner, c);
    try {
      for (const t of tools) {
        if (Date.now() - started > BUDGET_MS) {
          notCovered.push(`${t.id} ${c.tag}: the time budget was used up`);
          continue;
        }
        try {
          const url = `/channels/${w.guildId}/${w.channelId}`;
          await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
          await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
          await settle(page, 4000);
          await openTool(page, t.query, t.pick);
          const drawer = drawerOf(page);
          await expect(drawer).toBeVisible();
          await settle(page, 3000);
          await shot(page, `tool-${t.id}`, c);
          await testRoot(page, drawer, `tool-${t.id}`, 'first state', c);
          await layoutOf(page, drawer, t.id, 'first state', c);
          if (t.tap && c.touch) {
            const prim = drawer.locator('.ud-btn-primary').first();
            if (await prim.count()) {
              await prim.evaluate((el) => (el as HTMLElement).click());
              await page.waitForTimeout(t.id === 'wheel' ? 3500 : 900);
              await shot(page, `tool-${t.id}-tapped`, c);
              await testRoot(page, drawer, `tool-${t.id}`, 'after the primary tap', c);
              await layoutOf(page, drawer, t.id, 'after the primary tap', c);
            } else notes.push(`${t.id}: no .ud-btn-primary to tap`);
          }
          await closeTool(page);
        } catch (e) {
          notCovered.push(`${t.id} ${c.tag}: ${msg(e)}`);
          say(`NOT COVERED ${t.id} ${c.tag}: ${msg(e)}`);
          await closeTool(page).catch(() => undefined);
        }
        flush();
      }
    } finally {
      await ctx.close();
    }
  }
}

// ---- the test -------------------------------------------------------------------------------------------------------

test(`census b part ${PART} (${PHASE})`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  try {
    await apiPatch(owner.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-light', zoom: 100, sectionThemes: SECTIONS } },
    });
    if (PART === 1) {
      await libraryPart(browser, owner);
      await barsPart(browser, owner);
    } else if (PART === 2) await adminPart(browser, owner);
    else await toolsPart(browser, owner);
  } finally {
    flush();
    await owner.close();
  }
  for (const n of notes) say(`note: ${n}`);
  for (const n of notCovered) say(`not covered: ${n}`);
  for (const [k, v] of Object.entries(themes)) say(`theme ${k}: ${JSON.stringify(v)}`);
  for (const [k, v] of Object.entries(sideways)) say(`sideways ${k}: ${v.join('/')}`);
  say(
    `part ${PART}: ${rowsOut.length} rows, fail ${rowsOut.filter((r) => r.status === 'fail').length}, not covered ${notCovered.length}, ${Math.round((Date.now() - started) / 1000)} s`,
  );
  expect(existsSync(resolve(OUT, `figures-${PHASE}-p${PART}.json`))).toBe(true);
  expect(notCovered, 'every page covered').toEqual([]);
});
