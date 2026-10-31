import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import {
  FAKE_BOT_TOKEN,
  FAKE_CHANNEL_ID,
  fakeAdminEdit,
  fakeReset,
  fakeStructure,
  type FakeStructure,
} from '../support/bridgeFake';

/**
 * BRG-SYN-004 (wave 35b, design 16.4): the whole-server mirror from the owner's point of view, end to end against
 * the real routes of sr-server and the fake Discord (REST, gateway and sync-gateway's structural half, port 8090).
 * Real Discord is never contacted.
 *
 *  1. The owner switches the bridge on and connects the bot; the mode card says "Only bridge these".
 *  2. Prepare the whole-server plan: the preflight runs against the fake (Administrator, top role, intents); the
 *     plan lists the Manager role to be created on Discord. The owner confirms with the plan hash; the seed runs and
 *     the status reads "In sync"; the fake now has the Manager role.
 *  3. A Discord admin gives Manager Ban Members on the fake: the bridge puts SIDE ROOM's word back on Discord and the
 *     conflict log shows it; "Apply on SIDE ROOM" opens the role editor prefilled and saves nothing.
 *  4. A Discord admin renames the fake's channel: the name arrives here. They delete it: it is held here (a conflict
 *     row, not deleted here), and "Restore" creates it again on Discord.
 *  5. Back to "Only bridge these" deletes nothing on Discord.
 *  6. Whole server again, then switch off with "Also remove what the bridge created": the channels the bridge
 *     created on Discord are gone from the fake, the fake's own objects stay.
 *
 * Needs the fake Discord on 8090 (`node e2e/support/fake-discord.mjs`) and the server pointed at it, as
 * bridge.spec.ts. Skipped when the fake is not up.
 */

const FAKE = process.env.SR_FAKE_DISCORD_URL ?? 'http://127.0.0.1:8090';
const MANAGE_GUILD = 1 << 5;
const BAN_MEMBERS = 1 << 2;

const roleNamed = (s: FakeStructure, name: string) => s.roles.find((r) => r.name === name);
const channelNamed = (s: FakeStructure, name: string) => s.channels.find((c) => c.name === name);

test.describe('the whole-server Discord mirror', () => {
  test('plan, confirm, revert a Discord edit, rename and restore a channel, go back, switch off with options', async ({
    scenario,
    playwright,
  }) => {
    test.setTimeout(300_000);
    const control = await playwright.request.newContext();
    // A whole-server bridge left behind by a failed run keeps the fake's Discord server taken (the preflight's
    // unique-server check), so the next run in the same database could never pass: switch it off on the way out.
    let switchOff: (() => Promise<unknown>) | undefined;
    let finished = false;
    try {
      const up = await control.get(`${FAKE}/__fake/state`).catch(() => null);
      test.skip(!up?.ok(), 'The fake Discord (port 8090) is not running.');
      await fakeReset(control);

      const { a, guildId } = await scenario.ownerAndMember();
      const owner = a.page;
      switchOff = () =>
        apiPatch(a.context.request, `/guilds/${guildId}/sr/features/bridge.enabled`, {
          enabled: false,
        });
      const role = await apiPost<{ id: string }>(a.context.request, `/guilds/${guildId}/roles`, {
        name: 'Manager',
        permissions: String(MANAGE_GUILD),
      });
      await apiPatch(a.context.request, `/guilds/${guildId}/roles/${role.id}`, { hoist: true });

      // 1. Real: switch on and connect the bot.
      await owner.goto(`/settings/server/${guildId}/features`);
      await owner.getByRole('switch', { name: 'Enable Discord bridge' }).click();
      await owner.getByRole('button', { name: 'Discord bridge', exact: true }).click();
      await owner.getByLabel('Bot token').fill(FAKE_BOT_TOKEN);
      await owner.getByRole('button', { name: 'Connect the bot' }).click();
      await expect(owner.getByText(/^Connected to the Discord server/)).toBeVisible({
        timeout: 30_000,
      });
      const mode = owner.getByTestId('bridge-mode-card');
      await expect(mode.getByText('Mode: Only bridge these')).toBeVisible();

      // 2. Preflight and plan against the fake; the owner confirms.
      const planAndConfirm = async () => {
        await mode.getByRole('button', { name: 'Prepare the whole-server plan' }).click();
        const pre = owner.getByTestId('bridge-preflight');
        await expect(pre.getByText('Ready', { exact: true })).toHaveCount(5, { timeout: 20_000 });
        await expect(pre.getByText('Fix this first')).toHaveCount(0);
        await owner.getByRole('button', { name: 'Build the plan' }).click();
        await expect(owner.getByRole('tab', { name: /^Roles \(\d+\)$/ })).toBeVisible({
          timeout: 20_000,
        });
        await expect(owner.getByRole('cell', { name: 'Manager' }).first()).toBeVisible();
        await owner.getByRole('button', { name: 'Confirm the plan' }).click();
        const dialog = owner.getByRole('dialog', { name: 'Bridge the whole server?' });
        await expect(dialog).toContainText('decided by Discord');
        await dialog.getByRole('button', { name: 'Confirm and start' }).click();
        await expect(owner.getByTestId('bridge-sync-status').getByText('In sync')).toBeVisible({
          timeout: 60_000,
        });
      };
      await planAndConfirm();
      await expect(
        owner.getByText(/Everything in this server except direct messages/),
      ).toBeVisible();
      await expect
        .poll(async () => roleNamed(await fakeStructure(control), 'Manager')?.permissions, {
          timeout: 30_000,
        })
        .toBe(String(MANAGE_GUILD));
      const discordRole = roleNamed(await fakeStructure(control), 'Manager')!;
      const discordGuild = (await fakeStructure(control)).roles.find((r) => r.name === '@everyone')!
        .id as string;

      // 3. A Discord admin adds Ban Members: put back on Discord, logged here, never applied here.
      await fakeAdminEdit(control, 'PATCH', `/guilds/${discordGuild}/roles/${discordRole.id}`, {
        permissions: String(MANAGE_GUILD | BAN_MEMBERS),
      });
      await expect
        .poll(async () => roleNamed(await fakeStructure(control), 'Manager')?.permissions, {
          timeout: 30_000,
        })
        .toBe(String(MANAGE_GUILD));
      const log = owner.getByTestId('bridge-conflicts');
      await expect(async () => {
        await owner.reload();
        await expect(log.getByText('Permission change undone: Manager')).toBeVisible({
          timeout: 5_000,
        });
      }).toPass({ timeout: 45_000 });
      await expect(log.getByText(/^By /).first()).toBeVisible();
      await log.getByRole('button', { name: 'Apply on SIDE ROOM: Manager' }).first().click();
      await expect(owner.getByText('Prefilled from Discord’s change')).toBeVisible();
      const roles = (await (
        await a.context.request.get(`/api/v1/guilds/${guildId}/roles`)
      ).json()) as { id: string; permissions: string }[];
      expect(roles.find((r) => r.id === role.id)?.permissions).toBe(String(MANAGE_GUILD));

      // 4. Rename on Discord arrives here; a deletion on Discord is held and restorable.
      await fakeAdminEdit(control, 'PATCH', `/channels/${FAKE_CHANNEL_ID}`, {
        name: 'renamed-there',
      });
      const names = async () =>
        (
          (await (await a.context.request.get(`/api/v1/guilds/${guildId}/channels`)).json()) as {
            name: string;
          }[]
        ).map((c) => c.name);
      await expect.poll(names, { timeout: 30_000 }).toContain('renamed-there');
      await fakeAdminEdit(control, 'DELETE', `/channels/${FAKE_CHANNEL_ID}`);
      // (The hold is recorded and shown in the conflict log; hiding a held channel from the channel list and the
      // read routes is not built yet: integration handoff, open item.)
      await owner.goto(`/settings/server/${guildId}/bridge`);
      await expect(async () => {
        await owner.reload();
        await expect(log.getByText('Deleted on Discord, held here: renamed-there')).toBeVisible({
          timeout: 5_000,
        });
      }).toPass({ timeout: 45_000 });
      await log.getByRole('button', { name: 'Restore: renamed-there' }).click();
      await expect
        .poll(async () => channelNamed(await fakeStructure(control), 'renamed-there')?.id, {
          timeout: 30_000,
        })
        .toBeTruthy();
      await expect.poll(names, { timeout: 30_000 }).toContain('renamed-there');

      // 5. Back to Only bridge these: nothing is deleted on either side.
      await owner.getByRole('button', { name: 'Go back to Only bridge these' }).click();
      await owner.getByRole('button', { name: 'Go back', exact: true }).click();
      await expect(mode.getByText('Mode: Only bridge these')).toBeVisible();
      const kept = await fakeStructure(control);
      expect(roleNamed(kept, 'Manager')).toBeTruthy();
      expect(channelNamed(kept, 'general')).toBeTruthy();

      // 6. Whole server again, then switch off and remove what the bridge created there.
      await planAndConfirm();
      const before = await fakeStructure(control);
      expect(channelNamed(before, 'general')).toBeTruthy();
      await owner.getByRole('button', { name: 'Switch off the Discord bridge' }).click();
      const off = owner.getByRole('dialog', { name: 'Switch off the whole-server bridge?' });
      await off.getByRole('checkbox', { name: /Also remove what the bridge created/ }).check();
      await off.getByRole('button', { name: 'Switch off and remove' }).click();
      await expect(owner.getByText('The Discord bridge is off')).toBeVisible({ timeout: 30_000 });
      const after = await fakeStructure(control);
      // The channel the bridge created on Discord is gone; the fake's own roles stay.
      expect(channelNamed(after, 'general')).toBeFalsy();
      expect(after.roles.find((r) => r.name === '@everyone')).toBeTruthy();
      expect(after.roles.find((r) => r.name === 'bridge-bot')).toBeTruthy();
      finished = true;
    } finally {
      if (!finished) await switchOff?.().catch(() => undefined);
      await control.dispose();
    }
  });
});
