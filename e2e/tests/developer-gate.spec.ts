import { expect, test, apiPost, type SignedIn } from '../support/fixtures';
import { startBot } from '../support/bot';
import { Person } from '../sdk-compat/lib/provision.mjs';

/**
 * RDY-DEV-005, the Phase 6 exit gate (doc 13): a developer's whole path, in one spec, in the manner of
 * `exit-gate.spec.ts`.
 *
 *  1. A member opens the developer portal, registers an application (the bot is created with it) and copies the
 *     bot token from the "Save your credentials" dialog.
 *  2. The browser walks the OAuth2 authorize page and adds the bot to the member's server with the
 *     `bot applications.commands` scopes.
 *  3. A Node bot (the in-repo `@sideroom/bot`, started with the copied token) connects, registers a slash command
 *     and answers the interaction; the member runs the command in the Lounge composer and reads the reply.
 *  4. The member fills in the release details, submits the application for review; staff (the Owner) approve; a
 *     second person installs the now released application on their own server through the same authorize page.
 *
 * It needs the gateway and the REST API only (no object storage, no voice), so it runs in the `playwright` job
 * as its own step (`--grep @gate`). The staff calls go through `Person` of `e2e/sdk-compat/lib/provision.mjs`,
 * the helper the SDK compatibility run uses, instead of a second copy of it.
 */

const API_ORIGIN = 'http://127.0.0.1:8080';
const PUBLIC = process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173';
// VIEW_CHANNEL, SEND_MESSAGES, READ_MESSAGE_HISTORY.
const BOT_PERMISSIONS = String((1 << 10) | (1 << 11) | (1 << 16));

interface GuildMade {
  id: string;
  channels: { id: string; type: number }[];
}

async function makeServer(u: SignedIn, name: string) {
  const g = await apiPost<GuildMade>(u.context.request, '/guilds', { name });
  const channel = g.channels.find((c) => c.type === 0);
  if (!channel) throw new Error('new server has no text channel');
  return { guildId: g.id, channelId: channel.id, name };
}

test.describe('@gate Phase 6 developer exit gate', () => {
  test('portal, OAuth2 bot install, slash command answered, reviewed release installs elsewhere', async ({
    seedUser,
    signedIn,
    owner,
  }) => {
    test.setTimeout(300_000);
    const stamp = Date.now().toString(36);
    const dev = await signedIn(await seedUser('devgate'));
    const other = await signedIn(await seedUser('devgate2'));
    const server = await makeServer(dev, `Bot test ${stamp}`);
    const second = await makeServer(other, `Second ${stamp}`);
    const appName = `Gate bot ${stamp}`;
    const commandName = `gate${stamp.replace(/[^a-z0-9]/g, '')}`.slice(0, 30);
    const page = dev.page;
    let bot: ReturnType<typeof startBot> | null = null;

    try {
      // --- 1. The developer portal: register the application and copy the token.
      await page.goto('/developers/applications');
      await page.getByRole('button', { name: 'New application' }).first().click();
      const create = page.getByRole('dialog', { name: 'New application' });
      await create.getByLabel('Name').fill(appName);
      await create.getByRole('checkbox').check();
      await create.getByRole('button', { name: 'Create', exact: true }).click();

      const secrets = page.getByRole('dialog', { name: 'Save your credentials' });
      await expect(secrets).toBeVisible();
      const token = await secrets.getByLabel('Bot token').inputValue();
      expect(token.length, 'a bot token was shown').toBeGreaterThan(20);
      await secrets.getByRole('button', { name: 'Done' }).click();
      await expect(page).toHaveURL(/\/developers\/applications\/\d+/);
      const appId = /\/applications\/(\d+)/.exec(page.url())![1]!;
      await expect(page.getByRole('heading', { name: appName }).first()).toBeVisible();

      // --- 2. OAuth2: add the bot to the server with the command scope.
      await page.goto(
        `/oauth2/authorize?client_id=${appId}&scope=${encodeURIComponent('bot applications.commands')}` +
          `&permissions=${BOT_PERMISSIONS}`,
      );
      await page.getByLabel('Add to server').selectOption({ label: server.name });
      await page.getByRole('button', { name: 'Authorize', exact: true }).click();
      await expect(page.getByRole('heading', { name: 'Authorized' })).toBeVisible();

      // --- 3. The bot connects, registers its command and answers it.
      bot = startBot({
        apiUrl: `${API_ORIGIN}/api/v1`,
        token,
        command: { name: commandName, description: 'Answers from the gate bot' },
        reply: `pong from ${stamp}`,
      });
      const ready = await bot.ready;
      expect(ready.applicationId).toBe(appId);
      await expect
        .poll(
          async () => {
            const res = await fetch(`${API_ORIGIN}/api/v1/applications/${appId}/commands`, {
              headers: { Authorization: `Bot ${token}` },
            });
            if (!res.ok) return [];
            return ((await res.json()) as { name: string }[]).map((c) => c.name);
          },
          { message: 'the bot registered its command' },
        )
        .toContain(commandName);

      // The composer lists app commands per channel and keeps the list for a minute: open the channel only now.
      await page.goto(`/channels/${server.guildId}/${server.channelId}`);
      const box = page.getByRole('textbox', { name: /^Message/ });
      await box.click();
      await box.fill(`/${commandName}`);
      await expect(page.getByRole('option', { name: new RegExp(`/${commandName}`) })).toBeVisible({
        timeout: 20_000,
      });
      await box.press('Enter'); // takes the suggestion
      await box.press('Enter'); // runs the command
      await expect(page.getByText(`pong from ${stamp}`)).toBeVisible({ timeout: 30_000 });
      expect(bot.answered).toContain(commandName);
      expect(bot.errors, 'the bot reported no errors').toEqual([]);

      // --- 4. Release: details, submit for review, staff approve, a second person installs it.
      await page.goto(`/developers/applications/${appId}?tab=general`);
      await page.getByLabel(/Privacy policy/).fill('https://example.test/privacy');
      await page.getByLabel(/Terms of service/).fill('https://example.test/terms');
      await page.getByRole('button', { name: 'Save changes' }).click();
      await expect(page.getByRole('button', { name: 'Save changes' })).toBeDisabled();

      await page.goto(`/developers/applications/${appId}?tab=review`);
      await page.getByRole('button', { name: 'Submit for review' }).click();
      await page
        .getByRole('dialog', { name: 'Submit for review?' })
        .getByRole('button', { name: 'Submit', exact: true })
        .click();
      await expect(page.getByText('Submitted for review.')).toBeVisible();

      // Staff: the Owner reviews the queue entry of this application and approves its version.
      const staff = new Person(API_ORIGIN, PUBLIC);
      await staff.login(owner.username, owner.password);
      const queue = await staff.get<{ id: string; application_id: string }[]>('/admin/developers/versions');
      const entry = queue.find((v) => v.application_id === appId);
      expect(entry, 'the version is in the staff review queue').toBeTruthy();
      await staff.post(`/admin/developers/versions/${entry!.id}/decision`, { decision: 'approve' });

      await page.reload();
      await expect(page.getByText('Approved').first()).toBeVisible();

      // A second person, who is nobody's tester, installs the released application on their own server.
      await other.page.goto(
        `/oauth2/authorize?client_id=${appId}&scope=${encodeURIComponent('bot applications.commands')}`,
      );
      await other.page.getByLabel('Add to server').selectOption({ label: second.name });
      await other.page.getByRole('button', { name: 'Authorize', exact: true }).click();
      await expect(other.page.getByRole('heading', { name: 'Authorized' })).toBeVisible();

      // The install is real: the bot is a member of the second server.
      const members = await other.context.request.get(
        `/api/v1/guilds/${second.guildId}/members?limit=10`,
      );
      expect(members.ok()).toBeTruthy();
      expect(
        ((await members.json()) as { user: { id: string; bot?: boolean } }[]).some(
          (m) => m.user.bot,
        ),
        'the bot joined the second server',
      ).toBe(true);
    } finally {
      bot?.stop();
    }
  });
});
