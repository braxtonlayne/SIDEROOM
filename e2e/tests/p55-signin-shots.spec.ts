import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page, Route } from '@playwright/test';
import { apiPatch, apiPost, expect, loginApi, registerUser, test } from '../support/fixtures';
import { nextWindowCode, secretFromOtpauth, totp } from '../support/totp';

/**
 * SEC-AUD-041 (wave 55, D-573): a change to how an account signs in takes the password, then (when the account has
 * a second factor) the re-auth dialog. Shoots the Discord link password prompt, the re-auth dialog over it and over
 * the password card, the refusal after a cancelled dialog, and clicks the real TOTP removal (password, code, done)
 * in Editorial, Lounge Light and Lounge Dark, desktop and phone. Local only (SR_P55_SIGNIN=1); shots go to
 * SR_P55_SIGNIN_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P55_SIGNIN_OUT ?? resolve('test-results/p55-signin');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P55_SIGNIN !== '1', 'local visual check only');
test.setTimeout(1_200_000);

/** What the server answers when the second factor is missing (the Discord link is not configured here). */
const mfaRequired = (r: Route) =>
  r.fulfill({
    status: 400,
    contentType: 'application/json',
    body: JSON.stringify({
      code: 50035,
      message: 'Invalid Form Body',
      errors: {
        mfa: {
          _errors: [
            {
              code: 'required',
              message:
                'Enter a code from your authenticator app, use a passkey, or enter a recovery code.',
            },
          ],
        },
      },
    }),
  });

test('sign-in changes ask for the password, then the second factor', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  for (const theme of THEMES) {
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      // A fresh member per run: the second-factor window of a session opens for 15 minutes once proved.
      const c = await browser.newContext({ baseURL: ORIGIN, viewport: size });
      const user = await registerUser(c.request, 'p55signin');
      await loginApi(c.request, user);
      await apiPatch(c.request, '/users/@me/settings', {
        settings: { user_prefs: { theme, zoom: 100 } },
      });
      await c.addInitScript(
        (p) => {
          localStorage.setItem(
            'sr.user.prefs',
            JSON.stringify({ state: { prefs: p }, version: 2 }),
          );
        },
        { theme, zoom: 100 },
      );
      // TOTP on the account (the password is checked; there is no factor yet, so nothing else).
      const enrol = await apiPost<{ otpauth_uri: string }>(c.request, '/sr/auth/totp/enroll', {
        password: user.password,
      });
      const secret = secretFromOtpauth(enrol.otpauth_uri);
      await apiPost(c.request, '/sr/auth/totp/verify', { code: totp(secret) });

      const page: Page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const shot = async (name: string) => {
        await page.waitForTimeout(400);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };
      // The Discord card shows only where Discord sign-in is configured: say it is.
      await page.route(/\/api\/v1\/sr\/auth\/discord$/, (r) =>
        r.fulfill({ contentType: 'application/json', body: '{"enabled":true,"linked":false}' }),
      );
      await page.route(/\/api\/v1\/sr\/auth\/discord\/link$/, mfaRequired);
      await page.goto('/settings/user/account');

      // 1. Link Discord: the password prompt first.
      await page.locator('#us-discord').getByRole('button', { name: 'Link Discord' }).click();
      const link = page.getByRole('dialog', { name: 'Link Discord' });
      await expect(link.getByText(/add Discord as a way to sign in/)).toBeVisible();
      await shot('discord-link-password');
      await link.getByLabel('Current password').fill(user.password);
      await link.getByRole('button', { name: 'Continue' }).click();
      // 2. The re-auth dialog over it, with the sign-in wording.
      const reauth = page.getByRole('dialog', { name: 'Confirm it is you' });
      await expect(reauth.getByText(/Changing how you sign in/)).toBeVisible();
      await shot('discord-link-reauth');
      await reauth.getByRole('button', { name: 'Cancel' }).click();
      // 3. Cancelled: nothing was linked and the prompt says why.
      await expect(link.getByText(/Enter a code from your authenticator app/)).toBeVisible();
      await shot('discord-link-cancelled');
      await link.getByRole('button', { name: 'Cancel' }).click();

      // 4. Password change (real server): the right password, then the dialog; cancelling keeps the password.
      const card = page.locator('#us-password');
      await card.getByLabel('Current password').fill(user.password);
      await card.getByLabel('New password').fill('A-different-Passw0rd-77!');
      await card.getByRole('button', { name: 'Change password' }).click();
      await expect(reauth.getByText(/Changing how you sign in/)).toBeVisible();
      await shot('password-reauth');
      await reauth.getByRole('button', { name: 'Cancel' }).click();
      await expect(card.getByText(/Enter a code from your authenticator app/)).toBeVisible();
      await shot('password-cancelled');

      // 5. Remove the authenticator (real server): password, the dialog, a fresh code, done.
      await page.getByRole('button', { name: 'Remove authenticator' }).click();
      const remove = page.getByRole('dialog', { name: 'Remove authenticator' });
      await remove.getByLabel('Current password').fill(user.password);
      await remove.getByRole('button', { name: 'Remove' }).click();
      await expect(reauth.getByText(/Changing how you sign in/)).toBeVisible();
      await shot('totp-remove-reauth');
      // The enrolment code is spent: wait for the next window.
      await reauth
        .getByLabel('Code from your authenticator app')
        .fill(await nextWindowCode(secret));
      await reauth.getByRole('button', { name: 'Confirm' }).click();
      await expect(page.getByText('Two-factor authentication removed')).toBeVisible();
      await expect(page.getByRole('button', { name: 'Enable authenticator app' })).toBeVisible();
      await shot('totp-removed');
      await c.close();
    }
  }
});
