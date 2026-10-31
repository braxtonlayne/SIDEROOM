import { expect, test } from '@playwright/test';

/**
 * OPS-INV-001 / D-385: "Lock the Doors". The entrance's one change is a notice that the doors are closed. The lock is
 * a server-wide operator state that would stop every other spec from registering on a shared database, so the two
 * answers that carry it are stubbed (the server side, the refusal and the audit rows, is proven by the Postgres tests
 * of OPS-INV-001).
 */
test.describe('stubbed API', () => {
  // See entrance-smoke.spec.ts: `page.route` cannot see requests made through the service worker.
  test.use({ serviceWorkers: 'block' });

  test('the landing page says the doors are closed while they are locked', async ({ page }) => {
    await page.route('**/api/v1/sr/compliance/public', (route) =>
      route.fulfill({ json: { minimum_age: 13, invite_initial_credits: 5, doors_closed: true } }),
    );
    await page.goto('/');
    await expect(page.getByText(/The doors are closed right now/)).toBeVisible();
    // Nothing else about the entrance changed: the invite field and the way to sign in are still there.
    await expect(page.locator('#invite-code')).toBeVisible();
    await expect(page.getByRole('link', { name: 'No invitation?' })).toBeVisible();
  });

  test('a code checked while the doors are locked gets the same words', async ({ page }) => {
    await page.route('**/api/v1/sr/invites/*', (route) =>
      route.fulfill({ json: { valid: false, reason: 'locked' } }),
    );
    await page.goto('/');
    await page.locator('#invite-code').fill('ZZZZZ-ZZZZZ');
    await page.getByRole('button', { name: 'Check invitation' }).click();
    await expect(page.getByText(/The doors are closed right now/)).toBeVisible();
    await expect(page).toHaveURL(/\/$/);
  });

  test('the notice is absent while the doors are open', async ({ page }) => {
    await page.route('**/api/v1/sr/compliance/public', (route) =>
      route.fulfill({ json: { minimum_age: 13, invite_initial_credits: 5, doors_closed: false } }),
    );
    await page.goto('/');
    await expect(page.locator('#invite-code')).toBeVisible();
    await expect(page.getByText(/The doors are closed right now/)).toHaveCount(0);
  });
});
