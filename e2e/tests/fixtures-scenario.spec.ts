import { expect, test } from '../support/fixtures';

/**
 * Exercises the shared fixtures (XC-TEST-004): seeded users, API-driven sign-in and a two-user scenario.
 * Doubles as the reference for how new multi-user specs should start.
 */
test.describe('e2e fixtures', () => {
  test('a seeded user is signed in without the login form', async ({
    browserName,
    seedUser,
    signedIn,
  }) => {
    test.skip(browserName !== 'chromium', 'Chromium only in this environment.');
    const { page } = await signedIn(await seedUser('solo'));
    await page.goto('/channels/@me');
    await expect(page).toHaveURL(/\/channels\/@me/);
    await expect(page.getByRole('navigation', { name: 'Servers' })).toBeVisible();
  });

  test('owner and member share a server and see each other live', async ({
    browserName,
    scenario,
  }) => {
    test.skip(browserName !== 'chromium', 'Chromium only in this environment.');
    const { a, b, guildId, channelId } = await scenario.ownerAndMember();
    await a.page.goto(`/channels/${guildId}/${channelId}`);
    await b.page.goto(`/channels/${guildId}/${channelId}`);
    const box = a.page.getByRole('textbox', { name: 'Message #general' });
    await expect(box).toBeEnabled();
    await box.click();
    const text = `fixture hello ${Date.now()}`;
    await a.page.keyboard.type(text);
    await a.page.keyboard.press('Enter');
    await expect(b.page.getByText(text)).toBeVisible();
  });
});
