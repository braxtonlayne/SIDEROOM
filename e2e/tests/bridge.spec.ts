import type { Page } from '@playwright/test';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import {
  FAKE_BOT_TOKEN,
  FAKE_CHANNEL_ID,
  FAKE_CHANNEL_NAME,
  fakeInject,
  fakeReset,
  fakeState,
} from '../support/bridgeFake';

/**
 * BRG-UI-001 (wave 35, D-381): the Discord bridge from the owner's switch to a clean stop, against the fake Discord
 * (bridge-core's `e2e/support/fake-discord.mjs`; real Discord is never contacted).
 *
 *  1. A manager (MANAGE_GUILD, not the owner) sees the card under Modules greyed with the reason, and no
 *     "Discord bridge" entry in the settings nav.
 *  2. The owner switches it on under Modules; the entry appears. They paste the bot token (sealed, not shown
 *     again), map #general to the fake's channel, and see the banner in the channel.
 *  3. A message posted here reaches the fake Discord's webhook with the author's name; a message posted there
 *     shows here with the DISCORD tag.
 *  4. A member posts: it is mirrored too (there is no per-member opt-out, owner decision).
 *  5. The owner switches the bridge off: the webhook DELETE reaches the fake, the banner and the nav entry are
 *     gone, the system message is in the channel, and a post here reaches nothing.
 *
 * STATUS: written against the contract in `support/bridgeFake.ts`; it needs the fake Discord on 8090 and the server
 * pointed at it (bridge-core). It is skipped when the fake is not up, so a run without it says so instead of failing.
 */

const poll = <T>(fn: () => Promise<T>, done: (v: T) => boolean) =>
  expect.poll(async () => done(await fn()), { timeout: 15_000 });

async function send(page: Page, channel: string, text: string) {
  const box = page.getByRole('textbox', { name: `Message #${channel}` });
  await expect(box).toBeEnabled();
  await box.click();
  await page.keyboard.type(text);
  await page.keyboard.press('Enter');
  await expect(page.getByText(text, { exact: true })).toBeVisible();
}

test.describe('the Discord bridge', () => {
  test('owner switches it on, maps a channel, messages flow both ways, switch-off leaves nothing', async ({
    scenario,
    playwright,
  }) => {
    test.setTimeout(180_000);
    const control = await playwright.request.newContext();
    try {
      const up = await control
        .get(`${process.env.SR_FAKE_DISCORD_URL ?? 'http://127.0.0.1:8090'}/__fake/state`)
        .catch(() => null);
      test.skip(!up?.ok(), 'The fake Discord (bridge-core, port 8090) is not running.');
      await fakeReset(control);

      const { a, b, guildId, channelId } = await scenario.ownerAndMember();
      const owner = a.page;
      const member = b.page;

      // The member becomes a manager: a role with MANAGE_GUILD (bit 5).
      const role = await apiPost<{ id: string }>(a.context.request, `/guilds/${guildId}/roles`, {
        name: 'Manager',
        permissions: String(1 << 5),
      });
      await apiPatch(a.context.request, `/guilds/${guildId}/members/${b.user.id}`, {
        roles: [role.id],
      });

      // --- 1. The manager sees the card greyed and no nav entry.
      await member.goto(`/settings/server/${guildId}/features`);
      const card = member.getByTestId('bridge-card');
      await expect(card).toBeVisible();
      await expect(card.getByRole('switch', { name: 'Enable Discord bridge' })).toBeDisabled();
      await expect(card.getByText('Only the server owner can switch this on.')).toBeVisible();
      await expect(member.getByRole('button', { name: 'Discord bridge' })).toHaveCount(0);

      // --- 2. The owner switches it on, the entry appears, the token is pasted and a channel is mapped.
      await owner.goto(`/settings/server/${guildId}/features`);
      await expect(
        owner.getByText(/mirrors the channels you map to a Discord server/),
      ).toBeVisible();
      await expect(owner.getByRole('button', { name: 'Discord bridge' })).toHaveCount(0);
      await owner.getByRole('switch', { name: 'Enable Discord bridge' }).click();
      await owner.getByRole('button', { name: 'Discord bridge' }).click();
      await owner.getByLabel('Bot token').fill(FAKE_BOT_TOKEN);
      await owner.getByRole('button', { name: 'Connect the bot' }).click();
      await expect(owner.getByText(/^Connected to the Discord server/)).toBeVisible();
      // Shown once, sealed: the token is not on the page any more.
      await expect(owner.getByLabel('Bot token')).toHaveCount(0);
      expect(await owner.content()).not.toContain(FAKE_BOT_TOKEN);

      await owner.getByLabel('Channel here', { exact: true }).selectOption({ label: '#general' });
      await owner
        .getByLabel('Channel on Discord', { exact: true })
        .selectOption({ label: `#${FAKE_CHANNEL_NAME}` });
      await owner.getByRole('button', { name: 'Mirror this channel' }).click();
      const table = owner.getByRole('table', { name: 'Mirrored channels' });
      await expect(table.getByText(`#${FAKE_CHANNEL_NAME}`)).toBeVisible();
      await owner.getByRole('button', { name: 'Post a hello in #general on Discord' }).click();
      await poll(
        () => fakeState(control),
        (s) => s.posts.length >= 1,
      );

      // The banner is in the channel for both people.
      await owner.goto(`/channels/${guildId}/${channelId}`);
      await member.goto(`/channels/${guildId}/${channelId}`);
      await expect(owner.getByTestId('bridge-banner')).toContainText('mirrored to Discord server');
      await expect(member.getByTestId('bridge-banner')).toBeVisible();

      // --- 3. Here to there, and there to here with the DISCORD tag.
      await send(owner, 'general', 'hello from the lounge');
      await poll(
        () => fakeState(control),
        (s) => s.posts.some((p) => p.content === 'hello from the lounge' && !!p.username),
      );
      await fakeInject(control, {
        channel_id: FAKE_CHANNEL_ID,
        content: 'hello from Discord',
        author: { id: '4200000000000000001', username: 'Rhea' },
      });
      const arrived = owner.getByRole('article').filter({ hasText: 'hello from Discord' });
      await expect(arrived).toBeVisible();
      await expect(arrived.getByTestId('bridge-tag')).toHaveText('DISCORD');
      await expect(arrived).toContainText('Rhea');

      // --- 4. A member's message in the mapped channel is mirrored too: there is no opt-out.
      await send(member, 'general', 'member note');
      await poll(
        () => fakeState(control),
        (s) => s.posts.some((p) => p.content === 'member note'),
      );
      await owner.goto(`/settings/server/${guildId}/bridge`);
      await expect(owner.getByRole('table', { name: 'Mirrored channels' })).toBeVisible();

      // --- 5. Switch off: what is removed, what stays, then nothing is left.
      await owner.getByRole('button', { name: 'Switch off the Discord bridge' }).click();
      const dialog = owner.getByRole('dialog', { name: 'Switch off the Discord bridge?' });
      await expect(dialog.getByText(/Messages already mirrored stay where they are/)).toBeVisible();
      await dialog.getByRole('button', { name: 'Switch off and remove' }).click();
      const done = owner.getByRole('dialog', { name: 'The Discord bridge is off' });
      await expect(done.getByText('Every webhook was deleted on Discord.')).toBeVisible();
      await done.getByRole('button', { name: 'Done' }).click();
      await poll(
        () => fakeState(control),
        (s) => s.deleted_webhooks.length >= 1,
      );
      await expect(owner.getByRole('button', { name: 'Discord bridge' })).toHaveCount(0);

      await owner.goto(`/channels/${guildId}/${channelId}`);
      await expect(owner.getByText('The Discord bridge was switched off')).toBeVisible();
      await expect(owner.getByTestId('bridge-banner')).toHaveCount(0);
      const posts = (await fakeState(control)).posts.length;
      await send(owner, 'general', 'after the bridge');
      await new Promise((r) => setTimeout(r, 4_000));
      expect((await fakeState(control)).posts.length).toBe(posts);
    } finally {
      await control.dispose();
    }
  });
});
