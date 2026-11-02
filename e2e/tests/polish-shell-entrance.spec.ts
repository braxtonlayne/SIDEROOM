import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { repoRoot } from '../support/cli';

// Polish round (XC-VIS-002, shell): the signed-out entrance flows a real visitor takes, on a phone, against the
// running sweep stack. Opt-in like the page sweep; screenshots go outside git.
const ON = process.env.SR_E2E_SWEEP === '1';
test.skip(!ON, 'Opt-in: set SR_E2E_SWEEP=1.');
const OUT = process.env.SR_SWEEP_OUT ?? resolve(repoRoot, '../visual-sweep-shots/shell');
test.beforeAll(() => mkdirSync(OUT, { recursive: true }));

test.use({ baseURL: 'http://localhost:5173', viewport: { width: 375, height: 812 } });

test('wrong password shows an error next to the form', async ({ page }) => {
  await page.goto('/login');
  await page.getByLabel(/username or email/i).fill('owner');
  await page.getByLabel(/^password$/i).fill('definitely-not-it-123');
  await page.getByRole('button', { name: /step inside/i }).click();
  await expect(page.getByRole('alert').first()).toBeVisible({ timeout: 10_000 });
  await page.screenshot({ path: `${OUT}/flow-login-error.png`, fullPage: true });
});

test('forgot password confirms without saying whether the email exists', async ({ page }) => {
  await page.goto('/forgot');
  await page.getByLabel(/email/i).fill('nobody@example.test');
  await page.getByRole('button', { name: /send the link/i }).click();
  await page.waitForTimeout(1500);
  await page.screenshot({ path: `${OUT}/flow-forgot-sent.png`, fullPage: true });
});

test('a bad invitation code on the landing page says so', async ({ page }) => {
  await page.goto('/');
  await page.waitForLoadState('networkidle');
  await page.getByPlaceholder(/invite code/i).pressSequentially('zzzzz99999', { delay: 30 });
  await page.getByRole('button', { name: /check invitation/i }).click();
  await page.waitForTimeout(1500);
  await page.screenshot({ path: `${OUT}/flow-landing-bad-code.png`, fullPage: true });
});
