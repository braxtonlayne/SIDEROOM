import { readFileSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';
import { STATE_FILE, type BootstrapState } from '../global-setup';
import { createInvite } from '../support/cli';

/**
 * Lounge server flows: the Owner (user A) creates a server in the UI and an invite link; a newly registered
 * user (B, platform invite from `sr-cli`) accepts that link in the UI; both then see each other's messages,
 * which also proves the gateway WebSocket works behind the Vite dev proxy.
 */

const state = (): BootstrapState => JSON.parse(readFileSync(STATE_FILE, 'utf8')) as BootstrapState;

async function composerSend(page: Page, channel: string, text: string) {
  const box = page.getByRole('textbox', { name: `Message #${channel}` });
  await box.click();
  await page.keyboard.type(text);
  await page.keyboard.press('Enter');
}

test.describe('Lounge server flows', () => {
  test('A creates a server and an invite, B registers and accepts it, both see each other', async ({
    browser,
    browserName,
  }) => {
    test.skip(browserName !== 'chromium', 'Chromium only in this environment.');
    const stamp = Date.now().toString(36);
    const serverName = `Flow ${stamp}`;
    const { owner } = state();

    const ctxA = await browser.newContext();
    const a = await ctxA.newPage();
    const ctxB = await browser.newContext();
    const b = await ctxB.newPage();

    // --- A: sign in, create a server through "Add a Server".
    await a.goto('/login');
    await a.getByLabel('Username or email').fill(owner.username);
    await a.getByLabel('Password', { exact: true }).fill(owner.password);
    await a.getByRole('button', { name: 'Step inside' }).click();
    await expect(a).toHaveURL(/\/channels/);

    await a.getByRole('button', { name: 'Add a Server' }).click();
    await a.getByRole('button', { name: 'Create My Own' }).click();
    await a.getByLabel('Server name').fill(serverName);
    await a.getByRole('button', { name: 'Create', exact: true }).click();
    await expect(a).toHaveURL(/\/channels\/\d+\/\d+/);
    await expect(a.getByRole('heading', { level: 1, name: 'general' })).toBeVisible();
    // The gateway must be live for B's message to arrive without a reload.
    await expect(a.getByRole('textbox', { name: 'Message #general' })).toBeEnabled();

    // --- A: create an invite from the server menu and read the link.
    await a.getByTestId('channel-sidebar').getByRole('button', { name: serverName }).click();
    await a.getByRole('menuitem', { name: 'Invite to Server' }).click();
    const dialog = a.getByRole('dialog', { name: /Invite people to/ });
    await dialog.getByRole('button', { name: 'Generate invite link' }).click();
    const link = await dialog.getByLabel('Invite link').inputValue();
    expect(link).toMatch(/\/invite\/[A-Za-z0-9-]+$/);
    await a.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);

    await composerSend(a, 'general', `hello from A ${stamp}`);
    await expect(a.getByText(`hello from A ${stamp}`)).toBeVisible();

    // --- B: register with a platform invite, then accept A's server invite link.
    const friend = {
      username: `flowb_${stamp}`,
      email: `flowb_${stamp}@example.test`,
      password: 'Friend-Passw0rd-9!',
    };
    const platformCode = createInvite(`flows ${stamp}`);
    await b.goto('/');
    await b.locator('#invite-code').fill(platformCode);
    await b.getByRole('button', { name: 'Check invitation' }).click();
    await expect(b).toHaveURL(/\/register/);
    await b.getByLabel('Email').fill(friend.email);
    await b.getByLabel('Username').fill(friend.username);
    await b.getByLabel('Password', { exact: true }).fill(friend.password);
    await b.getByLabel('Confirm password').fill(friend.password);
    await b.getByLabel(/house notes/).check();
    await b.getByRole('button', { name: 'Open the door' }).click();
    await expect(b).toHaveURL(/\/channels/);

    await b.goto(new URL(link).pathname);
    await expect(b.getByRole('heading', { name: serverName })).toBeVisible();
    await b.getByRole('button', { name: 'Accept invite' }).click();
    await expect(b).toHaveURL(/\/channels\/\d+\/\d+/);
    await expect(b.getByRole('heading', { level: 1, name: 'general' })).toBeVisible();

    // B sees A's earlier message, then both exchange live messages.
    await expect(b.getByText(`hello from A ${stamp}`)).toBeVisible();
    await composerSend(b, 'general', `hello from B ${stamp}`);
    await expect(b.getByText(`hello from B ${stamp}`)).toBeVisible();
    await expect(a.getByText(`hello from B ${stamp}`)).toBeVisible();
    await composerSend(a, 'general', `reply from A ${stamp}`);
    await expect(b.getByText(`reply from A ${stamp}`)).toBeVisible();

    // B is now a member of the server on the rail; a non-manager has no Create Channel entry.
    await b.getByTestId('channel-sidebar').getByRole('button', { name: serverName }).click();
    await expect(b.getByRole('menuitem', { name: 'Invite to Server' })).toBeVisible();
    await expect(b.getByRole('menuitem', { name: 'Create Channel' })).toHaveCount(0);

    await ctxA.close();
    await ctxB.close();
  });
});
