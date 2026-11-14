import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-208 (wave 67, D-691): Admin > Review queues, Submissions: the queue unfiltered, a submitter pressed, the
 * chip, cleared. A fresh stack has no submissions, so the queue answer is faked with `page.route` (it honours the
 * `submitter` parameter like the API). Editorial, Lounge Light and Lounge Dark, desktop and phone. Local only
 * (SR_P67_READER=1); shots go to SR_P67_REVIEW_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P67_REVIEW_OUT ?? resolve('test-results/p67-review');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const T = 1_800_000_000_000;
const SUBMITTERS = ['310000000000000001', '310000000000000002', '310000000000000003'];

test.skip(process.env.SR_P67_READER !== '1', 'local visual check only');
test.setTimeout(900_000);

const row = (i: number) => ({
  id: String(500 + i),
  kind: i % 3 === 0 ? 'library.items' : 'library.series',
  state: 'submitted',
  title: ['Frieren: Beyond Journey', 'Dungeon Meshi', 'Monster', 'Vinland Saga', 'Berserk'][i % 5],
  summary: '',
  submitter_id: SUBMITTERS[i % 3],
  submitted_at_ms: T - (i + 1) * 3_600_000 * 7,
  updated_at_ms: T,
  claimed_by: null,
  claim_expires_at_ms: null,
});
const ALL = Array.from({ length: 9 }, (_, i) => row(i));

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

test('the review queue filtered by a submitter', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const requests: string[] = [];
  await setStudySurfacesWith(octx.request, true); // the Submissions tab is hidden while the ingest switch is off

  for (const theme of THEMES) {
    await apiPatch(octx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme, zoom: 100 } },
    });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.route(/\/api\/v1\/admin\/review\/queue(\?.*)?$/, (r) => {
        const u = new URL(r.request().url());
        requests.push(u.search);
        const only = u.searchParams.get('submitter');
        const items = only ? ALL.filter((s) => s.submitter_id === only) : ALL;
        return r.fulfill({ json: { items, total: items.length, limit: 25, offset: 0 } });
      });
      const shot = async (name: string) => {
        await page.waitForTimeout(400);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };

      await page.goto('/admin/review');
      await page.getByRole('tab', { name: 'Submissions' }).click();
      const table = page.getByRole('table', { name: 'Review queue' });
      await expect(table).toBeVisible();
      await expect(table.getByRole('row')).toHaveCount(ALL.length + 1);
      await shot('queue');

      await table
        .getByRole('button', { name: /^310000000000000002\. Show only this person/ })
        .first()
        .click();
      const chip = page.getByRole('button', { name: /Submitted by Account 310000000000000002/ });
      await expect(chip).toBeVisible();
      await expect(table.getByRole('row')).toHaveCount(3 + 1);
      await expect(page).toHaveURL(/submitter=310000000000000002/);
      expect(requests.at(-1)).toContain('submitter=310000000000000002');
      await shot('filtered');

      // A reload keeps it.
      await page.reload();
      await expect(chip).toBeVisible();
      await expect(page.getByRole('table', { name: 'Review queue' }).getByRole('row')).toHaveCount(
        4,
      );

      await chip.click();
      await expect(chip).toHaveCount(0);
      await expect(page).not.toHaveURL(/submitter=/);
      await expect(page.getByRole('table', { name: 'Review queue' }).getByRole('row')).toHaveCount(
        ALL.length + 1,
      );
      await shot('cleared');
      await c.close();
    }
  }
  await apiPatch(octx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
  await setStudySurfacesWith(octx.request, false);
});
