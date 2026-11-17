import { appendFileSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * Wave 90, members (SEC-AUD-387; D-898). Local only (SR_P90_MEMBERS_ON=1).
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`'s "seed the sweep world";
 * that spec is never edited), as `p86-guild-shots.spec.ts` does.
 *
 * The commander's command, after merging, from `e2e/` on a stack with the seeded world (API 8683, Vite 5683):
 *   SR_E2E_BASE_URL=http://localhost:5683 SR_P90_MEMBERS_ON=1 \
 *   SR_P90_MEMBERS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-members \
 *   npx playwright test tests/p90-members-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 *
 *   SR_P90_MEMBERS_BASE  1: the base case (tag `base`; run it on the tree before the change).
 *   SR_P90_MEMBERS_OUT   the folder of the shots and `measure.json`.
 *
 * Members at 375 (Lounge Light/Dark, touch) and 1280 (Editorial): every checkbox's box and hit tests (cy -/+ 21, and
 * the centre of every other control of its row), the first row's height, the first column's width, the table's width,
 * the bulk bar's box with one row ticked; after only: taps 15 px off a box's centre (phone), "select all" and its mixed
 * state, Tab and Space, the focus ring (1280). Chromium only.
 */
const ON = process.env.SR_P90_MEMBERS_ON === '1';
const OUT =
  process.env.SR_P90_MEMBERS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-members';
const BASE = process.env.SR_P90_MEMBERS_BASE === '1';
const TAG = BASE ? 'base' : 'after';
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
const ALL: { size: Size; theme: string }[] = [
  { size: 'phone', theme: 'lounge-light' },
  { size: 'phone', theme: 'lounge-dark' },
  { size: 'desktop', theme: 'lounge-editorial' },
];
const SETTINGS = ALL;
const NAV = '[role="dialog"] nav ul li > button';
const ROW_BOXES = "table.ss-table tbody input[type='checkbox']";
const HEAD_BOX = "table.ss-table thead input[type='checkbox']";

test.skip(!ON, 'local visual check only');
test.setTimeout(600_000);

interface World {
  guildId: string;
}
const loadWorld = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

/** In the page: every checkbox of the table, the other controls of the first rows, hit tests, heights and widths. */
function measureMembers() {
  const de = document.documentElement;
  const table = document.querySelector<HTMLElement>('table.ss-table');
  const themed = document.querySelector('[data-theme]');
  if (!table) return { found: false, theme: themed?.getAttribute('data-theme') ?? null };
  table.scrollIntoView({ block: 'start' });
  const hitOk = (el: HTMLElement, x: number, y: number) => {
    const h = document.elementFromPoint(x, y);
    if (!h) return false;
    if (h === el || el.contains(h)) return true;
    const lab =
      el.closest('label') ?? (el.id ? document.querySelector(`label[for="${el.id}"]`) : null);
    return !!lab && (h === lab || lab.contains(h));
  };
  const box = (e: HTMLElement) => {
    const r = e.getBoundingClientRect();
    return [r.left, r.top, r.width, r.height].map((n) => Math.round(n * 10) / 10);
  };
  const desc = (e: HTMLElement) =>
    `${e.tagName.toLowerCase()}${e instanceof HTMLInputElement ? ':' + e.type : ''}.${String(e.className).slice(0, 30)}`;
  const rows = Array.from(table.querySelectorAll<HTMLElement>('tr'));
  const out = rows.slice(0, 4).map((tr, i) => {
    const boxes = Array.from(
      tr.querySelectorAll<HTMLElement>('input[type=checkbox], button, a, select, [role=button]'),
    ).filter((e) => e.getBoundingClientRect().width > 0);
    const ctls = boxes.map((e) => {
      const r = e.getBoundingClientRect();
      const cx = r.left + r.width / 2;
      const cy = r.top + r.height / 2;
      const hit = document.elementFromPoint(cx, cy);
      return {
        el: desc(e),
        name: (e.getAttribute('aria-label') || e.textContent || '').trim().slice(0, 28),
        box: box(e),
        parent: e.parentElement ? desc(e.parentElement) : '',
        hitTop: hitOk(e, cx, cy - 21),
        hitBottom: hitOk(e, cx, cy + 21),
        centreOwn: hit === e || e.contains(hit),
        isCheckbox: e instanceof HTMLInputElement,
      };
    });
    return { i, rowH: Math.round(tr.getBoundingClientRect().height * 10) / 10, ctls };
  });
  const firstRow = table.querySelector<HTMLElement>('tbody tr');
  const firstCell = firstRow?.querySelector<HTMLElement>('td');
  const headCell = table.querySelector<HTMLElement>('thead th');
  const bar = document.querySelector<HTMLElement>('[role=toolbar]');
  const hdr = table.querySelector<HTMLInputElement>('thead input[type=checkbox]');
  return {
    found: true,
    theme: themed?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - de.clientWidth,
    tableBox: box(table),
    firstRowH: firstRow ? Math.round(firstRow.getBoundingClientRect().height * 10) / 10 : null,
    firstColW: firstCell ? box(firstCell)[2] : null,
    headColW: headCell ? box(headCell)[2] : null,
    bulkBar: bar ? box(bar) : null,
    header: hdr
      ? {
          checked: hdr.checked,
          indeterminate: hdr.indeterminate,
          aria: hdr.getAttribute('aria-checked'),
        }
      : null,
    rows: out,
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

test('SEC-AUD-387: Members checkboxes are the kit Checkbox', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const world = loadWorld();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const totals = { shots: 0, failed: [] as string[] };
  const log = (id: string, size: Size, theme: string, data: unknown) => {
    appendFileSync(jsonl, `${JSON.stringify({ id, tag: TAG, size, theme, data })}\n`);
    console.log(`REPORT ${TAG} ${size} ${theme} ${id} ${JSON.stringify(data).slice(0, 3000)}`);
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
  const openMembers = async (page: Page) => {
    await page.goto(`/settings/server/${world.guildId}`, {
      waitUntil: 'domcontentloaded',
      timeout: 15_000,
    });
    await idle(page, 4000);
    const labels = await page
      .locator(NAV)
      .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
    const i = labels.findIndex((l) => /^Members$/i.test(l));
    if (i < 0) throw new Error(`no nav item Members: ${labels.join(' | ')}`);
    await page
      .locator(NAV)
      .nth(i)
      .evaluate((el) => (el as HTMLElement).click());
    await idle(page, 2500);
    await page.evaluate(() => document.fonts?.ready);
  };

  for (const { size, theme } of SETTINGS) {
    await setTheme(theme);
    const { ctx, page } = await newPage(browser, size, theme, ownerCtx);
    await guard(`${size} ${theme} members`, async () => {
      await openMembers(page);
      log('members', size, theme, await page.evaluate(measureMembers));
      await shoot(page, 'members', size, theme);
      const boxes = page.locator(ROW_BOXES);
      if (BASE) {
        // One row ticked, base: the bulk bar's box (native input, clicked in the page).
        await boxes.first().evaluate((e) => (e as HTMLInputElement).click());
        await page.waitForTimeout(300);
        log('members-one-ticked', size, theme, await page.evaluate(measureMembers));
        return;
      }
      if (size === 'phone') {
        // A tap 15 px off the first row's box centre (below, beside, above) selects that row only.
        const n = await boxes.count();
        const res: unknown[] = [];
        for (const [dx, dy] of [
          [0, 15],
          [15, 0],
          [0, -15],
        ] as const) {
          const bb = await boxes.first().boundingBox();
          if (!bb) break;
          await page.touchscreen.tap(bb.x + bb.width / 2 + dx, bb.y + bb.height / 2 + dy);
          await page.waitForTimeout(250);
          const checked = await boxes.evaluateAll((els) =>
            els.map((e) => (e as HTMLInputElement).checked),
          );
          res.push({
            dx,
            dy,
            rows: n,
            checked: checked.slice(0, 6),
            only0: checked[0] === true && checked.slice(1).every((c) => !c),
          });
          if (checked[0]) await boxes.first().evaluate((e) => (e as HTMLInputElement).click());
        }
        log('tap-off-centre', size, theme, res);
        log('select-all-on-phone', size, theme, {
          headerCheckboxes: await page.locator(HEAD_BOX).count(),
        });
        // One row ticked with a real tap on its centre: the bulk bar shows.
        await page.evaluate(() => window.scrollTo(0, 0));
        const bb = await boxes.first().boundingBox();
        if (bb) await page.touchscreen.tap(bb.x + bb.width / 2, bb.y + bb.height / 2);
        await page.waitForTimeout(300);
        log('members-one-ticked', size, theme, await page.evaluate(measureMembers));
        await shoot(page, 'members-selected', size, theme);
      } else {
        const head = page.locator(HEAD_BOX);
        await boxes.first().click();
        await page.waitForTimeout(250);
        log('members-one-ticked', size, theme, await page.evaluate(measureMembers));
        await head.click();
        await page.waitForTimeout(250);
        const allChecked = await boxes.evaluateAll((els) =>
          els.map((e) => (e as HTMLInputElement).checked),
        );
        log('select-all', size, theme, {
          rows: allChecked.length,
          allTicked: allChecked.every(Boolean),
          header: await head.evaluate((e) => ({
            checked: (e as HTMLInputElement).checked,
            aria: e.getAttribute('aria-checked'),
          })),
        });
        await boxes.nth(1).click(); // untick one: the mixed state
        await page.waitForTimeout(250);
        log('select-all-mixed', size, theme, await page.evaluate(measureMembers));
        await shoot(page, 'members-select-all-mixed', size, theme);
        await page.getByRole('button', { name: /clear selection/i }).click();
        await page.waitForTimeout(250);
        // Keyboard: Tab from the page's start until the first row's checkbox, then Space.
        await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
        let tabs = 0;
        let reached = false;
        for (let i = 0; i < 80 && !reached; i++) {
          await page.keyboard.press('Tab');
          tabs++;
          reached = await page.evaluate(() =>
            /^Select /.test(document.activeElement?.getAttribute('aria-label') ?? ''),
          );
        }
        const focused = await page.evaluate(() => {
          const a = document.activeElement as HTMLInputElement | null;
          const cs = a ? getComputedStyle(a) : null;
          return {
            label: a?.getAttribute('aria-label'),
            outline: cs ? `${cs.outlineStyle} ${cs.outlineWidth} ${cs.outlineColor}` : null,
            focusVisible: a?.matches(':focus-visible'),
          };
        });
        await page.keyboard.press('Tab'); // first row's box (the select-all came first)
        await page.keyboard.press('Space');
        await page.waitForTimeout(250);
        const state = await page.evaluate(() => {
          const a = document.activeElement as HTMLInputElement | null;
          return { label: a?.getAttribute('aria-label'), checked: a?.checked };
        });
        log('keyboard', size, theme, { tabs, reached, focused, afterSpace: state });
        await shoot(page, 'members-keyboard', size, theme);
      }
    });
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
  console.log(`REPORT totals ${JSON.stringify(totals)}`);
});
