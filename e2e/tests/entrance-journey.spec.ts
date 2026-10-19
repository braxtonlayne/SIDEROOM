import { expect, test, type CDPSession, type Page } from '@playwright/test';
import { createInvite } from '../support/cli';
import { mailedToken } from '../support/mailLog';
import { nextWindowCode, totp } from '../support/totp';

/**
 * ENT-UI-026, the whole Entrance journey through the UI in one account's life:
 * invite -> register -> verify email (link from the dev mail sink) -> enable an authenticator app
 * (manual key shown in Settings) -> log out -> password login with TOTP -> add a passkey (CDP virtual
 * authenticator) -> log out -> passkey login. The pieces are also covered separately by exit-gate.spec.ts
 * (API-driven TOTP) and passkeys.spec.ts (passkey lifecycle); this spec is the end-to-end chain.
 *
 * Chromium only: the virtual authenticator is a CDP feature.
 */

test.skip(({ browserName }) => browserName !== 'chromium', 'CDP WebAuthn is Chromium only.');

async function logOutViaUi(page: Page) {
  await page.goto('/settings/user/account');
  await page.getByRole('button', { name: 'Log Out' }).first().click();
  await page
    .getByRole('dialog', { name: 'Log out?' })
    .getByRole('button', { name: 'Log Out' })
    .click();
  await expect(page).toHaveURL(/\/login|\/$/);
}

async function virtualAuthenticator(page: Page): Promise<CDPSession> {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('WebAuthn.enable');
  await cdp.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2',
      transport: 'internal',
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  });
  return cdp;
}

test('invite, register, verify, TOTP login, passkey login', async ({ page }) => {
  const stamp = Date.now().toString(36);
  const me = {
    username: `jr_${stamp}`,
    email: `jr_${stamp}@example.test`,
    password: 'Journey-Passw0rd-9!',
  };
  await virtualAuthenticator(page);

  // Invite -> register.
  const code = createInvite(`journey ${stamp}`);
  await page.goto('/');
  await page.locator('#invite-code').fill(code);
  await page.getByRole('button', { name: 'Check invitation' }).click();
  await expect(page).toHaveURL(/\/register/);
  await page.getByLabel('Email').fill(me.email);
  await page.getByLabel('Username').fill(me.username);
  await page.getByLabel('Password', { exact: true }).fill(me.password);
  await page.getByLabel('Confirm password').fill(me.password);
  await page.getByLabel(/house notes/).check();
  await page.getByRole('button', { name: 'Open the door' }).click();
  await expect(page).toHaveURL(/\/channels/);

  // Verify the address through the emailed link.
  const token = await mailedToken(me.email, 'verify-email');
  await page.goto(`/verify-email/${token}`);
  await expect(
    page.getByRole('status').filter({ hasText: /verified|confirmed|thanks/i }),
  ).toBeVisible();

  // Enable the authenticator app: the manual entry key is the TOTP secret.
  await page.goto('/settings/user/account');
  await page.getByRole('button', { name: 'Enable authenticator app' }).click();
  const wizard = page.getByRole('dialog', { name: 'Enable authenticator app' });
  const secret = ((await wizard.getByLabel('Manual entry key').textContent()) ?? '').replace(
    /\s+/g,
    '',
  );
  expect(secret).toMatch(/^[A-Z2-7]{16,}$/);
  await wizard.getByLabel('Authentication code').fill(totp(secret));
  await wizard.getByRole('button', { name: 'Verify and enable' }).click();
  const codes = page.getByRole('dialog', { name: 'Your recovery codes' });
  await expect(
    codes.getByRole('list', { name: 'Recovery codes' }).getByRole('listitem'),
  ).toHaveCount(10);
  await codes.getByRole('button', { name: 'I have saved them' }).click();

  // Log out, then password + TOTP (the enrolment code is spent; wait for the next window).
  await logOutViaUi(page);
  await page.goto('/login');
  await page.getByLabel('Username or email').fill(me.username);
  await page.getByLabel('Password', { exact: true }).fill(me.password);
  await page.getByRole('button', { name: 'Step inside' }).click();
  await expect(page).toHaveURL(/\/login\/2fa/);
  const digits = await nextWindowCode(secret);
  await page.getByLabel('Digit 1').click();
  await page.keyboard.type(digits);
  await expect(page).toHaveURL(/\/channels/);

  // Add a passkey (password re-check), log out, sign in with it alone.
  await page.goto('/settings/user/account');
  const card = page.locator('#us-passkeys');
  await card.getByRole('button', { name: 'Add a passkey' }).click();
  const dialog = page.getByRole('dialog', { name: 'Add a passkey' });
  await dialog.getByLabel('Name').fill('Journey key');
  await dialog.getByLabel('Current password').fill(me.password);
  await dialog.getByRole('button', { name: 'Add passkey' }).click();
  await expect(card.locator('strong', { hasText: 'Journey key' })).toBeVisible();

  await logOutViaUi(page);
  await page.goto('/login');
  await page.getByRole('button', { name: 'Use a passkey' }).click();
  await expect(page).toHaveURL(/\/channels/);
  expect((await page.request.get('/api/v1/users/@me')).ok()).toBeTruthy();
});
