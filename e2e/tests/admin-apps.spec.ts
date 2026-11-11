import { expect, test } from '@playwright/test';
import { runCli } from '../support/cli';
import { apiPost, registerUser, signIn } from '../support/fixtures';
import { nextWindowCode, secretFromOtpauth, totp } from '../support/totp';

/**
 * SEC-AUD-057 (D-587): an App Reviewer finds a developer's application in the Review hub's Applications tab,
 * suspends it with a reason (the "Confirm it is you" dialog opens when the server demands the second-factor
 * window), sees the reason, who and when on the row, and unsuspends it. The developer gets a system DM on each.
 */
test('Staff suspends and unsuspends an application from the Review hub', async ({ browser }) => {
  test.setTimeout(120_000);
  const dev = await registerUser((await browser.newContext()).request, 'appdev');
  const D = await signIn(browser, dev);
  const name = `Relay ${Date.now().toString(36)}`;
  await apiPost(D.context.request, '/applications', { name, description: 'e2e relay' });

  const staff = await registerUser(D.context.request, 'appstaff');
  runCli(['staff', 'appoint', staff.username, '--role', 'App Reviewer', '--reason', 'e2e apps']);
  const S = await signIn(browser, staff);
  const enroll = await apiPost<{ otpauth_uri: string }>(S.context.request, '/sr/auth/totp/enroll', {
    password: staff.password,
  });
  const secret = secretFromOtpauth(enroll.otpauth_uri);
  await apiPost(S.context.request, '/sr/auth/totp/verify', { code: totp(secret) });
  const page = S.page;

  await page.goto('/admin/review');
  await page.getByRole('tab', { name: 'Applications' }).click();
  await page.getByLabel('Search', { exact: true }).fill(name);
  await page.getByRole('button', { name: 'Search', exact: true }).click();
  await expect(page.getByText(name, { exact: true })).toBeVisible();
  await expect(page.getByText(`@${dev.username}`)).toBeVisible();

  // The reason is mandatory.
  await page.getByRole('button', { name: `Suspend ${name}` }).click();
  const suspend = page.getByRole('dialog', { name: `Suspend ${name}` });
  await expect(suspend.getByRole('button', { name: 'Suspend application' })).toBeDisabled();
  await suspend.getByLabel(/Reason/).fill('e2e: relays unsolicited invites');
  await suspend.getByRole('button', { name: 'Suspend application' }).click();
  const done = page.getByText(`${name} was suspended.`);
  const confirm = page.getByRole('dialog', { name: 'Confirm it is you' });
  await expect(done.or(confirm)).toBeVisible();
  if (await confirm.isVisible()) {
    await confirm.getByLabel('Code from your authenticator app').fill(await nextWindowCode(secret));
    await confirm.getByRole('button', { name: 'Confirm' }).click();
  }
  await expect(done).toBeVisible();
  await expect(page.getByText('Reason: e2e: relays unsolicited invites')).toBeVisible();
  await expect(page.getByText(`Suspended by @${staff.username} on`)).toBeVisible();

  // The state filter finds it, and the developer was told.
  await page.getByRole('combobox', { name: 'Release state' }).click();
  await page.getByRole('option', { name: 'Suspended' }).click();
  await expect(page.getByText(name, { exact: true })).toBeVisible();
  const dms = await D.context.request.get('/api/v1/users/@me/channels');
  expect(await dms.text()).toContain('sideroom');

  await page.getByRole('button', { name: `Unsuspend ${name}` }).click();
  const lift = page.getByRole('dialog', { name: `Unsuspend ${name}` });
  await lift.getByRole('button', { name: 'Unsuspend application' }).click();
  await expect(page.getByText(`${name} was unsuspended.`)).toBeVisible();

  await S.context.close();
  await D.context.close();
});
