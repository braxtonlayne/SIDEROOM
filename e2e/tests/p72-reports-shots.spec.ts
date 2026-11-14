import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-277 (wave 72, D-758): Admin > Bug reports, the detail view of a report whose browser said nothing about
 * being online (posted by hand with `page.request`: a description only, and one with `online` as a string) and of a
 * normal one (`online` true, and one with false), on the real stack. The Online row reads "Unknown" for the first two.
 *
 * Editorial, Lounge Light and Lounge Dark, desktop 1280x800 and phone 375x812. Local only (SR_P72_REPORTS=1); shots go
 * to SR_P72_REPORTS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P72_REPORTS_OUT ?? resolve('test-results/p72-reports');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P72_REPORTS !== '1', 'local visual check only');
test.setTimeout(600_000);

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
) {
  const phone = size.width < 500;
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    ...(phone ? { isMobile: true, hasTouch: true } : {}),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100 },
  );
  return c;
}

const bundle = (online: unknown) => ({
  commit: '0a1b2c3d4e5f6a7b',
  route: 'lounge.:guildId.:channelId',
  at: Date.now(),
  viewport: { w: 1280, h: 800, dpr: 2 },
  caps: { engine: 'chromium', wasm: true },
  locale: 'en',
  theme: 'lounge-dark',
  ...(online === undefined ? {} : { online }),
  gateway: { status: 'connected' },
  sw: 'active',
  voice: { connected: false },
  failed_requests: [],
  crashes: [],
});

test('Admin > Bug reports: the Online row of a report without `online` reads Unknown', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, bootstrapState().owner);
  const o = octx.request;
  await apiPatch(o, '/admin/features/platform.beta_reports', { enabled: true });

  // Posted oldest first; the list is newest first.
  const made: Record<string, { id: string }> = {};
  made['online-false'] = await apiPost(o, '/bug-reports', {
    description: 'Report with online false.',
    bundle: bundle(false),
  });
  made['online-true'] = await apiPost(o, '/bug-reports', {
    description: 'Report with online true.',
    bundle: bundle(true),
  });
  made['online-string'] = await apiPost(o, '/bug-reports', {
    description: 'Report with online as a string.',
    bundle: bundle('yes'),
  });
  made['online-missing'] = await apiPost(o, '/bug-reports', {
    description: 'Report without online.',
    bundle: bundle(undefined),
  });
  made['description-only'] = await apiPost(o, '/bug-reports', { description: 'Description only.' });
  const order = [
    'description-only',
    'online-missing',
    'online-string',
    'online-true',
    'online-false',
  ];
  const want: Record<string, string> = {
    'description-only': 'Unknown',
    'online-missing': 'Unknown',
    'online-string': 'Unknown',
    'online-true': 'Yes',
    'online-false': 'No',
  };

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const names = theme === 'lounge-editorial' ? order : ['online-missing', 'online-true'];
      for (const name of names) {
        await page.goto('/admin/bug-reports');
        const rows = page.getByRole('table', { name: 'Bug reports' }).locator('tbody tr');
        await expect(rows).toHaveCount(order.length);
        await rows
          .nth(order.indexOf(name))
          .getByRole('button', { name: /Open the report/ })
          .click();
        await expect(page.getByRole('button', { name: 'Close report' })).toBeVisible();
        const cell = page
          .locator('#adm-bug-debug')
          .getByText('Online', { exact: true })
          .locator('xpath=following-sibling::dd[1]');
        await expect(cell).toHaveText(want[name] ?? '');
        await page.waitForTimeout(300);
        await page.locator('#adm-bug-debug').scrollIntoViewIfNeeded();
        await page.screenshot({ path: resolve(OUT, `detail-${name}-${tag}.png`), fullPage: true });
      }
      await c.close();
    }
  }
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
  expect(Object.keys(made).length).toBe(5);
});
