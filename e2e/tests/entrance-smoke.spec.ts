import { expect, test } from '@playwright/test';

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

test('/register without an invite bounces to the landing page', async ({ page }) => {
  await page.goto('/register');
  await expect(page).toHaveURL(/\/$/);
});

test('there is no Board anywhere in the entrance', async ({ page }) => {
  for (const path of ['/', '/login', '/no-invitation']) {
    await page.goto(path);
    await expect(page.getByRole('link', { name: /board/i })).toHaveCount(0);
    await expect(page.locator('a[href*="board" i]')).toHaveCount(0);
  }
  const res = await page.goto('/board');
  expect(res?.ok()).toBeTruthy(); // the SPA answers every path...
  await expect(page.getByText('Page not found.')).toBeVisible(); // ...but no Board route exists
});
