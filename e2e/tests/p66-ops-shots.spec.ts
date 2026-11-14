import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-195 (wave 66, D-647 and D-665): Admin > Bug reports with and without the "100 or more accounts" notice.
 * The list answer is faked with `page.route` (the sweep cannot make 100 accounts). Editorial, Lounge Light and
 * Lounge Dark, desktop and phone. Local only (SR_P66_OPS=1); shots go to SR_P66_OPS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P66_OPS_OUT ?? resolve('test-results/p66-ops');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const T = 1_800_000_000_000;
const ROW = {
  id: '12',
  created_ms: T,
  status: 'open',
  route: 'lounge.:guildId.:channelId',
  commit: '0a1b2c3d4e5f6a7b',
  reporter: { id: '9', username: 'bea' },
  has_screenshot: true,
};

test.skip(process.env.SR_P66_OPS !== '1', 'local visual check only');
test.setTimeout(900_000);

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

async function fakeList(page: Page, beta_over_100: boolean) {
  await page.unroute(/\/api\/v1\/admin\/bug-reports(\?.*)?$/).catch(() => undefined);
  await page.route(/\/api\/v1\/admin\/bug-reports(\?.*)?$/, (r) =>
    r.fulfill({
      json: {
        reports: [
          ROW,
          { ...ROW, id: '11', has_screenshot: false, reporter: { id: '8', username: 'carl' } },
        ],
        open_count: 2,
        beta_over_100,
      },
    }),
  );
}

test('the bug report list with and without the 100-accounts notice', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.route('**/api/v1/sr/features/flags', async (r) => {
        const res = await r.fetch();
        const j = (await res.json()) as { flags: Record<string, boolean> };
        await r.fulfill({
          response: res,
          json: { flags: { ...j.flags, 'platform.beta_reports': true } },
        });
      });
      const shot = async (name: string) => {
        await page.waitForTimeout(400);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };

      await fakeList(page, true);
      await page.goto('/admin/bug-reports');
      const notice = page.getByText(/this platform has 100 or more accounts/);
      await expect(notice).toBeVisible();
      await expect(page.getByRole('link', { name: 'Open Features' })).toBeVisible();
      await shot('notice');

      // The link goes to Features.
      await page.getByRole('link', { name: 'Open Features' }).click();
      await expect(page).toHaveURL(/\/admin\/features$/);
      await shot('notice-link-target');

      await fakeList(page, false);
      await page.goto('/admin/bug-reports');
      await expect(page.getByRole('table', { name: 'Bug reports' })).toBeVisible();
      await expect(notice).toHaveCount(0);
      await shot('no-notice');
      await page.unrouteAll({ behavior: 'ignoreErrors' });
      await c.close();
    }
  }
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
});
