import { readFileSync } from 'node:fs';
import { expect, test, type APIRequestContext, type Page } from '@playwright/test';
import { STATE_FILE, type BootstrapState } from '../global-setup';
import { createInvite } from '../support/cli';
import { openTheDoor, tickConfirmations } from '../support/register';
import { nextWindowCode, secretFromOtpauth, totp } from '../support/totp';

/**
 * Phase 0 exit gate (docs/13): a fresh install can be bootstrapped; the Owner can invite a friend; the
 * friend can register, log in with 2FA, and see an empty Lounge.
 * Bootstrapping happens in global-setup.ts (sr-cli migrate + bootstrap); invites come from `sr-cli invite
 * create` because `POST /sr/invites` is not implemented yet (docs/05 section 5.1 lists only the check).
 */

const state = (): BootstrapState => JSON.parse(readFileSync(STATE_FILE, 'utf8')) as BootstrapState;

const API = '/api/v1';

async function csrf(request: APIRequestContext): Promise<string> {
  const res = await request.get(`${API}/sr/csrf`);
  expect(res.ok()).toBeTruthy();
  return ((await res.json()) as { token: string }).token;
}

async function expectEmptyLounge(page: Page) {
  await expect(page).toHaveURL(/\/channels\/@me/);
  await expect(page.getByRole('navigation', { name: 'Servers' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Friends' })).toBeVisible();
  await expect(page.getByText('No conversations yet.')).toBeVisible();
}

test.describe('Phase 0 exit gate', () => {
  test('bootstrap produced a one-time Owner link that signs the Owner in', async ({
    request,
    browserName,
  }) => {
    test.skip(browserName !== 'chromium', 'The link is single use; check it once per run.');
    const { loginLink } = state();
    test.skip(loginLink === null, 'Owner already existed; the one-time link is not available.');
    const token = new URL(loginLink!).searchParams.get('token');
    expect(token).toBeTruthy();
    const res = await request.post(`${API}/sr/auth/login-link`, { data: { token } });
    expect(res.ok()).toBeTruthy();
    expect(await res.json()).toMatchObject({ status: 'ok' });
    const me = await request.get(`${API}/users/@me`);
    expect(me.ok()).toBeTruthy();
  });

  test('Owner signs in with the bootstrap password', async ({ page }) => {
    const { owner } = state();
    await page.goto('/login');
    await page.getByLabel('Username or email').fill(owner.username);
    await page.getByLabel('Password', { exact: true }).fill(owner.password);
    await page.getByRole('button', { name: 'Step inside' }).click();
    await expect(page).toHaveURL(/\/channels/);
  });

  test('Owner invites a friend, who registers, logs in with TOTP and lands on an empty Lounge', async ({
    page,
  }) => {
    const stamp = Date.now().toString(36);
    const friend = {
      username: `friend_${stamp}`,
      email: `friend_${stamp}@example.test`,
      password: 'Friend-Passw0rd-9!',
    };

    // The Owner creates an invite.
    const code = createInvite(`e2e ${stamp}`);

    // The friend enters it on the landing page and registers.
    await page.goto('/');
    await page.locator('#invite-code').fill(code);
    await page.getByRole('button', { name: 'Check invitation' }).click();
    await expect(page.getByText("You're on the list.")).toBeVisible();
    await expect(page).toHaveURL(/\/register/);

    await page.getByLabel('Email').fill(friend.email);
    await page.getByLabel('Username').fill(friend.username);
    await page.getByLabel('Password', { exact: true }).fill(friend.password);
    await page.getByLabel('Confirm password').fill(friend.password);
    await tickConfirmations(page);
    await openTheDoor(page);

    // Enrol TOTP through the API using the otpauth secret (cookie session + CSRF header).
    const request = page.request;
    const enroll = await request.post(`${API}/sr/auth/totp/enroll`, {
      headers: { 'X-SR-CSRF': await csrf(request) },
    });
    expect(enroll.ok()).toBeTruthy();
    const secret = secretFromOtpauth(
      ((await enroll.json()) as { otpauth_uri: string }).otpauth_uri,
    );
    const verify = await request.post(`${API}/sr/auth/totp/verify`, {
      headers: { 'X-SR-CSRF': await csrf(request) },
      data: { code: totp(secret) },
    });
    expect(verify.ok()).toBeTruthy();
    expect(((await verify.json()) as { recovery_codes: string[] }).recovery_codes).toHaveLength(10);

    // Sign out, then sign in again through the UI with password + TOTP.
    const out = await request.post(`${API}/sr/auth/logout`, {
      headers: { 'X-SR-CSRF': await csrf(request) },
    });
    expect(out.status()).toBe(204);

    await page.goto('/login');
    await page.getByLabel('Username or email').fill(friend.username);
    await page.getByLabel('Password', { exact: true }).fill(friend.password);
    await page.getByRole('button', { name: 'Step inside' }).click();
    await expect(page).toHaveURL(/\/login\/2fa/);

    // The enrolment code is spent for this window; use the next one (replays are rejected).
    const digits = await nextWindowCode(secret);
    await page.getByLabel('Digit 1').click();
    await page.keyboard.type(digits);

    await expectEmptyLounge(page);
    await expect(page.getByRole('link', { name: /board/i })).toHaveCount(0);
  });
});
