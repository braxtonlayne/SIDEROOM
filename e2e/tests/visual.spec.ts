import { expect, test, type Page } from '@playwright/test';
import { apiPost, loginApi, bootstrapState } from '../support/fixtures';

/**
 * XC-TEST-005: visual regression snapshots for the Entrance, Cinema and Library pages.
 *
 * Opt-in (SR_E2E_VISUAL=1) and chromium-only: pixels depend on the OS font stack and rasteriser, so the
 * committed baselines (`visual.spec.ts-snapshots/*-chromium-win32.png`) only match the platform that made
 * them.
 * Windows baselines are for local runs. The linux baselines (`*-chromium-linux.png`) are the CI reference and come
 * from the manual `visual-baselines` job in .github/workflows/e2e.yml (L-20, D-275): dispatch it, download the
 * `visual-baselines-linux` artifact, commit the PNGs. Refresh them in the same PR as a Playwright or Chromium bump
 * or an intended change to a captured page. Until they exist, CI skips the file (no SR_E2E_VISUAL there).
 * See docs/dev/handoff/feat-p16-e2e.md.
 */
test.skip(
  process.env.SR_E2E_VISUAL !== '1',
  'Visual snapshots are opt-in: set SR_E2E_VISUAL=1 (baselines are per platform).',
);
test.skip(({ browserName }) => browserName !== 'chromium', 'Baselines exist for chromium only.');

test.use({ viewport: { width: 1280, height: 800 }, locale: 'en-US', timezoneId: 'UTC' });

const SHOT = { animations: 'disabled', caret: 'hide', maxDiffPixelRatio: 0.01 } as const;

async function settle(page: Page) {
  await page.evaluate(() => document.fonts.ready);
  await page.addStyleTag({
    content:
      '*,*::before,*::after{transition:none!important;animation:none!important;caret-color:transparent!important}',
  });
}

test.describe('Entrance', () => {
  for (const [name, path, ready] of [
    ['landing', '/', '#invite-code'],
    ['login', '/login', 'text=Step inside'],
    ['forgot', '/forgot', 'form'],
    ['no-invitation', '/no-invitation', 'main, body'],
    ['house-notes', '/house-notes', 'main, body'],
  ] as const) {
    test(`${name} page`, async ({ page }) => {
      await page.goto(path);
      await page.locator(ready).first().waitFor();
      await settle(page);
      await expect(page).toHaveScreenshot(`entrance-${name}.png`, SHOT);
    });
  }
});

test.describe('Cinema and Library (seeded)', () => {
  const TITLE = 'Visual Baseline Film';

  test.beforeEach(async ({ page }) => {
    const { owner } = bootstrapState();
    await loginApi(page.request, owner);
    const found = (await (
      await page.request.get(`/api/v1/admin/cinema/titles?q=${encodeURIComponent(TITLE)}`)
    ).json()) as { items?: { title: string }[] };
    if (!found.items?.some((t) => t.title === TITLE)) {
      const t = await apiPost<{ id: string }>(page.request, '/admin/cinema/titles', {
        kind: 'movie',
        title: TITLE,
        year: 1999,
        runtime_min: 100,
        synopsis: 'A fixed synopsis used only for visual regression baselines.',
        genres: ['Drama'],
      });
      await page.request.put(`/api/v1/admin/cinema/titles/${t.id}/asset`, {
        data: { asset_id: '4242' },
        headers: {
          'X-SR-CSRF': (
            (await (await page.request.get('/api/v1/sr/csrf')).json()) as { token: string }
          ).token,
        },
      });
      await apiPost(page.request, `/admin/cinema/titles/${t.id}/publish`);
    }
  });

  for (const [name, path] of [
    ['cinema-discover', '/cinema'],
    ['cinema-movies', '/cinema/movies'],
    ['library-home', '/library'],
    ['library-browse', '/library/browse'],
  ] as const) {
    test(name, async ({ page }) => {
      await page.goto(path);
      await page.waitForLoadState('networkidle');
      await settle(page);
      await expect(page).toHaveScreenshot(`${name}.png`, SHOT);
    });
  }
});
