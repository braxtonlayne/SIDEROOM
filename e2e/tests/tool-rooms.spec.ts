import { expect, test, apiPost } from '../support/fixtures';
import type { Page } from '@playwright/test';

/**
 * UTL-COL-001: the collaborative tier list. Two people in one server channel open the Tier list tool from the
 * utility drawer, one starts a board, both see it, and an edit by either shows up for the other without a reload.
 * A third person outside the server cannot read the board over the API.
 *
 * Needs the whole stack (sr-server with the migrations through 1660 and the web app). The wave-32 ports for this
 * spec are 8081 (API) and 5174 (web): `SR_E2E_BASE_URL=http://localhost:5174 SR_E2E_SERVER_PORT=8081`.
 */

async function openTierList(page: Page, guildId: string, channelId: string) {
  await page.goto(`/channels/${guildId}/${channelId}`);
  await page.getByRole('navigation', { name: 'Servers' }).waitFor({ timeout: 60_000 });
  await page.keyboard.press('Control+k');
  const drawer = page.getByRole('dialog', { name: 'Drawer' });
  await drawer.getByRole('combobox').fill('>tier list');
  await drawer.getByRole('option', { name: /Tier list/ }).first().click();
  return drawer;
}

test.describe('Tool rooms (tier list)', () => {
  test('two people edit one tier list live, and a stranger cannot read it', async ({
    scenario,
    seedUser,
    signedIn,
  }) => {
    const { a, b, guildId, channelId } = await scenario.ownerAndMember();
    const da = await openTierList(a.page, guildId, channelId);
    const db = await openTierList(b.page, guildId, channelId);

    // A starts a board with the default rows.
    await da.getByRole('button', { name: 'Start a new one' }).click();
    await expect(da.getByRole('status').first()).toHaveText('Everyone is up to date');

    // B finds it in the channel's list and opens it.
    await db.getByRole('button', { name: 'Tier list', exact: true }).click();
    await expect(db.getByLabel('Row name').first()).toHaveValue('S');

    // A adds an item; B sees it in the unranked pool.
    await da.getByLabel('Item', { exact: true }).fill('pizza');
    await da.getByRole('button', { name: 'Add item' }).click();
    await expect(db.getByRole('list', { name: 'Unranked' }).getByText('pizza')).toBeVisible();

    // B moves it into S; A sees it in the S row.
    await db.getByLabel('Move pizza to').selectOption({ label: 'S' });
    await expect(da.getByRole('list', { name: 'S' }).getByText('pizza')).toBeVisible();

    // Both edit at the same moment (different items): neither loses theirs.
    await Promise.all([
      (async () => {
        await da.getByLabel('Item', { exact: true }).fill('tacos');
        await da.getByRole('button', { name: 'Add item' }).click();
      })(),
      (async () => {
        await db.getByLabel('Item', { exact: true }).fill('sushi');
        await db.getByRole('button', { name: 'Add item' }).click();
      })(),
    ]);
    for (const d of [da, db]) {
      await expect(d.getByRole('list', { name: 'Unranked' }).getByText('tacos')).toBeVisible();
      await expect(d.getByRole('list', { name: 'Unranked' }).getByText('sushi')).toBeVisible();
    }

    // Someone who is not in the server gets a refusal from the API.
    const stranger = await signedIn(await seedUser('stranger'));
    const res = await stranger.context.request.get(`/api/v1/channels/${channelId}/tool-rooms`);
    expect([403, 404]).toContain(res.status());
  });

  test('a poll builder room becomes the ordinary poll message', async ({ scenario }) => {
    const { a, b, guildId, channelId } = await scenario.ownerAndMember();
    await apiPost(a.context.request, `/channels/${channelId}/tool-rooms`, {
      kind: 'poll',
      title: 'Lunch',
      state: {
        poll: { question: 'Lunch?', hours: 24, multi: false },
        'opt.a': { text: 'Pizza', emoji: '', pos: 1 },
        'opt.b': { text: 'Tacos', emoji: '', pos: 2 },
      },
    });
    await b.page.goto(`/channels/${guildId}/${channelId}`);
    await b.page.getByRole('navigation', { name: 'Servers' }).waitFor({ timeout: 60_000 });
    await b.page.keyboard.press('Control+k');
    const drawer = b.page.getByRole('dialog', { name: 'Drawer' });
    await drawer.getByRole('combobox').fill('>poll builder');
    await drawer.getByRole('option', { name: /Poll builder/ }).first().click();
    await drawer.getByRole('button', { name: 'Lunch' }).click();
    await expect(drawer.getByLabel('Question')).toHaveValue('Lunch?');
    await drawer.getByRole('button', { name: 'Post the poll' }).click();
    await expect(b.page.getByText('Lunch?').first()).toBeVisible();
    await expect(b.page.getByText('Pizza').first()).toBeVisible();
  });
});
