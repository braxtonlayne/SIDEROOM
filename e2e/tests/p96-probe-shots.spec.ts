import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import {
  hitTest,
  hitTestAll,
  MENU_ITEM_SELECTOR,
  ProbeError,
  type ControlRow,
} from '../support/hit-test';

/**
 * Wave 96, probe builder: SEC-AUD-434 (D-941): `hitTestAll` takes menu items by the option `also`
 * (`MENU_ITEM_SELECTOR`); its default is unchanged. Proved on a message's long "More" menu at 375x812 with touch in
 * Lounge Light. NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by
 *   SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium \
 *     --workers=1 --retries=0 --reporter=line
 * from e2e/; that spec is never edited). The Owner's account. Creates no named data. Local only: without
 * SR_P96_PROBE_ON=1 it skips itself. Every variable starts with SR_P96_PROBE_.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8741, Vite 5741; also set SR_CLI_BIN,
 * SR_DATABASE__URL and DATABASE_URL of the stack):
 *   SR_E2E_BASE_URL=http://localhost:5741 SR_P96_PROBE_ON=1 \
 *   SR_P96_PROBE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p96-probe \
 *   npx playwright test tests/p96-probe-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 *
 * Case 1: `hitTestAll(page, menu)` without the option finds no `menuitem` row (the gap, watched).
 * Case 2: with `{ also: MENU_ITEM_SELECTOR }`: one row per menu item, in document order, all pass, 44 px or more.
 * Case 3: `hitTest` on each `[role=menuitem]` equals case 2's row (box and pass).
 * Case 4: menu closed, on `main`: with and without the option give the same rows.
 * probe.json and the shot go to SR_P96_PROBE_OUT. The shot is taken with the pointer on the first plain item and no
 * submenu open (D-943, wave 97).
 */
const ON = process.env.SR_P96_PROBE_ON === '1';
const OUT =
  process.env.SR_P96_PROBE_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p96-probe';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const THEME = 'lounge-light';

test.skip(!ON, 'local visual check only');
test.setTimeout(240_000);

const r1 = (n: number) => Math.round(n * 10) / 10;
const sig = (rows: ControlRow[]) =>
  rows.map((r) => ({
    tag: r.tag,
    role: r.role,
    name: r.name,
    status: r.status,
    box: r.result ? [r1(r.result.box.width), r1(r.result.box.height)] : null,
  }));

test('hitTestAll takes menu items by an option (SEC-AUD-434)', async ({ browser }) => {
  const world = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as {
    guildId: string;
    channelId: string;
  };
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: THEME, zoom: 100 } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  console.log(`REPORT account theme read back: ${back.settings?.user_prefs?.theme}`);
  expect(back.settings?.user_prefs?.theme).toBe(THEME);

  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: 375, height: 812 },
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
  const page: Page = await ctx.newPage();
  page.setDefaultTimeout(6000);
  await page.route(/sweep-preview\.invalid|\/cdn\/external\//, (r: Route) =>
    r.fulfill({ status: 200, contentType: 'image/png', body: Buffer.alloc(0) }),
  );
  try {
    await page.goto(`/channels/${world.guildId}/${world.channelId}`, {
      waitUntil: 'domcontentloaded',
      timeout: 15_000,
    });
    await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
    await page.locator('.lng-header').waitFor({ timeout: 8000 });
    await page.waitForTimeout(800);
    const theme = await page.evaluate(
      () => document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    );
    expect(theme).toBe(THEME);

    const main = page.getByRole('main');
    // Case 4, first half: the menu is closed.
    const mainPlain = await hitTestAll(page, main);
    const mainAlso = await hitTestAll(page, main, { also: MENU_ITEM_SELECTOR });

    // Open the long menu: a message toolbar's "More".
    const art = page.locator('article.lng-msg:has([role=toolbar])').last();
    await art.scrollIntoViewIfNeeded();
    await art.hover();
    await page.waitForTimeout(400);
    await art.locator('button[aria-label="More"]').tap();
    await page.waitForTimeout(600);
    const menu = page.getByRole('menu').first();
    await menu.waitFor({ timeout: 5000 });
    const items = menu.locator('[role=menuitem]');
    const nItems = await items.count();
    // D-943: the pointer that hovered the message rests where the menu opens, and a hover-opened submenu only closes
    // when the pointer enters a plain item. Rest it on the first plain item, wait out the hover intent
    // (SUBMENU_HOVER_INTENT_MS = 150), and assert no submenu is open and one menu panel is on the page.
    const plainItem = menu
      .locator('[role=menuitem]:not([aria-haspopup]):not([aria-disabled="true"])')
      .first();
    await plainItem.scrollIntoViewIfNeeded();
    await plainItem.hover();
    await page.waitForTimeout(450);
    await expect(
      menu.locator('[aria-haspopup="menu"][aria-expanded="true"]'),
      'no submenu is open for the shot (D-943)',
    ).toHaveCount(0);
    await expect(page.getByRole('menu'), 'one menu panel on the page (D-943)').toHaveCount(1);
    await menu.evaluate((m) => m.scrollTo(0, 0));
    mkdirSync(OUT, { recursive: true });
    await page.screenshot({ path: resolve(OUT, 'long-menu-375-light.png'), timeout: 10_000 });

    // Case 1: the gap.
    const plain = await hitTestAll(page, menu);
    // Case 2: the option.
    const also = await hitTestAll(page, menu, { also: MENU_ITEM_SELECTOR });
    // Case 3: the loop.
    const loop: { name: string; box: number[]; pass: boolean }[] = [];
    for (let i = 0; i < nItems; i++) {
      const it = items.nth(i);
      const name = ((await it.textContent()) ?? '').trim().replace(/\s+/g, ' ').slice(0, 40);
      try {
        const r = await hitTest(page, it);
        loop.push({ name, box: [r1(r.box.width), r1(r.box.height)], pass: r.pass });
      } catch (e) {
        if (!(e instanceof ProbeError)) throw e;
        loop.push({ name, box: [], pass: false });
      }
    }

    const itemRows = also.filter((r) => r.role === 'menuitem');
    const skipped = also.filter((r) => r.status === 'skipped').map((r) => r.name);
    writeFileSync(
      resolve(OUT, 'probe.json'),
      JSON.stringify(
        {
          theme,
          menuItems: nItems,
          case1: {
            rows: plain.length,
            menuitemRows: plain.filter((r) => r.role === 'menuitem').length,
          },
          case2: { rows: also.length, statuses: sig(also) },
          skipped,
          case3: loop,
          case4: { plain: sig(mainPlain), also: sig(mainAlso) },
        },
        null,
        1,
      ),
    );
    console.log(
      `REPORT items=${nItems} case1 rows=${plain.length} case2 rows=${also.length} skipped=${skipped.join('|')} main=${mainPlain.length}/${mainAlso.length}`,
    );

    // Case 1.
    expect(nItems, 'the long menu has items').toBeGreaterThan(0);
    expect(
      plain.filter((r) => r.role === 'menuitem'),
      'default: no menuitem row',
    ).toEqual([]);
    // Case 2.
    expect(also.length, 'one row per item, plus what case 1 found').toBe(nItems + plain.length);
    expect(itemRows.length).toBe(nItems);
    expect(also.filter((r) => r.status === 'fail' || r.status === 'not-reached')).toEqual([]);
    for (const r of itemRows.filter((x) => x.status === 'pass'))
      expect(r.result?.box.height ?? 0, `${r.name}: 44 px or more`).toBeGreaterThanOrEqual(44);
    // Case 3.
    expect(loop.length).toBe(itemRows.length);
    loop.forEach((l, i) => {
      const row = itemRows[i]!;
      if (row.status === 'skipped') return;
      expect(l.name).toBe(row.name);
      expect(l.box, `${l.name}: box equals`).toEqual([
        r1(row.result!.box.width),
        r1(row.result!.box.height),
      ]);
      expect(l.pass, `${l.name}: pass equals`).toBe(row.status === 'pass');
    });
    // Case 4.
    expect(sig(mainAlso)).toEqual(sig(mainPlain));
  } finally {
    await ctx.close().catch(() => undefined);
    await ownerCtx.close().catch(() => undefined);
  }
});
