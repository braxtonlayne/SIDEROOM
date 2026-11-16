import { appendFileSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * Wave 86, guild (SEC-AUD-374; D-880). Local only (SR_P86_GUILD=1).
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`'s "seed the sweep world";
 * that spec is never edited), as `p85-guild-shots.spec.ts` does.
 *
 * The commander's command, after merging, from `e2e/` on a stack with the seeded world (API 8642, Vite 5642):
 *   SR_E2E_BASE_URL=http://localhost:5642 SR_P86_GUILD=1 SR_P86_GUILD_TAG=after \
 *   SR_P86_GUILD_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-guild \
 *   npx playwright test tests/p86-guild-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 *
 *   SR_P86_GUILD_TAG   file-name prefix and log tag: `base` (before) or `after`.
 *   SR_P86_GUILD_OUT   the folder of the shots and `measure.json`.
 *   SR_P86_GUILD_BASE  1: the base case (tag `base`; measures only, no click-through).
 *
 * Members at 375 (Lounge Light/Dark, touch) and 1280 (Editorial): every table checkbox (box, parent, hit test at
 * cy -/+ 21, the row's height, the other controls of its row with their own hit tests); after only: a tap 15 px off a
 * box's centre selects that row only. One Admin table page (Accounts) at 375 and 1280: every cell's box (0 px moved).
 */
const ON = process.env.SR_P86_GUILD === '1';
const OUT =
  process.env.SR_P86_GUILD_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-guild';
const BASE = process.env.SR_P86_GUILD_BASE === '1';
const TAG = process.env.SR_P86_GUILD_TAG ?? (BASE ? 'base' : 'after');
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
test.setTimeout(900_000);

interface World {
  guildId: string;
}
const loadWorld = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

/** In the page: every checkbox of the table, its row's other controls, the hit tests, the row heights. */
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
  const out = rows.slice(0, 6).map((tr, i) => {
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
  const cellSig = Array.from(table.querySelectorAll<HTMLElement>('td, th')).map((c) => box(c));
  return {
    found: true,
    theme: themed?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - de.clientWidth,
    rows: out,
    sigLen: cellSig.length,
    sig: cellSig.slice(0, 80),
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

test('SEC-AUD-374: Members table checkboxes on a phone', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const world = loadWorld();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const totals = { shots: 0, failed: [] as string[] };
  const log = (id: string, size: Size, data: unknown) => {
    appendFileSync(jsonl, `${JSON.stringify({ id, tag: TAG, size, data })}\n`);
    console.log(`REPORT ${TAG} ${size} ${id} ${JSON.stringify(data).slice(0, 2400)}`);
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
    await guard(`${size} ${theme} members`, async () => {
      await openNav(page, `/settings/server/${world.guildId}`, /^Members$/i);
      await page.evaluate(() => document.fonts?.ready);
      const m = await page.evaluate(measureMembers);
      log('members', size, m);
      await shoot(page, 'members', size, theme);
      if (!BASE && size === 'phone') {
        // A tap 15 px off the first row's box centre (below, beside, above) selects that row only.
        const boxes = page.locator("table.ss-table tbody input[type='checkbox']");
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
        log('tap-off-centre', size, res);
        await boxes.first().evaluate((e) => (e as HTMLInputElement).click());
        await page.waitForTimeout(250);
        await shoot(page, 'members-selected', size, theme);
        const sel = await page.evaluate(measureMembers);
        log('members-selected', size, sel);
      }
    });
    if (theme !== 'lounge-light') {
      await guard(`${size} ${theme} admin`, async () => {
        await openNav(page, '/admin/dashboard', /^Accounts$/i);
        const m = await page.evaluate(() => {
          const t = document.querySelector<HTMLElement>('table');
          if (!t) return { found: false };
          const sig = Array.from(t.querySelectorAll<HTMLElement>('td, th')).map((c) => {
            const r = c.getBoundingClientRect();
            return [r.left, r.top, r.width, r.height].map((n) => Math.round(n * 10) / 10);
          });
          return {
            found: true,
            cls: String(t.className).slice(0, 60),
            sigLen: sig.length,
            sig,
            doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
          };
        });
        log('admin-accounts', size, m);
        await shoot(page, 'admin-accounts', size, theme);
      });
    }
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
  console.log(`REPORT totals ${JSON.stringify(totals)}`);
});
