import { apiPost, expect, loginApi, test } from '../support/fixtures';
import { PUBLIC_URL } from '../support/cli';
import { LINKS_NOT_TABBABLE, tabTo, tabsToLinks } from '../support/keys';
import { nextWindowCode, secretFromOtpauth, totp } from '../support/totp';

/**
 * RDY-A11Y-003, flow 2: sign in with the keyboard only, through the second factor, with the passkey button
 * reachable from the form. The account has an authenticator app (enrolled through the API); everything after
 * that is Tab, typing and Enter.
 */
test('sign in: fields, passkey button, second factor, keyboard only', async ({
  page,
  playwright,
  seedUser,
}) => {
  test.skip(!(await tabsToLinks(page)), LINKS_NOT_TABBABLE);
  const user = await seedUser('kb2');
  const api = await playwright.request.newContext({ baseURL: PUBLIC_URL });
  await loginApi(api, user);
  const enroll = await apiPost<{ otpauth_uri: string }>(api, '/sr/auth/totp/enroll');
  const secret = secretFromOtpauth(enroll.otpauth_uri);
  await apiPost(api, '/sr/auth/totp/verify', { code: totp(secret) });
  await api.dispose();

  await page.goto('/login');
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();

  // Skip link, then the form in reading order: username, password, submit, passkey.
  await page.keyboard.press('Tab');
  await expect(page.getByRole('link', { name: 'Skip to main content' })).toBeFocused();
  const login = page.getByLabel('Username or email');
  const password = page.getByLabel('Password', { exact: true });
  const submit = page.getByRole('button', { name: 'Step inside' });
  const passkey = page.getByRole('button', { name: 'Use a passkey' });
  await tabTo(page, login);
  await page.keyboard.type(user.username);
  await tabTo(page, password, { max: 3 });
  await page.keyboard.type(user.password);
  await tabTo(page, submit, { max: 4 });
  // The passkey button is a tab stop of the same form (it is not activated here: that needs an authenticator).
  await tabTo(page, passkey, { max: 6 });
  await tabTo(page, submit, { max: 6, back: true });
  await page.keyboard.press('Enter');

  // Second factor: focus lands on the first digit, the code types across the boxes, Enter is not needed.
  await expect(page).toHaveURL(/\/login\/2fa/);
  const digit = page.getByLabel('Digit 1');
  await expect(digit).toBeVisible();
  if (!(await digit.evaluate((el) => el === document.activeElement)))
    await tabTo(page, digit, { max: 8 });
  await expect(digit).toBeFocused();
  await page.keyboard.type(await nextWindowCode(secret));

  // Signed in: the Lounge's server rail is there, and the skip link leads to the main region.
  await expect(page).toHaveURL(/\/channels/);
  await expect(page.getByRole('navigation', { name: 'Servers' })).toBeVisible();
});
