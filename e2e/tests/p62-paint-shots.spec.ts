import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext } from '@playwright/test';
import { bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-144 (wave 62): the Handbook, Library browse and shelf pages load as their own chunks. Direct loads
 * (reload on /help and /library/browse?filter), a click-through, and shots in Editorial, Lounge Light and Lounge Dark,
 * desktop and phone. Local only (SR_P62_PAINT=1); shots go to SR_P62_PAINT_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P62_PAINT_OUT ?? resolve('test-results/p62-paint');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const PAGES = [
  ['help', '/help'],
  ['help-operators', '/help/operators'],
  ['library-browse', '/library/browse?kind=novel&sort=title&bogus=1'],
  ['library-shelf-list', '/library/shelves'],
  ['library-shelf-item', '/library/shelves/9999'],
  ['cinema-shelf-list', '/cinema/shelves'],
  ['cinema-shelf-item', '/cinema/shelves/9999'],
] as const;

test.skip(process.env.SR_P62_PAINT !== '1', 'local visual check only');
test.setTimeout(1_200_000);

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100 },
  );
  return c;
}

test('lazy pages load directly, click through and look right', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, bootstrapState().owner);

  for (const theme of THEMES) {
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      const errors: string[] = [];
      page.on('pageerror', (e) => errors.push(String(e)));
      for (const [name, path] of PAGES) {
        await page.goto(path);
        await page.waitForLoadState('networkidle');
        await page.screenshot({
          path: resolve(OUT, `${name}-${vp}-${theme}.png`),
          fullPage: false,
        });
      }
      expect(errors).toEqual([]);
      await c.close();
    }
  }

  // Click-through on desktop: Lounge -> Help -> back -> Library browse -> a filter -> shelves.
  const c = await context(browser, octx, VIEWPORTS.desktop, 'lounge-editorial');
  const page = await c.newPage();
  await page.goto('/channels');
  await page.waitForLoadState('networkidle');
  await page.goto('/help'); // a full load: the lazy chunk is fetched by a first request
  await expect(page.getByRole('heading', { name: 'Help' }).first()).toBeVisible();
  await page.goBack();
  await page.goto('/library/browse?kind=novel&bogus=1');
  await expect(page.getByRole('heading', { name: 'Browse' }).first()).toBeVisible();
  await expect.poll(() => page.url()).toContain('kind=novel');
  await page.getByRole('button', { name: 'Comic', exact: true }).click();
  await expect.poll(() => page.url()).toContain('kind=comic');
  await page.screenshot({ path: resolve(OUT, 'flow-browse-filter.png') });
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Browse' }).first()).toBeVisible();
  expect(page.url()).toContain('kind=comic');
  await page.locator('a[href="/library/shelves"]').first().click();
  await expect.poll(() => page.url()).toContain('/library/shelves');
  await expect(page.getByRole('heading', { name: 'Shelves' }).first()).toBeVisible();
  await page.goto('/help');
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Help' }).first()).toBeVisible();
  await c.close();
});
