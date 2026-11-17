import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, hitTest, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * Wave 93, kit builder: SEC-AUD-416 and the control of 414 (D-922): on a phone the kit's segmented control and slider
 * keep their 44 px touch target inside their own layout box. Local only: without SR_P93_KIT_ON=1 it skips itself.
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by
 *   SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium \
 *     --workers=1 --retries=0 --reporter=line
 * from e2e/; that spec is never edited). The Owner's account.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8711, Vite 5711, metrics 9411,
 * `SR_ADMIN_REQUIRE_REAUTH=0`):
 *   SR_E2E_BASE_URL=http://localhost:5711 SR_P93_KIT_ON=1 \
 *   SR_P93_KIT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-kit \
 *   npx playwright test tests/p93-kit-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * `SR_P93_KIT_BASE=1` names the files `base-*` (run it with the sheet's change switched off). `SR_P93_KIT_ONLY=a,b`
 * picks cases: a the kit controls of Voice & Video and Profile's "view as" at 375 Lounge Light (every radio and range:
 * box, group box, three points, taker, the px between the box and the taker's edge; a shot of "Optimise for"), b the
 * 20 user Settings pages at 375 Lounge Light (every control through `hitTestAll`, page heights), c Voice & Video at
 * 375 Lounge Dark, d Voice & Video at 1280 Editorial (the boxes of the kit controls, to compare with the base), e the
 * Library home at 375 (a page outside Settings that uses a segmented control).
 * Every hit test is the shared helper (`e2e/support/hit-test.ts`); `<name>-pass.json` and `-measure.json` go to
 * SR_P93_KIT_OUT. With a `base-pass.json` beside it, case b prints the controls that passed on the base and fail now.
 */
const ON = process.env.SR_P93_KIT_ON === '1';
const BASE = process.env.SR_P93_KIT_BASE === '1';
const OUT =
  process.env.SR_P93_KIT_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-kit';
const ONLY = (process.env.SR_P93_KIT_ONLY ?? 'a,b,c,d,e').split(',').map((s) => s.trim());
const PRE = BASE ? 'base' : 'after';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PHONE = { width: 375, height: 812 };
const DESK = { width: 1280, height: 800 };

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const rows: unknown[] = [];
const log = (id: string, data: unknown) => {
  mkdirSync(OUT, { recursive: true });
  rows.push({ id, ...(data as object) });
  writeFileSync(resolve(OUT, `${PRE}-measure.json`), JSON.stringify(rows, null, 1));
  console.log(`REPORT ${id} ${JSON.stringify(data).slice(0, 900)}`);
};

async function setMain(ownerCtx: BrowserContext, theme: string) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  const got = back.settings?.user_prefs?.theme;
  console.log(`REPORT account theme read back: ${got}`);
  expect(got).toBe(theme);
}

async function newPage(browser: Browser, ownerCtx: BrowserContext, theme: string, phone: boolean) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: phone ? PHONE : DESK,
    storageState: await ownerCtx.storageState(),
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

const idle = async (page: Page, ms = 1500) => {
  await page.waitForLoadState('networkidle', { timeout: 3000 }).catch(() => undefined);
  await page.waitForTimeout(ms);
};
const go = async (page: Page, path: string) => {
  await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 20_000 });
  await idle(page);
};
const state = (page: Page) =>
  page.evaluate(() => ({
    theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
  }));
/** The viewport shot; data-theme and the sideways scroll are read back first (D-817). */
const shot = async (page: Page, name: string) => {
  mkdirSync(OUT, { recursive: true });
  const s = await state(page);
  console.log(`REPORT shot ${PRE}-${name} data-theme ${s.theme} scroll ${s.scroll}`);
  await page.screenshot({ path: resolve(OUT, `${PRE}-${name}.png`), timeout: 10_000 });
  return s;
};

const KIT = 'input[type="radio"], input[type="range"]';
const r1 = (n: number) => Math.round(n * 10) / 10;

/** Every radio and range input under `root`: hit test (helper), the group's box, who took each failing point. */
async function kitControls(page: Page, root: Locator) {
  const inputs = root.locator(KIT);
  const n = await inputs.count();
  const out: unknown[] = [];
  for (let i = 0; i < n; i++) {
    const loc = inputs.nth(i);
    if (!(await loc.isVisible())) continue;
    const kind = await loc.evaluate((e) => (e as HTMLInputElement).type);
    const name = await loc.evaluate((e) => {
      const lab = e.closest('label') ?? document.querySelector(`label[for="${e.id}"]`);
      return (lab?.textContent ?? e.getAttribute('aria-label') ?? '').trim().slice(0, 30);
    });
    const res = await hitTest(page, loc, kind === 'radio' ? {} : { wrapper: '[class*="slider"]' });
    // After the helper's scroll: the group's (or the slider's) box, the box of every taker that is not the control.
    const extra = await loc.evaluate(
      (el, pts) => {
        const g = el.closest('[class*="segmented"], [class*="slider"]');
        const gb = g?.getBoundingClientRect();
        const takers = pts.map((p) => {
          const t = document.elementFromPoint(p.x, p.y);
          const tb = t?.getBoundingClientRect();
          return tb ? [tb.left, tb.top, tb.width, tb.height] : null;
        });
        return { group: gb ? [gb.left, gb.top, gb.width, gb.height] : null, takers };
      },
      res.points.map((p) => ({ x: p.x, y: p.y })),
    );
    const gb = extra.group;
    out.push({
      kind,
      name,
      box: [res.box.x, res.box.y, res.box.width, res.box.height].map(r1),
      group: gb?.map((v) => r1(v)),
      pass: res.pass,
      points: res.points.map((p, k) => ({
        at: p.at,
        y: r1(p.y),
        ok: p.ok,
        relation: p.relation,
        taker: p.ok ? null : `${p.taker?.tag} "${p.taker?.name}" ${p.taker?.path}`,
        takerBox: p.ok ? null : extra.takers[k]?.map((v) => r1(v)),
        // px between the group's edge and where the taker sits: a negative number is inside the group.
        outsideGroupPx:
          p.ok || !gb
            ? null
            : r1(p.at === 'above' ? (gb[1] ?? 0) - p.y : p.y - ((gb[1] ?? 0) + (gb[3] ?? 0))),
      })),
    });
  }
  return out;
}

const section = (page: Page) => page.locator('main[class*="content"]').first();

async function openSection(page: Page, id: string) {
  await go(page, `/settings/user/${id}`);
  await expect(section(page)).toBeVisible();
  await openCollapsed(page, section(page));
}

const heights = (page: Page) =>
  page.evaluate(() => {
    const m = document.querySelector('main[class*="content"]');
    return {
      doc: document.documentElement.scrollHeight,
      main: m ? m.scrollHeight : null,
      scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
    };
  });

/** Scrolls the control to the middle and then 150 px further, so the group and what lies under it show. */
async function centreOnBottom(page: Page, loc: Locator) {
  await loc.evaluate((el) => {
    (el as HTMLElement).scrollIntoView({ block: 'center', behavior: 'instant' });
    let sc: Element | null = el.parentElement;
    while (
      sc &&
      !(sc.scrollHeight > sc.clientHeight && /(auto|scroll)/.test(getComputedStyle(sc).overflowY))
    )
      sc = sc.parentElement;
    (sc ?? document.scrollingElement)?.scrollBy(0, 150);
  });
  await page.waitForTimeout(300);
}

async function caseA(browser: Browser, owner: BrowserContext) {
  await setMain(owner, 'lounge-light');
  const { ctx, page } = await newPage(browser, owner, 'lounge-light', true);
  await openSection(page, 'voice');
  const vv = await kitControls(page, section(page));
  log('a-voice-controls-375-lounge-light', { n: vv.length, controls: vv, h: await heights(page) });
  const opt = page
    .locator('[class*="segmented"]')
    .filter({ has: page.locator('input[type="radio"]') })
    .last();
  await centreOnBottom(page, opt);
  await shot(page, 'voice-optimise-375-lounge-light');
  // A tap on a pill selects it (the radio is the pill's own box); a slider drags.
  const sliders = section(page).locator('input[type="range"]:not([disabled])');
  if ((await sliders.count()) > 0) {
    const s = sliders.first();
    await s.scrollIntoViewIfNeeded();
    const b = (await s.boundingBox())!;
    const before = await s.inputValue();
    await page.touchscreen.tap(b.x + b.width * 0.8, b.y + b.height / 2);
    await page.waitForTimeout(200);
    log('a-slider-tap', { before, after: await s.inputValue() });
  }
  await ctx.close();

  const p2 = await newPage(browser, owner, 'lounge-light', true);
  await go(p2.page, '/settings/user/profile');
  await openCollapsed(p2.page, section(p2.page));
  let va = p2.page.locator('.us-view-as').first();
  if ((await va.count()) === 0) {
    await openSection(p2.page, 'privacy');
    va = p2.page.locator('.us-view-as').first();
  }
  expect(await va.count(), '"view as" found').toBeGreaterThan(0);
  const ctl = va.locator('[class*="segmented"]').first();
  const viewAs = await kitControls(p2.page, ctl);
  log('a-view-as-375-lounge-light', {
    n: viewAs.length,
    controls: viewAs,
    h: await heights(p2.page),
  });
  await centreOnBottom(p2.page, ctl);
  await shot(p2.page, 'view-as-375-lounge-light');
  // Tap the second pill: it becomes the checked one.
  const second = ctl.locator('input[type="radio"]').nth(1);
  const b2 = (await second.boundingBox())!;
  await p2.page.touchscreen.tap(b2.x + b2.width / 2, b2.y + b2.height / 2);
  await p2.page.waitForTimeout(400);
  log('a-view-as-tap', { checked: await second.isChecked() });
  await p2.ctx.close();
}

interface PassRow {
  page: string;
  tested: number;
  fail: number;
  notReached: number;
  height: unknown;
  fails: { control: string; path: string; points: unknown }[];
  status: Record<string, string>;
}

async function caseB(browser: Browser, owner: BrowserContext) {
  await setMain(owner, 'lounge-light');
  const { ctx, page } = await newPage(browser, owner, 'lounge-light', true);
  await go(page, '/settings/user/account');
  const items = page.locator('button[class*="navItem"]');
  const labels = (await items.allTextContents()).map((s) => s.trim()).filter(Boolean);
  expect(labels.length).toBeGreaterThanOrEqual(15);
  const pages: PassRow[] = [];
  for (const label of labels) {
    const toggle = page.locator('button[class*="navToggle"]').first();
    if ((await toggle.isVisible()) && (await toggle.getAttribute('aria-expanded')) === 'false')
      await toggle.click();
    await page.locator('button[class*="navItem"]', { hasText: label }).first().click();
    await idle(page, 1200);
    const main = section(page);
    await expect(main).toBeVisible();
    const path = new URL(page.url()).pathname;
    await openCollapsed(page, main);
    const h = await heights(page);
    const res: ControlRow[] = await hitTestAll(page, main);
    const status: Record<string, string> = {};
    res.forEach((r) => {
      status[`${r.index}:${r.tag}:${r.type ?? ''}:${r.name}`] = r.status;
    });
    const fails = res
      .filter((r) => r.status === 'fail')
      .map((r) => ({
        control: `${r.tag}${r.type ? `[${r.type}]` : ''} "${r.name}"`,
        path: r.path,
        points: (r.result?.points ?? [])
          .filter((p) => !p.ok)
          .map((p) => `${p.at}: ${p.taker?.tag} "${p.taker?.name}" ${p.taker?.path}`),
      }));
    const row: PassRow = {
      page: `${label} (${path})`,
      tested: res.filter((r) => r.status === 'pass' || r.status === 'fail').length,
      fail: fails.length,
      notReached: res.filter((r) => r.status === 'not-reached').length,
      height: h,
      fails,
      status,
    };
    pages.push(row);
    console.log(
      `REPORT b ${row.page}: tested ${row.tested} fail ${row.fail} notReached ${row.notReached} h ${JSON.stringify(h)}`,
    );
    for (const f of fails)
      console.log(`REPORT b   FAIL ${f.control} ${f.path} ${JSON.stringify(f.points)}`);
  }
  mkdirSync(OUT, { recursive: true });
  writeFileSync(resolve(OUT, `${PRE}-pass.json`), JSON.stringify(pages, null, 1));
  await ctx.close();
  if (!BASE) {
    let basePages: PassRow[] | null = null;
    try {
      basePages = JSON.parse(readFileSync(resolve(OUT, 'base-pass.json'), 'utf8')) as PassRow[];
    } catch {
      console.log('REPORT b no base-pass.json to compare with');
    }
    if (basePages) {
      const worse: string[] = [];
      const grew: string[] = [];
      for (const p of pages) {
        const b = basePages.find((x) => x.page === p.page);
        if (!b) continue;
        for (const [k, v] of Object.entries(p.status))
          if (b.status[k] === 'pass' && v !== 'pass') worse.push(`${p.page} ${k}: ${v}`);
        const hb = (b.height as { doc: number }).doc;
        const ha = (p.height as { doc: number }).doc;
        if (hb !== ha) grew.push(`${p.page}: ${hb} -> ${ha} (${ha - hb})`);
      }
      console.log(`REPORT b passed on the base and not now: ${JSON.stringify(worse)}`);
      console.log(`REPORT b page heights (doc scrollHeight) that changed: ${JSON.stringify(grew)}`);
      log('b-compare', { worse, grew });
    }
  }
  const bad = pages.filter((p) => p.notReached > 0);
  expect(bad.map((p) => `${p.page}: not reached ${p.notReached}`)).toEqual([]);
}

async function caseC(browser: Browser, owner: BrowserContext) {
  await setMain(owner, 'lounge-dark');
  const { ctx, page } = await newPage(browser, owner, 'lounge-dark', true);
  await openSection(page, 'voice');
  const vv = await kitControls(page, section(page));
  log('c-voice-controls-375-lounge-dark', { n: vv.length, controls: vv, h: await heights(page) });
  const opt = page
    .locator('[class*="segmented"]')
    .filter({ has: page.locator('input[type="radio"]') })
    .last();
  await centreOnBottom(page, opt);
  await shot(page, 'voice-optimise-375-lounge-dark');
  await ctx.close();
}

async function caseD(browser: Browser, owner: BrowserContext) {
  await setMain(owner, 'lounge-editorial');
  const { ctx, page } = await newPage(browser, owner, 'lounge-editorial', false);
  await openSection(page, 'voice');
  // Boxes only (no hit test needed at 1280); relative to the content pane's top so a scroll does not matter.
  const boxes = await section(page)
    .locator(KIT)
    .evaluateAll((els) =>
      els.map((e) => {
        const b = e.getBoundingClientRect();
        const g = e.closest('[class*="segmented"], [class*="slider"]')!.getBoundingClientRect();
        const m = document.querySelector('main[class*="content"]')!.getBoundingClientRect();
        const r = (n: number) => Math.round(n * 10) / 10;
        return {
          box: [b.width, b.height, b.top - m.top].map(r),
          group: [g.width, g.height, g.top - m.top].map(r),
        };
      }),
    );
  log('d-voice-boxes-1280-editorial', { n: boxes.length, boxes, h: await heights(page) });
  const opt = page
    .locator('[class*="segmented"]')
    .filter({ has: page.locator('input[type="radio"]') })
    .last();
  await centreOnBottom(page, opt);
  await shot(page, 'voice-1280-editorial');
  await ctx.close();
}

async function caseE(browser: Browser, owner: BrowserContext) {
  await setMain(owner, 'lounge-light');
  const { ctx, page } = await newPage(browser, owner, 'lounge-light', true);
  await go(page, '/library');
  const seg = page.locator('[class*="segmented"]').first();
  const n = await page.locator('[class*="segmented"]').count();
  if (n === 0) {
    log('e-library', { segmented: 0, url: page.url() });
  } else {
    const c = await kitControls(page, page.locator('[class*="segmented"]').first());
    log('e-library', { segmented: n, url: page.url(), controls: c, h: await heights(page) });
    await seg.scrollIntoViewIfNeeded();
    await shot(page, 'library-375-lounge-light');
  }
  await ctx.close();
}

test('p93 kit: the segmented control and slider keep their phone target inside their box', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const errors: string[] = [];
  const run = async (id: string, fn: () => Promise<void>) => {
    if (!ONLY.includes(id)) return;
    const t0 = Date.now();
    try {
      await fn();
    } catch (e) {
      errors.push(`case ${id}: ${String(e).slice(0, 600)}`);
      console.log(`REPORT case ${id} FAILED ${String(e).slice(0, 600)}`);
    }
    console.log(`REPORT case ${id} took ${Math.round((Date.now() - t0) / 1000)} s`);
  };
  await run('a', () => caseA(browser, owner));
  await run('b', () => caseB(browser, owner));
  await run('c', () => caseC(browser, owner));
  await run('d', () => caseD(browser, owner));
  await run('e', () => caseE(browser, owner));
  await owner.close();
  if (errors.length) throw new Error(errors.join('\n'));
});
