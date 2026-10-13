import { readFileSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';
import { STATE_FILE, type BootstrapState } from '../global-setup';
import { createInvite } from '../support/cli';

/**
 * Lounge shell, sidebar and modal flows against the real server: a template server, channel management
 * (create, rename, delete) from the channel list, rail folders by drag and drop, mute, and the Friends page
 * (add friend, accept, message) between two real accounts.
 */

const state = (): BootstrapState => JSON.parse(readFileSync(STATE_FILE, 'utf8')) as BootstrapState;

async function login(page: Page, who: { username: string; password: string }) {
  await page.goto('/login');
  await page.getByLabel('Username or email').fill(who.username);
  await page.getByLabel('Password', { exact: true }).fill(who.password);
  await page.getByRole('button', { name: 'Step inside' }).click();
  await expect(page).toHaveURL(/\/channels/);
}

async function createServer(page: Page, name: string, template?: RegExp) {
  await page.getByRole('button', { name: 'Add a Server' }).click();
  const dialog = page.getByRole('dialog', { name: 'Create a Server' });
  if (template) await dialog.getByRole('button', { name: template }).click();
  else await dialog.getByRole('button', { name: 'Create My Own' }).click();
  await page.getByRole('button', { name: 'skip this question' }).click();
  await page.getByLabel('Server name').fill(name);
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page).toHaveURL(/\/channels\/\d+\/\d+/);
}

test.describe('Lounge shell flows', () => {
  test('template server, channel management, rail folder and mute', async ({
    page,
    browserName,
  }) => {
    test.skip(browserName !== 'chromium', 'Chromium only in this environment.');
    const stamp = Date.now().toString(36);
    await login(page, state().owner);

    // Template server: the Gaming template adds its category and channels.
    const first = `Shell ${stamp}`;
    await createServer(page, first, /Gaming/);
    const sidebar = page.getByTestId('channel-sidebar');
    await expect(sidebar.getByRole('link', { name: 'game-chat' })).toBeVisible();
    await expect(sidebar.getByRole('button', { name: 'Squad 1, voice channel' })).toBeVisible();

    // Create a channel from the category "+" button.
    await sidebar.getByRole('button', { name: 'Create Channel in Games' }).click();
    const create = page.getByRole('dialog', { name: 'Create Channel' });
    await create.getByLabel('Channel name').fill('Ideas Box');
    await create.getByRole('button', { name: 'Create Channel' }).click();
    await expect(sidebar.getByRole('link', { name: 'ideas-box' })).toBeVisible();

    // Rename it in the Edit Channel modal, then delete it from the context menu.
    const row = sidebar.getByRole('link', { name: 'ideas-box' }).locator('xpath=ancestor::li[1]');
    await row.hover();
    await row.getByRole('button', { name: 'Edit Channel' }).click();
    const edit = page.getByRole('dialog', { name: 'Edit Channel' });
    await edit.getByLabel('Channel name').fill('idea-vault');
    await edit.getByRole('button', { name: 'Save Changes' }).click();
    await expect(sidebar.getByRole('link', { name: 'idea-vault' })).toBeVisible();
    await edit.getByRole('button', { name: 'Done' }).click();
    await sidebar.getByRole('link', { name: 'idea-vault' }).click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Delete Channel' }).click();
    await page
      .getByRole('dialog', { name: 'Delete Channel' })
      .getByRole('button', { name: 'Delete Channel' })
      .click();
    await expect(sidebar.getByRole('link', { name: 'idea-vault' })).toHaveCount(0);

    // Move a channel up from the context menu; the new order survives a reload.
    await sidebar.getByRole('link', { name: 'clips-and-highlights' }).click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Move Up' }).click();
    await page.reload();
    const names = await page
      .getByTestId('channel-sidebar')
      .getByRole('link')
      .evaluateAll((els) => els.map((e) => (e.getAttribute('aria-label') ?? '').split(',')[0]));
    expect(names.indexOf('clips-and-highlights')).toBeLessThan(names.indexOf('game-chat'));

    // Mute the server from the header menu: the rail dims it and the menu offers Unmute.
    await page.getByTestId('channel-sidebar').getByRole('button', { name: first }).click();
    await page.getByRole('menuitem', { name: 'Mute Server' }).click();
    await page.getByRole('menuitem', { name: 'For 1 Hour' }).click();
    const rail = page.getByRole('navigation', { name: 'Servers' });
    await expect(
      rail
        .getByRole('link', { name: new RegExp(first) })
        .locator('xpath=ancestor::*[@data-muted][1]'),
    ).toHaveCount(1);
    await page.getByTestId('channel-sidebar').getByRole('button', { name: first }).click();
    await expect(page.getByRole('menuitem', { name: 'Unmute Server' })).toBeVisible();
    await page.keyboard.press('Escape');

    // A second server dropped onto the first makes a folder; the layout persists across a reload.
    const second = `Second ${stamp}`;
    await createServer(page, second);
    await rail
      .getByRole('link', { name: new RegExp(second) })
      .dragTo(rail.getByRole('link', { name: new RegExp(first) }));
    await expect(rail.getByRole('button', { name: /folder/ })).toBeVisible();
    await page.reload();
    await expect(
      page.getByRole('navigation', { name: 'Servers' }).getByRole('button', { name: /folder/ }),
    ).toBeVisible();
  });

  test('rail folders follow the account into a fresh browser (D-179)', async ({
    browser,
    browserName,
  }) => {
    test.skip(browserName !== 'chromium', 'Chromium only in this environment.');
    const stamp = Date.now().toString(36);
    const user = {
      username: `railer_${stamp}`,
      email: `railer_${stamp}@example.test`,
      password: 'Railer-Passw0rd-9!',
    };
    const ctxA = await browser.newContext();
    const a = await ctxA.newPage();
    await a.goto('/');
    await a.locator('#invite-code').fill(createInvite(`rail ${stamp}`));
    await a.getByRole('button', { name: 'Check invitation' }).click();
    await expect(a).toHaveURL(/\/register/);
    await a.getByLabel('Email').fill(user.email);
    await a.getByLabel('Username').fill(user.username);
    await a.getByLabel('Password', { exact: true }).fill(user.password);
    await a.getByLabel('Confirm password').fill(user.password);
    await a.getByLabel(/house notes/).check();
    await a.getByRole('button', { name: 'Open the door' }).click();
    await expect(a).toHaveURL(/\/channels/);

    const first = `Rail A ${stamp}`;
    const second = `Rail B ${stamp}`;
    await createServer(a, first);
    await createServer(a, second);
    const rail = a.getByRole('navigation', { name: 'Servers' });
    // The drag writes guild_folders to the account's settings blob (debounced 1 s).
    const saved = a.waitForResponse(
      (r) =>
        r.url().includes('/users/@me/settings') &&
        r.request().method() === 'PATCH' &&
        (r.request().postData() ?? '').includes('guild_folders') &&
        r.ok(),
    );
    await rail
      .getByRole('link', { name: new RegExp(second) })
      .dragTo(rail.getByRole('link', { name: new RegExp(first) }));
    await expect(rail.getByRole('button', { name: /folder/ })).toBeVisible();
    await saved;

    // A browser that never saw this layout (empty localStorage) gets the folder from the server.
    const ctxB = await browser.newContext();
    const b = await ctxB.newPage();
    await login(b, user);
    await expect(
      b.getByRole('navigation', { name: 'Servers' }).getByRole('button', { name: /folder/ }),
    ).toBeVisible();

    await ctxA.close();
    await ctxB.close();
  });

  test('friends: add by username, accept, message', async ({ browser, browserName }) => {
    test.skip(browserName !== 'chromium', 'Chromium only in this environment.');
    const stamp = Date.now().toString(36);
    const ctxA = await browser.newContext();
    const a = await ctxA.newPage();
    const ctxB = await browser.newContext();
    const b = await ctxB.newPage();
    await login(a, state().owner);

    const friend = {
      username: `shellb_${stamp}`,
      email: `shellb_${stamp}@example.test`,
      password: 'Friend-Passw0rd-9!',
    };
    await b.goto('/');
    await b.locator('#invite-code').fill(createInvite(`shell ${stamp}`));
    await b.getByRole('button', { name: 'Check invitation' }).click();
    await expect(b).toHaveURL(/\/register/);
    await b.getByLabel('Email').fill(friend.email);
    await b.getByLabel('Username').fill(friend.username);
    await b.getByLabel('Password', { exact: true }).fill(friend.password);
    await b.getByLabel('Confirm password').fill(friend.password);
    await b.getByLabel(/house notes/).check();
    await b.getByRole('button', { name: 'Open the door' }).click();
    await expect(b).toHaveURL(/\/channels/);

    // A: Friends > Add Friend.
    await a.goto('/channels/@me');
    await a.getByRole('tab', { name: 'Add Friend' }).click();
    await a.getByRole('textbox', { name: 'Username' }).fill(friend.username);
    await a.getByRole('button', { name: 'Send Friend Request' }).click();
    await expect(a.getByText(/request to .* was sent/)).toBeVisible();

    // B: Pending > accept.
    await b.goto('/channels/@me');
    await b.getByRole('tab', { name: /Pending/ }).click();
    await b.getByRole('button', { name: /^Accept / }).click();
    await expect(b.getByText('There are no pending friend requests')).toBeVisible();

    // A: the new friend is on the All tab; Message opens a DM.
    await a.reload();
    await a.getByRole('tab', { name: 'All' }).click();
    await a.getByRole('button', { name: `Message ${friend.username}` }).click();
    await expect(a).toHaveURL(/\/channels\/@me\/\d+/);

    await ctxA.close();
    await ctxB.close();
  });
});
