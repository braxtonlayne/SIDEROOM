import { expect, test, type Page } from '@playwright/test';

/** Entrance smoke tests (ENT-UI-026). They need the API only for the "invalid invite" check. */

test('landing renders the invite form and a door to sign in', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#invite-code')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Check invitation' })).toBeDisabled();
  await expect(page.getByRole('link', { name: 'No invitation?' })).toBeVisible();
});

test('an unknown invite code is refused', async ({ page }) => {
  await page.goto('/');
  await page.locator('#invite-code').fill('ZZZZZ-ZZZZZ');
  await page.getByRole('button', { name: 'Check invitation' }).click();
  await expect(page.getByText("That invitation doesn't open this door.")).toBeVisible();
  await expect(page).toHaveURL(/\/$/);
});

test.describe('stubbed API', () => {
  // `page.route` cannot see a request made from a page a service worker controls (WebKit routes every fetch of
  // such a page through the worker), and the built bundle registers one on every public page. The worker is not
  // what this test is about, so it is kept out; pwa-*.spec.ts cover the worker.
  test.use({ serviceWorkers: 'block' });

  test('a closed door (cap reached) is explained', async ({ page }) => {
    // The cap is a server-wide state that is awkward to reach on a shared database, so the response is stubbed.
    await page.route('**/api/v1/sr/invites/*', (route) =>
      route.fulfill({ json: { valid: false, reason: 'closed' } }),
    );
    await page.goto('/');
    await page.locator('#invite-code').fill('ZZZZZ-ZZZZZ');
    await page.getByRole('button', { name: 'Check invitation' }).click();
    await expect(page.getByText("The door's closed for now. We're at capacity.")).toBeVisible();
  });
});

test('/register without an invite bounces to the landing page', async ({ page }) => {
  await page.goto('/register');
  await expect(page).toHaveURL(/\/$/);
});

/**
 * Same-origin links whose own path names a Board. Only the path counts: after `/board` the sign-in page carries the
 * visitor's requested path back in `?next=%2Fboard` (on the Discord button, for instance), which is the visitor's
 * own URL and not a link to a Board.
 */
const boardLinks = (page: Page) =>
  page.locator('a[href]').evaluateAll((as) =>
    as
      .map((a) => new URL((a as HTMLAnchorElement).href, location.href))
      .filter((u) => u.origin === location.origin && /board/i.test(u.pathname))
      .map((u) => u.pathname),
  );

test('there is no Board anywhere in the entrance', async ({ page }) => {
  for (const path of ['/', '/login', '/no-invitation']) {
    await page.goto(path);
    await expect(page.getByRole('link', { name: /board/i })).toHaveCount(0);
    expect(await boardLinks(page)).toEqual([]);
  }
  // D-385: a path that is not a public surface is not served to a visitor at all. The server answers 302 /login?next=
  // (the built bundle, SR_E2E_WEB=server) and the dev server does the same through vite/publicDev.ts; either way the
  // visitor lands on the sign-in page and never on a Board, and nothing says whether a route exists.
  await page.goto('/board');
  await expect(page).toHaveURL(/\/login/);
  await expect(page.getByRole('link', { name: /board/i })).toHaveCount(0);
  expect(await boardLinks(page)).toEqual([]);
});
