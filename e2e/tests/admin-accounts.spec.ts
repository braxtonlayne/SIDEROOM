import { readFileSync } from 'node:fs';
import { expect, test } from '@playwright/test';
import { STATE_FILE, type BootstrapState } from '../global-setup';
import { createInvite, runCli } from '../support/cli';
import { apiPost, registerUser, signIn } from '../support/fixtures';
import { nextWindowCode, secretFromOtpauth, totp } from '../support/totp';

/**
 * Admin pages against a real server: a staff member finds an account, suspends it with a reason, sees it
 * deactivated, then reactivates it. The friend account is created through the public API with an invite
 * from `sr-cli invite create` (admin-smoke.mjs covers the API surface; this covers the UI path).
 *
 * Suspension needs a fresh second factor (D-574), so the staff member is a Senior Staff account with TOTP
 * (appointed with `sr-cli`), and the "Confirm it is you" dialog opens on the first suspension when the server
 * demands the window (the default; `SR_ADMIN_REQUIRE_REAUTH=0` skips it).
 */

const state = (): BootstrapState => JSON.parse(readFileSync(STATE_FILE, 'utf8')) as BootstrapState;
const API = '/api/v1';

test('Staff suspends and reactivates an account from the admin pages', async ({
  browser,
  request,
  baseURL,
}) => {
  test.setTimeout(120_000);
  const stamp = Date.now().toString(36);
  const username = `adm_${stamp}`;
  const origin = baseURL ?? 'http://localhost:5173';

  const staff = await registerUser(request, 'admstaff');
  runCli([
    'staff',
    'appoint',
    staff.username,
    '--role',
    'Senior Staff',
    '--reason',
    'e2e admin accounts',
  ]);
  const S = await signIn(browser, staff);
  const enroll = await apiPost<{ otpauth_uri: string }>(S.context.request, '/sr/auth/totp/enroll', {
    password: staff.password,
  });
  const secret = secretFromOtpauth(enroll.otpauth_uri);
  await apiPost(S.context.request, '/sr/auth/totp/verify', { code: totp(secret) });
  const page = S.page;

  const res = await request.post(`${API}/sr/register`, {
    headers: { Origin: origin },
    data: {
      code: createInvite(`admin e2e ${stamp}`),
      email: `${username}@example.test`,
      username,
      password: 'Admin-E2e-Passw0rd-9!',
      accepted_house_notes: true,
      confirmed_min_age: 13,
    },
  });
  expect(res.ok(), await res.text()).toBeTruthy();

  await page.goto('/admin/accounts');
  await expect(page.getByRole('heading', { name: 'Accounts' })).toBeVisible();
  await page.getByLabel('Search accounts').fill(username);
  await page.getByRole('button', { name: 'Search' }).click();
  await page.getByRole('button', { name: `View ${username}` }).click();

  // The reason is mandatory: the confirm button stays disabled until one is typed.
  await page.getByRole('button', { name: 'Suspend', exact: true }).click();
  const suspend = page.getByRole('dialog', { name: `Suspend @${username}` });
  await expect(suspend.getByRole('button', { name: 'Suspend account' })).toBeDisabled();
  await suspend.getByLabel(/Reason/).fill('e2e: spam wave');
  await suspend.getByRole('button', { name: 'Suspend account' }).click();
  const done = page.getByText(`@${username} was suspended.`);
  const confirm = page.getByRole('dialog', { name: 'Confirm it is you' });
  await expect(done.or(confirm)).toBeVisible();
  if (await confirm.isVisible()) {
    await confirm.getByLabel('Code from your authenticator app').fill(await nextWindowCode(secret));
    await confirm.getByRole('button', { name: 'Confirm' }).click();
  }
  await expect(done).toBeVisible();

  await page.getByRole('button', { name: 'Reactivate' }).click();
  const back = page.getByRole('dialog', { name: `Reactivate @${username}` });
  await back.getByLabel(/Reason/).fill('e2e: appeal accepted');
  await back.getByRole('button', { name: 'Reactivate account' }).click();
  await expect(page.getByText(`@${username} was reactivated.`)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Suspend', exact: true })).toBeVisible();
  await S.context.close();
});

test('Owner opens the access log and verifies the chain', async ({ page }) => {
  const { owner } = state();
  await page.goto('/login');
  await page.getByLabel('Username or email').fill(owner.username);
  await page.getByLabel('Password', { exact: true }).fill(owner.password);
  await page.getByRole('button', { name: 'Step inside' }).click();
  await expect(page).toHaveURL(/\/channels/);

  await page.goto('/admin/access-log');
  await expect(page.getByRole('heading', { name: 'Access log' })).toBeVisible();
  await page.getByRole('button', { name: 'Verify chain' }).click();
  await expect(page.getByText('Chain intact')).toBeVisible();
});
