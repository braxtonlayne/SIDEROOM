import { expect, test, type APIRequestContext, type CDPSession, type Page } from '@playwright/test';
import { createInvite } from '../support/cli';
import { openTheDoor, tickConfirmations } from '../support/register';

/**
 * Passkeys end to end (ENT-AUTH-010, Q83): Chromium's CDP WebAuthn virtual authenticator registers a
 * passkey from Settings, then signs in with it as a passkey-only login and as the second step of a
 * password login, against the real sr-server (pure-Rust verifier) and the real Postgres.
 *
 * Chromium only: the virtual authenticator is a CDP feature. Firefox and WebKit have no equivalent.
 */

const API = '/api/v1';

async function csrf(request: APIRequestContext): Promise<string> {
  const res = await request.get(`${API}/sr/csrf`);
  expect(res.ok()).toBeTruthy();
  return ((await res.json()) as { token: string }).token;
}

/** A platform-style authenticator with resident keys and user verification, always approving. */
async function virtualAuthenticator(page: Page): Promise<{ cdp: CDPSession; id: string }> {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('WebAuthn.enable');
  const { authenticatorId } = await cdp.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2',
      transport: 'internal',
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  });
  return { cdp, id: authenticatorId };
}

async function register(page: Page, friend: { username: string; email: string; password: string }) {
  const code = createInvite(`passkeys ${friend.username}`);
  await page.goto('/');
  await page.locator('#invite-code').fill(code);
  await page.getByRole('button', { name: 'Check invitation' }).click();
  await expect(page).toHaveURL(/\/register/);
  await page.getByLabel('Email').fill(friend.email);
  await page.getByLabel('Username').fill(friend.username);
  await page.getByLabel('Password', { exact: true }).fill(friend.password);
  await page.getByLabel('Confirm password').fill(friend.password);
  await tickConfirmations(page);
  await openTheDoor(page);
}

async function logout(page: Page) {
  const res = await page.request.post(`${API}/sr/auth/logout`, {
    headers: { 'X-SR-CSRF': await csrf(page.request) },
  });
  expect(res.status()).toBe(204);
}

test.describe('Passkeys (virtual authenticator)', () => {
  test.skip(({ browserName }) => browserName !== 'chromium', 'CDP WebAuthn is Chromium only.');

  test('register in Settings, sign in with it, use it as a second step, remove it', async ({
    page,
  }) => {
    const stamp = Date.now().toString(36);
    const friend = {
      username: `pk_${stamp}`,
      email: `pk_${stamp}@example.test`,
      password: 'Friend-Passw0rd-9!',
    };
    const auth = await virtualAuthenticator(page);
    const credentials = async () =>
      (await auth.cdp.send('WebAuthn.getCredentials', { authenticatorId: auth.id })).credentials;

    await register(page, friend);

    // Settings > My Account > Passkeys: add one (password re-check, then the browser prompt).
    await page.goto('/settings/user/account');
    const card = page.locator('#us-passkeys');
    await expect(card.getByText('No passkeys yet.')).toBeVisible();
    await card.getByRole('button', { name: 'Add a passkey' }).click();
    const dialog = page.getByRole('dialog', { name: 'Add a passkey' });
    await dialog.getByLabel('Name').fill('Virtual key');
    // A wrong password is refused before the authenticator is touched.
    await dialog.getByLabel('Current password').fill('definitely-wrong-password');
    await dialog.getByRole('button', { name: 'Add passkey' }).click();
    await expect(dialog.getByText("That password doesn't match.")).toBeVisible();
    expect(await credentials()).toHaveLength(0);
    await dialog.getByLabel('Current password').fill(friend.password);
    await dialog.getByRole('button', { name: 'Add passkey' }).click();

    // The first second factor comes with recovery codes, shown once.
    const codes = page.getByRole('dialog', { name: 'Your recovery codes' });
    await expect(codes).toBeVisible();
    await expect(
      codes.getByRole('list', { name: 'Recovery codes' }).getByRole('listitem'),
    ).toHaveCount(10);
    await codes.getByRole('button', { name: 'I have saved them' }).click();
    await expect(card.locator('strong', { hasText: 'Virtual key' })).toBeVisible();
    expect(await credentials()).toHaveLength(1);

    // Rename.
    await card.getByRole('button', { name: /Rename/ }).click();
    const rename = page.getByRole('dialog', { name: 'Rename passkey' });
    await rename.getByLabel('Name').fill('Desk key');
    await rename.getByRole('button', { name: 'Save' }).click();
    await expect(card.locator('strong', { hasText: 'Desk key' })).toBeVisible();

    // Passkey-only sign-in: no username, no password.
    await logout(page);
    await page.goto('/login');
    await page.getByRole('button', { name: 'Use a passkey' }).click();
    await expect(page).toHaveURL(/\/channels/);
    expect((await page.request.get(`${API}/users/@me`)).ok()).toBeTruthy();

    // The signature counter moved: the list shows the key as used.
    await page.goto('/settings/user/account');
    await expect(card.getByText(/last used/)).toBeVisible();

    // Password login now asks for a second step, and the passkey answers it.
    await logout(page);
    await page.goto('/login');
    await page.getByLabel('Username or email').fill(friend.username);
    await page.getByLabel('Password', { exact: true }).fill(friend.password);
    await page.getByRole('button', { name: 'Step inside' }).click();
    await expect(page).toHaveURL(/\/login\/2fa/);
    await page.getByRole('button', { name: 'Use your passkey' }).click();
    await expect(page).toHaveURL(/\/channels/);

    // A cloned authenticator (signature counter went back) is refused.
    const setCounter = async (signCount: number) => {
      const [cred] = await credentials();
      await auth.cdp.send('WebAuthn.removeCredential', {
        authenticatorId: auth.id,
        credentialId: cred!.credentialId,
      });
      await auth.cdp.send('WebAuthn.addCredential', {
        authenticatorId: auth.id,
        credential: { ...cred!, signCount },
      });
    };
    await setCounter(0);
    await logout(page);
    await page.goto('/login');
    await page.getByRole('button', { name: 'Use a passkey' }).click();
    await expect(
      page.getByRole('alert').filter({ hasText: "That passkey didn't work" }),
    ).toBeVisible();
    await setCounter(10_000);
    await page.getByRole('button', { name: 'Use a passkey' }).click();
    await expect(page).toHaveURL(/\/channels/);

    // Remove it from Settings.
    await page.goto('/settings/user/account');
    await card.getByRole('button', { name: /Remove/ }).click();
    const remove = page.getByRole('dialog', { name: 'Remove passkey' });
    await remove.getByLabel('Current password').fill(friend.password);
    await remove.getByRole('button', { name: 'Remove' }).click();
    await expect(card.getByText('No passkeys yet.')).toBeVisible();

    // Without the passkey the key on the device no longer works.
    await logout(page);
    await page.goto('/login');
    await page.getByRole('button', { name: 'Use a passkey' }).click();
    await expect(
      page.getByRole('alert').filter({ hasText: "That passkey didn't work" }),
    ).toBeVisible();
    await expect(page).toHaveURL(/\/login/);
  });
});
