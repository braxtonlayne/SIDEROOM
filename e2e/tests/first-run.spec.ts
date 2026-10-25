import { expect, test, type Page } from '@playwright/test';
import { createInvite } from '../support/cli';
import { openTheDoor, tickConfirmations } from '../support/register';
import { totp } from '../support/totp';

/**
 * RDY-ONB-001, from register to the first message. The e2e build mutes the welcome for every other spec (every
 * account is new), so this spec opts in per page. What it proves is the roadmap's guard: nothing in the first run
 * stands between a new person and the Lounge. Whoever closes it (Skip, Escape, or the last button) can create a
 * server and send a message at once, and it does not come back after a reload. One test walks every step,
 * including the two-step sign-in set up in place.
 */

/** Opts the page in to the welcome and counts every time anything asks for the notification permission. */
const OPT_IN = () => {
  sessionStorage.setItem('sr-e2e-first-run', '1');
  (window as unknown as { __notifAsked: number }).__notifAsked = 0;
  const n = window.Notification as typeof Notification | undefined;
  if (n?.requestPermission) {
    const original = n.requestPermission.bind(n);
    n.requestPermission = (...args: Parameters<typeof original>) => {
      (window as unknown as { __notifAsked: number }).__notifAsked++;
      return original(...args);
    };
  }
};

async function registerThroughTheDoor(page: Page, label: string) {
  const stamp = `${Date.now().toString(36)}${Math.floor(Math.random() * 1000)}`;
  const me = {
    username: `fr_${label}_${stamp}`.slice(0, 30),
    email: `fr_${label}_${stamp}@example.test`,
    password: 'FirstRun-Passw0rd-9!',
  };
  await page.addInitScript(OPT_IN);
  await page.goto('/');
  await page.locator('#invite-code').fill(createInvite(`first run ${label} ${stamp}`));
  await page.getByRole('button', { name: 'Check invitation' }).click();
  await expect(page).toHaveURL(/\/register/);
  await page.getByLabel('Email').fill(me.email);
  await page.getByLabel('Username').fill(me.username);
  await page.getByLabel('Password', { exact: true }).fill(me.password);
  await page.getByLabel('Confirm password').fill(me.password);
  await tickConfirmations(page);
  await openTheDoor(page);
  return me;
}

const welcome = (page: Page) => page.getByRole('dialog', { name: 'Welcome to SIDE ROOM' });

/** Creates a server of one's own and sends the first message in #general. */
async function firstMessage(page: Page, text: string) {
  await page.getByRole('button', { name: 'Add a Server' }).click();
  const dialog = page.getByRole('dialog', { name: 'Create a Server' });
  await dialog.getByRole('button', { name: 'Create My Own' }).click();
  await page.getByRole('button', { name: 'skip this question' }).click();
  await page.getByLabel('Server name').fill('First run');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page).toHaveURL(/\/channels\/\d+\/\d+/);
  const box = page.getByRole('textbox', { name: /^Message / }).first();
  await box.fill(text);
  await box.press('Enter');
  await expect(page.getByRole('log').first().getByText(text, { exact: true })).toBeVisible();
}

test.describe('platform first run', () => {
  test('register, skip the welcome, send a first message; it does not come back', async ({
    page,
  }) => {
    await registerThroughTheDoor(page, 'skip');
    await expect(welcome(page)).toBeVisible();
    await expect(welcome(page).getByText(/^Step 1 of \d$/)).toBeVisible();

    await welcome(page).getByRole('button', { name: 'Skip for now' }).click();
    await expect(welcome(page)).toHaveCount(0);
    await firstMessage(page, `hello after skipping ${Date.now()}`);

    // Stored on the account: not on a reload, and not on the next page either.
    await page.reload();
    await expect(page.getByRole('textbox', { name: /^Message / }).first()).toBeEnabled();
    await page.waitForTimeout(1500);
    await expect(welcome(page)).toHaveCount(0);
  });

  test('Escape closes it from a later step and the Lounge works straight away', async ({
    page,
  }) => {
    await registerThroughTheDoor(page, 'esc');
    await expect(welcome(page)).toBeVisible();
    await welcome(page).getByRole('button', { name: 'Next' }).click();
    await expect(welcome(page).getByRole('heading', { name: 'Pick a look' })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(welcome(page)).toHaveCount(0);
    await firstMessage(page, `hello after escape ${Date.now()}`);
  });

  test('walks every step, sets two-step sign-in up in place and finishes', async ({ page }) => {
    await registerThroughTheDoor(page, 'walk');
    const dialog = welcome(page);
    await expect(dialog.getByRole('heading', { name: 'Three rooms, one door' })).toBeVisible();
    await expect(dialog.getByText(/The Library: manga/)).toBeVisible();

    await dialog.getByRole('button', { name: 'Next' }).click();
    await dialog.getByRole('button', { name: 'Onyx' }).click();
    await expect(dialog.getByRole('button', { name: 'Onyx' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );

    // Two-step sign-in, in place: the same wizard as Settings, over the Lounge.
    await dialog.getByRole('button', { name: 'Next' }).click();
    await dialog.getByRole('button', { name: 'Set it up here' }).click();
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
    await expect(dialog.getByText('Two-step sign-in is on. Thank you.')).toBeVisible();
    await expect(page).toHaveURL(/\/channels/);

    // Notifications: told, not asked. (The install step comes first where the browser offers one.)
    const notifications = dialog.getByRole('heading', { name: 'Notifications' });
    for (let i = 0; i < 3 && !(await notifications.isVisible()); i++) {
      await dialog.getByRole('button', { name: 'Next' }).click();
    }
    await expect(notifications).toBeVisible();
    expect(
      await page.evaluate(() => (window as unknown as { __notifAsked: number }).__notifAsked),
    ).toBe(0);

    await dialog.getByRole('button', { name: 'Start exploring' }).click();
    await expect(welcome(page)).toHaveCount(0);
    await firstMessage(page, `hello after the tour ${Date.now()}`);
  });

  test('the handbook brings the welcome back', async ({ page }) => {
    await registerThroughTheDoor(page, 'again');
    await welcome(page).getByRole('button', { name: 'Skip for now' }).click();
    await expect(welcome(page)).toHaveCount(0);

    await page.goto('/help');
    await expect(page.getByRole('heading', { level: 1, name: 'Help' })).toBeVisible();
    // Every section names its screen.
    await expect(page.getByRole('link', { name: 'Open the TV guide' })).toHaveAttribute(
      'href',
      '/cinema/tv',
    );
    await page.getByRole('button', { name: 'Show the welcome again' }).click();
    await expect(page).toHaveURL(/\/channels/);
    await expect(welcome(page)).toBeVisible();
    await welcome(page).getByRole('button', { name: 'Skip for now' }).click();
    await expect(welcome(page)).toHaveCount(0);
  });

  test("what's new is reachable from the help menu", async ({ page }) => {
    await registerThroughTheDoor(page, 'news');
    await welcome(page).getByRole('button', { name: 'Skip for now' }).click();
    await page.getByRole('button', { name: /^Help/ }).first().click();
    await page.getByRole('menuitem', { name: "What's new" }).click();
    await expect(page).toHaveURL(/\/help\/whats-new/);
    await expect(page.getByRole('heading', { level: 1, name: "What's new" })).toBeVisible();
    await expect(page.getByRole('heading', { level: 2 }).first()).toContainText(/^Phase \d+/);
  });
});
