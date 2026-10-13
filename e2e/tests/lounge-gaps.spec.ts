import { readFileSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';
import { STATE_FILE, type BootstrapState } from '../global-setup';

/**
 * Wave-4 Lounge gaps against the real server (no object storage, so nothing here uploads): threads from a
 * message and from the plus menu, Mark Unread, bookmarks and reminders in the settings blob, the profile
 * popout, /nick, search context lines, the channel editor tabs and the age-restricted channel gate.
 */

const state = (): BootstrapState => JSON.parse(readFileSync(STATE_FILE, 'utf8')) as BootstrapState;

async function login(page: Page, who: { username: string; password: string }) {
  await page.goto('/login');
  await page.getByLabel('Username or email').fill(who.username);
  await page.getByLabel('Password', { exact: true }).fill(who.password);
  await page.getByRole('button', { name: 'Step inside' }).click();
  await expect(page).toHaveURL(/\/channels/);
}

async function createServer(page: Page, name: string) {
  await page.getByRole('button', { name: 'Add a Server' }).click();
  const dialog = page.getByRole('dialog', { name: 'Create a Server' });
  await dialog.getByRole('button', { name: 'Create My Own' }).click();
  await page.getByRole('button', { name: 'skip this question' }).click();
  await page.getByLabel('Server name').fill(name);
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page).toHaveURL(/\/channels\/\d+\/\d+/);
}

async function say(page: Page, text: string) {
  const box = page.getByRole('textbox', { name: /^Message / }).first();
  await box.fill(text);
  await box.press('Enter');
  await expect(page.getByRole('log').first().getByText(text, { exact: true })).toBeVisible();
}

test.describe('Lounge gaps', () => {
  test('threads, Mark Unread, bookmarks, profile popout, /nick and search context', async ({
    page,
    browserName,
  }) => {
    test.skip(browserName === 'firefox', 'Firefox is not installed in this environment.');
    const stamp = Date.now().toString(36);
    await login(page, state().owner);
    await createServer(page, `Gaps ${stamp}`);
    const log = page.getByRole('log').first();

    await say(page, `alpha ${stamp}`);
    await say(page, `bravo ${stamp}`);
    await say(page, `charlie ${stamp}`);

    // Create a thread from a message through the context menu, then talk inside it.
    await log.getByText(`bravo ${stamp}`).click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Create Thread' }).click();
    const dialog = page.getByRole('dialog', { name: 'Create Thread' });
    await dialog.getByRole('textbox', { name: 'Thread name' }).fill(`Side ${stamp}`);
    await dialog.getByRole('button', { name: 'Create Thread' }).click();
    const pane = page.getByRole('complementary', { name: `Thread: Side ${stamp}` });
    await expect(pane).toBeVisible();
    await expect(page).toHaveURL(/\/threads\/\d+/);
    await pane.getByRole('textbox', { name: `Message Side ${stamp}` }).fill(`inside ${stamp}`);
    await pane.getByRole('textbox', { name: `Message Side ${stamp}` }).press('Enter');
    await expect(pane.getByText(`inside ${stamp}`)).toBeVisible();

    // The parent message grows a chip, and the Threads popover lists the thread.
    const chip = log.getByRole('button', { name: new RegExp(`Side ${stamp}`) });
    await expect(chip).toBeVisible();
    await pane.getByRole('button', { name: 'Close thread' }).click();
    await expect(pane).toHaveCount(0);
    await page.getByRole('button', { name: 'Threads' }).click();
    await expect(
      page.getByRole('dialog', { name: 'Threads' }).getByText(`Side ${stamp}`),
    ).toBeVisible();
    await page.keyboard.press('Escape');
    await chip.click();
    await expect(pane).toBeVisible();
    await pane.getByRole('button', { name: 'Close thread' }).click();

    // Mark Unread moves the NEW divider before the chosen message (manual ack on the server).
    await log.getByText(`charlie ${stamp}`).click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Mark Unread' }).click();
    await expect(page.getByRole('separator', { name: 'New messages' })).toBeVisible();

    // Bookmark and reminder are stored in the settings blob and survive a reload.
    await log.getByText(`alpha ${stamp}`).click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Bookmark Message' }).click();
    await log.getByText(`alpha ${stamp}`).click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Remind Me' }).click();
    await page.getByRole('menuitem', { name: 'in 24 hours' }).click();
    await page.waitForResponse(
      (r) => r.url().includes('/users/@me/settings') && r.request().method() === 'PATCH',
    );
    await page.reload();
    await expect(page.getByRole('log').first().getByText(`alpha ${stamp}`)).toBeVisible();
    await page.getByRole('log').first().getByText(`alpha ${stamp}`).click({ button: 'right' });
    await expect(page.getByRole('menuitem', { name: 'Remove Bookmark' })).toBeVisible();
    await page.keyboard.press('Escape');

    // Profile popout from the author name.
    await page.getByRole('log').first().getByRole('button', { name: 'owner' }).first().click();
    const pop = page.getByRole('dialog', { name: 'User profile' });
    await expect(pop.getByRole('heading', { name: 'owner' })).toBeVisible();
    await expect(pop.getByRole('button', { name: 'View Full Profile' })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(pop).toHaveCount(0);

    // /nick changes the nickname in this server.
    const box = page.getByRole('textbox', { name: /^Message / }).first();
    await box.fill(`/nick Chief ${stamp}`);
    await box.press('Enter');
    await expect(page.getByText(`Your nickname is now Chief ${stamp}`)).toBeVisible();

    // Search shows the neighbours of each hit.
    const search = page.getByRole('searchbox', { name: /^Search/ });
    await search.fill(`bravo`);
    await search.press('Enter');
    const results = page.getByRole('region', { name: 'Search results' });
    await expect(results.getByText(`bravo ${stamp}`)).toBeVisible();
    await expect(results.getByText(`alpha ${stamp}`)).toBeVisible();
    // The neighbours are read after the result renders; allow for a slow machine.
    await expect(results.locator('.lng-search-context')).not.toHaveCount(0, { timeout: 20_000 });
  });

  test('channel editor tabs, standalone thread and the age-restricted gate', async ({
    page,
    browserName,
  }) => {
    test.skip(browserName === 'firefox', 'Firefox is not installed in this environment.');
    const stamp = Date.now().toString(36);
    await login(page, state().owner);
    await createServer(page, `Gate ${stamp}`);
    const sidebar = page.getByTestId('channel-sidebar');

    // Edit Channel: Invites, Integrations and Features tabs are backed by real endpoints.
    const row = sidebar.getByRole('link', { name: 'general' }).locator('xpath=ancestor::li[1]');
    await row.hover();
    await row.getByRole('button', { name: 'Edit Channel' }).click();
    const edit = page.getByRole('dialog', { name: 'Edit Channel' });
    await edit.getByRole('tab', { name: 'Invites' }).click();
    await edit.getByRole('button', { name: 'Create invite' }).click();
    await expect(
      edit.getByRole('list', { name: 'Invites to this channel' }).getByRole('listitem'),
    ).toHaveCount(1);
    await edit.getByRole('tab', { name: 'Integrations' }).click();
    await edit.getByRole('button', { name: 'Create webhook' }).click();
    await page.getByLabel('Webhook name').fill(`Hook ${stamp}`);
    await page
      .getByRole('dialog', { name: 'Create webhook' })
      .getByRole('button', { name: 'Create' })
      .click();
    await expect(edit.getByText(`Webhook created: Hook ${stamp}`)).toBeVisible();
    await expect(
      edit.getByRole('list', { name: 'Webhooks in this channel' }).getByText(`Hook ${stamp}`),
    ).toBeVisible();
    await edit.getByRole('tab', { name: 'Features' }).click();
    const archive = edit.getByLabel('Hide threads after inactivity');
    await archive.selectOption('4320');
    await expect(edit.getByText('Saved.')).toBeVisible();

    // Mark the channel age-restricted in Overview; the next visit asks first.
    await edit.getByRole('tab', { name: 'Overview' }).click();
    await edit.getByRole('switch', { name: 'Age-restricted channel' }).check();
    await edit.getByRole('button', { name: 'Save Changes' }).click();
    await expect(edit.getByText('Channel saved.')).toBeVisible();
    await edit.getByRole('button', { name: 'Done' }).click();
    await page.reload();
    await expect(page.getByRole('heading', { name: '#general is age-restricted' })).toBeVisible();
    await expect(page.getByRole('log')).toHaveCount(0);
    await page.getByRole('button', { name: 'Yes, I am 18 or older' }).click();
    await expect(page.getByRole('log').first()).toBeVisible();
    await page.reload();
    await expect(page.getByRole('log').first()).toBeVisible();
    await expect(page.getByRole('heading', { name: '#general is age-restricted' })).toHaveCount(0);

    // A standalone thread from the plus menu, and the sticker picker's empty state.
    await page.getByRole('button', { name: 'Add to message' }).click();
    await page.getByRole('menuitem', { name: 'Create Thread' }).click();
    const dialog = page.getByRole('dialog', { name: 'Create Thread' });
    await dialog.getByRole('textbox', { name: 'Thread name' }).fill(`Free ${stamp}`);
    await dialog.getByRole('button', { name: 'Create Thread' }).click();
    await expect(page.getByRole('complementary', { name: `Thread: Free ${stamp}` })).toBeVisible();
    await page.getByRole('button', { name: 'Close thread' }).click();
    await page.getByRole('button', { name: 'Open sticker picker' }).click();
    await expect(page.getByText('This server has no stickers yet.')).toBeVisible();
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Open GIF picker' }).click();
    await expect(page.getByText(/GIFs are not set up/)).toBeVisible();
  });
});
