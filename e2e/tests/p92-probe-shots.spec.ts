import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { apiDelete } from '../support/world';
import {
  CONTROL_SELECTOR,
  hitTest,
  hitTestAll,
  openCollapsed,
  ProbeError,
  type ControlRow,
  type Opened,
} from '../support/hit-test';

/**
 * Wave 92, probe builder: SEC-AUD-404 (D-917): the shared hit-test helper (`e2e/support/hit-test.ts`) watched to
 * fail and to pass, then run over every user Settings page at 375x812 with touch in Lounge Light. Nothing is built.
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by
 *   SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium \
 *     --workers=1 --retries=0 --reporter=line
 * from e2e/; that spec is never edited). The Owner's account. Local only: without SR_P92_PROBE_ON=1 it skips itself.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8706, Vite 5706):
 *   SR_E2E_BASE_URL=http://localhost:5706 SR_P92_PROBE_ON=1 \
 *   SR_P92_PROBE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p92-probe \
 *   npx playwright test tests/p92-probe-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * SR_P92_PROBE_BASE=1 asserts the chip figures of case 2 (the base, before the kit's row pitch); without it they are
 * printed only, so the spec does not break once the kit's fix is merged. SR_P92_PROBE_ONLY=1,2,3 picks cases.
 *
 * Case 1 (watched error, watched pass): Settings > Profile. `hitTestAll` on the "Advanced" card without
 *   `openCollapsed` collects "not reached" for the page-block selects and reports no fail; after it all 9 selects pass.
 * Case 2 (watched fail): wave 91 measure's case 1 (Server settings > Community > Onboarding "Default channels",
 *   channels p92-probe-r1 to r9 made through the API, chosen in the form, never saved): the helper reports "below"
 *   failing for the six remove buttons of rows 1 and 2 and names the lower row's button as the taker.
 * Case 3 (the pass): every user Settings page, the list read from the Settings navigation: `openCollapsed` on the
 *   content pane, then `hitTestAll`. A "not reached" fails the run. probe.json holds, per page, what was tested,
 *   opened, not reached and failed (the point and the taker); one shot per page with a failure (8 at most).
 * Shots and probe.json go to SR_P92_PROBE_OUT.
 */
const ON = process.env.SR_P92_PROBE_ON === '1';
const BASE = process.env.SR_P92_PROBE_BASE === '1';
const OUT =
  process.env.SR_P92_PROBE_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p92-probe';
const ONLY = (process.env.SR_P92_PROBE_ONLY ?? '1,2,3').split(',').map((s) => s.trim());
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PHONE = { width: 375, height: 812 };
const THEME = 'lounge-light';
const TAG = '375-lounge-light';
const MAX_SHOTS = 8;

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
const report: Record<string, unknown> = {};
const save = () => {
  mkdirSync(OUT, { recursive: true });
  writeFileSync(resolve(OUT, 'probe.json'), JSON.stringify(report, null, 1));
};
const say = (s: string) => console.log(`REPORT ${s}`);

async function setMain(ownerCtx: BrowserContext) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: THEME, zoom: 100 } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  const got = back.settings?.user_prefs?.theme;
  say(`account theme read back: ${got}`);
  expect(got).toBe(THEME);
}

async function newPage(browser: Browser, ownerCtx: BrowserContext) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: PHONE,
    storageState: await ownerCtx.storageState(),
    bypassCSP: true,
    userAgent: PHONE_UA,
    isMobile: true,
    hasTouch: true,
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
  }, THEME);
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
const shot = async (page: Page, name: string) => {
  mkdirSync(OUT, { recursive: true });
  await page.screenshot({ path: resolve(OUT, `${name}.png`), timeout: 10_000 });
};
const theme = (page: Page) =>
  page.evaluate(() => ({
    html: document.documentElement.getAttribute('data-theme'),
    first: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
  }));

const count = (rows: ControlRow[]) => ({
  pass: rows.filter((r) => r.status === 'pass').length,
  fail: rows.filter((r) => r.status === 'fail').length,
  notReached: rows.filter((r) => r.status === 'not-reached').length,
  skipped: rows.filter((r) => r.status === 'skipped').length,
});

/** The failing points of a row: which point, and who took it. */
const failures = (rows: ControlRow[]) =>
  rows
    .filter((r) => r.status === 'fail')
    .map((r) => ({
      index: r.index,
      control: `${r.tag}${r.role ? `[${r.role}]` : ''} "${r.name}"`,
      path: r.path,
      box: r.result?.box,
      points: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map((p) => ({ at: p.at, x: Math.round(p.x), y: Math.round(p.y), taker: p.taker })),
    }));

const notReached = (rows: ControlRow[]) =>
  rows
    .filter((r) => r.status === 'not-reached')
    .map((r) => ({ index: r.index, control: `${r.tag} "${r.name}"`, path: r.path, why: r.reason }));

/* ---------- case 1: Profile's "Advanced" card, closed then opened ---------- */
async function case1(browser: Browser, owner: BrowserContext) {
  await setMain(owner);
  const { ctx, page } = await newPage(browser, owner);
  await go(page, '/settings/user/profile');
  const main = page.locator('main[class*="content"]').first();
  const card = page.locator('details.us-advanced').first();
  await expect(card).toBeVisible();
  expect(await card.evaluate((e) => (e as HTMLDetailsElement).open)).toBe(false);

  // Watched error: the card is closed. Its selects are "not reached", and nothing is reported as a fail.
  const closed = await hitTestAll(page, card);
  const closedSelects = closed.filter((r) => r.role === 'combobox');
  say(`case1 closed: ${JSON.stringify(count(closed))}, selects ${closedSelects.length}`);
  expect(closedSelects.length).toBeGreaterThanOrEqual(8);
  expect(closedSelects.every((r) => r.status === 'not-reached')).toBe(true);
  expect(closed.filter((r) => r.status === 'fail')).toEqual([]);
  // And the single call throws.
  await expect(hitTest(page, card.locator('[role=combobox]').first())).rejects.toBeInstanceOf(
    ProbeError,
  );

  // Watched pass: the helper opens the card with a real click, then all 9 selects of the page pass.
  const opened = await openCollapsed(page, main);
  say(`case1 opened: ${JSON.stringify(opened)}`);
  expect(opened.some((o) => o.kind === 'details')).toBe(true);
  const rows = await hitTestAll(page, main);
  const selects = rows.filter((r) => r.role === 'combobox');
  say(`case1 open: ${JSON.stringify(count(rows))}, selects ${selects.length}`);
  expect(selects.length).toBe(9);
  expect(selects.every((r) => r.status === 'pass')).toBe(true);
  report['case1'] = {
    closed: { ...count(closed), selects: closedSelects.length },
    opened,
    open: { ...count(rows), selects: selects.length },
    theme: await theme(page),
  };
  save();
  await ctx.close();
}

/* ---------- case 2: equal-width chips, wave 91 measure's case 1 ---------- */
async function case2(browser: Browser, owner: BrowserContext) {
  const w = world();
  // Make the nine channels only when they are not there yet (a repeated run must not add duplicates).
  const have = (await (await owner.request.get(`/api/v1/guilds/${w.guildId}/channels`)).json()) as {
    id: string;
    name?: string;
  }[];
  const names = new Set<string | undefined>();
  for (const c of have) {
    if (!c.name?.startsWith('p92-probe-r')) continue;
    if (names.has(c.name)) await apiDelete(owner.request, `channels/${c.id}`);
    names.add(c.name);
  }
  for (let i = 1; i <= 9; i++)
    if (!names.has(`p92-probe-r${i}`))
      await apiPost(owner.request, `/guilds/${w.guildId}/channels`, {
        name: `p92-probe-r${i}`,
        type: 0,
      });
  await setMain(owner);
  const { ctx, page } = await newPage(browser, owner);
  await go(page, `/settings/server/${w.guildId}/community`);
  await page.getByRole('tab', { name: 'Onboarding' }).click();
  const field = page.locator('[aria-label="Default channels"]').first();
  await field.scrollIntoViewIfNeeded();
  for (let n = 0; n < 40; n++) {
    const gone = await page.evaluate(() => {
      const b = Array.from(
        document.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]'),
      ).find((x) => !/p92-probe-r\d/.test(x.getAttribute('aria-label') ?? ''));
      if (!b) return false;
      b.click();
      return true;
    });
    if (!gone) break;
    await page.waitForTimeout(150);
  }
  await field.click();
  for (let n = 0; n < 9; n++) {
    const free = page.locator('[role="option"]:not([aria-selected="true"])', {
      hasText: /p92-probe-r\d/,
    });
    if ((await page.getByRole('listbox').count()) === 0) await field.click();
    if ((await free.count()) === 0) break;
    await free.first().click();
    await page.waitForTimeout(150);
  }
  await page.keyboard.press('Escape');
  await page.waitForTimeout(500);

  const control = page
    .locator('div[class*="control"]')
    .filter({ has: page.locator('button[aria-label^="Remove "]') })
    .first();
  const rows = await hitTestAll(page, control);
  const chips = rows.filter((r) => r.name.startsWith('Remove '));
  say(`case2 chips: ${chips.map((r) => r.name + '/' + r.tag).join(' , ')}`);
  expect(chips.length).toBe(9);
  expect(notReached(rows)).toEqual([]);
  // Rows of chips, read once from the layout (a hit test scrolls, so its own boxes cannot be compared).
  const tops = await page.evaluate(() => {
    const out: Record<string, number> = {};
    document.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]').forEach((b) => {
      out[b.getAttribute('aria-label') ?? ''] = Math.round(
        b.parentElement!.getBoundingClientRect().top + window.scrollY,
      );
    });
    return out;
  });
  const ys = Array.from(new Set(Object.values(tops))).sort((x, y) => x - y);
  const rowOf = (r: ControlRow) => ys.indexOf(tops[r.name] ?? -1);
  const figures = chips.map((r) => ({
    name: r.name,
    row: rowOf(r),
    status: r.status,
    centre: r.result?.points.find((p) => p.at === 'centre')?.ok,
    above: r.result?.points.find((p) => p.at === 'above')?.ok,
    below: r.result?.points.find((p) => p.at === 'below')?.ok,
    belowTaker: r.result?.points.find((p) => p.at === 'below')?.taker?.name ?? null,
    belowTakerTag: r.result?.points.find((p) => p.at === 'below')?.taker?.tag ?? null,
  }));
  say(`case2 rows ${ys.length}: ${JSON.stringify(figures)}`);
  report['case2'] = { rows: ys.length, figures, base: BASE, theme: await theme(page) };
  save();
  if (BASE) {
    // A button fails "below" when a button of the next row sits under it (the names of this spec's channels are
    // longer than wave 91's, so a row holds two chips, not three); the taker is that lower row's button.
    const last = ys.length - 1;
    expect(ys.length).toBeGreaterThanOrEqual(2);
    const failing = figures.filter((f) => f.below === false);
    expect(failing.length).toBeGreaterThanOrEqual(6);
    for (const f of failing) {
      expect(f.belowTaker).toMatch(/^Remove /);
      expect(f.belowTaker).not.toBe(f.name);
      expect(figures.find((g) => g.name === f.belowTaker)?.row).toBe(f.row + 1);
      expect(f.row).toBeLessThan(last);
    }
    expect(figures.filter((f) => f.row === last).every((f) => f.status === 'pass')).toBe(true);
    expect(figures.every((f) => f.centre === true)).toBe(true);
    expect(figures.every((f) => f.above === true)).toBe(true);
  }
  await ctx.close();
}

/* ---------- case 3: every user Settings page ---------- */
interface PageRow {
  page: string;
  tested: number;
  pass: number;
  fail: number;
  notReached: number;
  skipped: number;
  opened: Opened[];
  failures: ReturnType<typeof failures>;
  notReachedList: ReturnType<typeof notReached>;
  shot?: string;
  theme?: unknown;
  ms: number;
  error?: string;
}

async function case3(browser: Browser, owner: BrowserContext) {
  await setMain(owner);
  const { ctx, page } = await newPage(browser, owner);
  await go(page, '/settings/user/account');
  const items = page.locator('button[class*="navItem"]');
  const labels = (await items.allTextContents()).map((s) => s.trim()).filter(Boolean);
  say(`case3 nav: ${labels.length} pages: ${labels.join(' | ')}`);
  expect(labels.length).toBeGreaterThanOrEqual(15);
  const pages: PageRow[] = [];
  report['case3'] = { labels, pages };
  let shots = 0;
  for (const label of labels) {
    const t0 = Date.now();
    const row: PageRow = {
      page: label,
      tested: 0,
      pass: 0,
      fail: 0,
      notReached: 0,
      skipped: 0,
      opened: [],
      failures: [],
      notReachedList: [],
      ms: 0,
    };
    pages.push(row);
    try {
      const toggle = page.locator('button[class*="navToggle"]').first();
      if ((await toggle.isVisible()) && (await toggle.getAttribute('aria-expanded')) === 'false')
        await toggle.click();
      await page.locator('button[class*="navItem"]', { hasText: label }).first().click();
      await idle(page, 1200);
      const main = page.locator('main[class*="content"]').first();
      await expect(main).toBeVisible();
      row.page = `${label} (${new URL(page.url()).pathname})`;
      row.opened = await openCollapsed(page, main);
      const rows = await hitTestAll(page, main);
      const c = count(rows);
      Object.assign(row, c, { tested: c.pass + c.fail });
      row.failures = failures(rows);
      row.notReachedList = notReached(rows);
      row.theme = await theme(page);
      if (c.fail > 0 && shots < MAX_SHOTS) {
        // The failing control in view: hit-test it again (it scrolls to the middle), then the shot of the viewport.
        const f = rows.find((r) => r.status === 'fail')!;
        await hitTest(page, main.locator(CONTROL_SELECTOR).nth(f.index)).catch(() => undefined);
        row.shot = `3-${String(shots + 1).padStart(2, '0')}-${label.toLowerCase().replace(/[^a-z0-9]+/g, '-')}-${TAG}`;
        await shot(page, row.shot);
        shots++;
      }
      say(
        `case3 ${row.page}: tested ${row.tested}, fail ${c.fail}, not reached ${c.notReached}, skipped ${c.skipped}, opened ${row.opened.length}`,
      );
    } catch (e) {
      row.error = String(e).slice(0, 300);
      say(`case3 ${label} ERROR ${row.error}`);
    }
    row.ms = Date.now() - t0;
    save();
  }
  await ctx.close();
  const bad = pages.filter((p) => p.notReached > 0 || p.error);
  expect(
    bad.map((p) => `${p.page}: not reached ${p.notReached} ${p.error ?? ''}`),
    'every control was reached',
  ).toEqual([]);
}

test('p92 probe: the hit-test helper, watched, then every user Settings page', async ({
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
      say(`case ${id} FAILED ${String(e).slice(0, 600)}`);
    }
    say(`case ${id} took ${Math.round((Date.now() - t0) / 1000)} s`);
  };
  await run('1', () => case1(browser, owner));
  await run('2', () => case2(browser, owner));
  await run('3', () => case3(browser, owner));
  await owner.close();
  report['errors'] = errors;
  save();
  if (errors.length) throw new Error(errors.join('\n'));
});
