import { request as pwRequest, type Request } from '@playwright/test';
import { PUBLIC_URL } from '../support/cli';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * UX-SHOW-001 (wave 45-B, gate proof): "Showroom mode toggles cleanly without mutating backend state".
 *
 * The operator switch `platform.showroom` (OFF by default) is switched on with the Owner's session for this file and
 * off again afterwards. A member then turns the preview on, uses the sample servers, shelves and games, turns it off,
 * and the spec proves:
 *
 *   - the toggle is absent while the switch is off, and present once it is on;
 *   - while the preview shows, the page sends no write at all and no read of the Arcade front page or the shelves;
 *   - a "Preview" banner is on screen, the rail carries ten sample servers, The Study separates sources, and The Arcade
 *     separates "PLAYABLE NOW" from "PREVIEW / IN DEVELOPMENT";
 *   - mutation attempts on sample objects only show the hint;
 *   - API reads taken before the toggle equal the reads taken after it (nothing on the server moved).
 *
 * Run (the integrator, with the e2e stack up): `pnpm --filter @sideroom/e2e exec playwright test showroom.spec.ts`
 * (or `pnpm exec playwright test e2e/tests/showroom.spec.ts` from the repository root, as CI does).
 */
const SWITCH = 'platform.showroom';
/** The reads whose answers must be identical before and after the preview. */
const SNAPSHOT = [
  '/api/v1/users/@me',
  '/api/v1/users/@me/guilds',
  '/api/v1/users/@me/relationships',
  '/api/v1/shelves/visible',
  '/api/v1/sr/features/flags',
];

async function setSwitch(enabled: boolean): Promise<void> {
  const ctx = await pwRequest.newContext({ baseURL: PUBLIC_URL });
  try {
    await loginApi(ctx, bootstrapState().owner);
    await apiPatch(ctx, `/admin/features/${SWITCH}`, {
      enabled,
      reason: enabled ? 'e2e: showroom spec' : 'e2e: showroom spec done',
    });
  } finally {
    await ctx.dispose();
  }
}

test.describe('Showroom (populated preview) @showroom', () => {
  test.beforeAll(() => setSwitch(true));
  test.afterAll(() => setSwitch(false));

  test('Showroom mode toggles cleanly without mutating backend state', async ({
    seedUser,
    signedIn,
  }) => {
    const { page, context } = await signedIn(await seedUser('showroom'));
    const snapshot = async () => {
      const out: Record<string, unknown> = {};
      for (const path of SNAPSHOT) {
        const res = await context.request.get(path);
        out[path] = res.ok() ? await res.json() : res.status();
      }
      return out;
    };
    const before = await snapshot();

    await page.goto('/study');
    const toggle = page.getByTestId('showroom-toggle');
    await expect(toggle).toBeVisible();
    await expect(toggle).toHaveAttribute('aria-pressed', 'false');
    await expect(page.getByTestId('showroom-banner')).toHaveCount(0);

    // From here until the toggle is turned off again, nothing but the app's own static files may be requested.
    const sent: Request[] = [];
    await toggle.click();
    await expect(page.getByTestId('showroom-banner')).toBeVisible();
    page.on('request', (r) => sent.push(r));

    // The Study: sources and the local metadata view.
    const study = page.getByTestId('showroom-study');
    await expect(study).toBeVisible();
    await study.getByRole('button', { name: 'Friends' }).click();
    const items = study.getByRole('list', { name: 'Sample items' });
    await expect(items.getByRole('button')).toHaveCount(6);
    await study.getByRole('button', { name: 'Servers' }).click();
    await expect(items.getByRole('button')).toHaveCount(5);
    await items
      .getByRole('button', { name: /Harbor Lights/ })
      .first()
      .click();
    const detail = page.getByRole('complementary', { name: 'Item details' });
    await expect(detail).toContainText('kept on this device only');
    await detail.getByRole('button', { name: 'Pin' }).click();
    await expect(page.getByTestId('showroom-hint')).toContainText('nothing was opened, pinned');

    // The Arcade: live builds apart from showcase cards.
    await page
      .getByRole('navigation', { name: 'Rooms' })
      .getByRole('link', { name: 'The Arcade' })
      .click();
    const arcade = page.getByTestId('showroom-arcade');
    await expect(arcade).toBeVisible();
    const live = arcade.getByRole('region', { name: 'PLAYABLE NOW' });
    const soon = arcade.getByRole('region', { name: 'PREVIEW / IN DEVELOPMENT' });
    await expect(live.getByRole('article')).toHaveCount(3);
    await expect(soon.getByRole('article')).toHaveCount(4);
    await live.getByRole('button', { name: 'Play' }).first().click();
    await expect(page.getByTestId('showroom-hint')).toContainText('no game was started');

    // The rail: ten sample servers with voice and unread cues.
    await page.goto('/channels');
    await expect(page.getByTestId('showroom-banner')).toBeVisible();
    const samples = page.getByRole('button', { name: /preview sample/ });
    await expect(samples).toHaveCount(10);
    await samples.first().click();
    await expect(page.getByTestId('showroom-hint')).toContainText('preview sample');

    // Nothing the sample screens did reached the server (the /channels load itself reads the real app's data).
    // Compare URLs (a Request prints as an opaque guid); the dev server's module files under /src are not API calls.
    expect(
      sent
        .filter((r) => !['GET', 'HEAD'].includes(r.method()))
        // Firefox posts CSP reports about the Vite dev server's own scripts; they are browser telemetry, not app writes.
        .filter((r) => !r.url().includes('/internal/csp-report'))
        .map((r) => `${r.method()} ${r.url()}`),
      'no write while the preview shows',
    ).toEqual([]);
    expect(
      sent.map((r) => r.url()).filter((u) => /\/api\/v1\/(?:arcade\/front|shelves\/)/.test(u)),
    ).toEqual([]);

    // Off again: the banner and the samples are gone, the device choice is cleared, and the server reads are unchanged.
    await page.getByRole('navigation', { name: 'Rooms' }).getByTestId('showroom-toggle').click();
    await expect(page.getByTestId('showroom-banner')).toHaveCount(0);
    await expect(page.getByRole('button', { name: /preview sample/ })).toHaveCount(0);
    expect(await page.evaluate(() => localStorage.getItem('sr.showroom.v1'))).toBeNull();
    expect(await snapshot()).toEqual(before);
  });

  test('the toggle is not offered while the deployment switch is off', async ({
    seedUser,
    signedIn,
  }) => {
    await setSwitch(false);
    try {
      const { page } = await signedIn(await seedUser('showroomoff'));
      await page.goto('/study');
      await expect(page.getByTestId('showroom-study')).toHaveCount(0);
      await expect(page.getByTestId('showroom-toggle')).toHaveCount(0);
      await expect(page.getByTestId('showroom-banner')).toHaveCount(0);
    } finally {
      await setSwitch(true);
    }
  });
});
