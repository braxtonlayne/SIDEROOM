import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-129 (D-623) and SEC-AUD-130 (D-626): Server Settings > Bridge with a connected bridge, two maps (one of
 * them shared with another server on this instance) and Discord messages of deleted accounts that could not be
 * removed. The status request is fulfilled by the page (the ordinary sweep shows the screen with no bridge). Editorial,
 * Lounge Light and Lounge Dark, desktop and phone. Local only (SR_P61_BRIDGE=1); shots go to SR_P61_BRIDGE_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P61_BRIDGE_OUT ?? resolve('test-results/p61-bridge');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P61_BRIDGE !== '1', 'local visual check only');
test.setTimeout(600_000);

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100 },
  );
  return c;
}

/** Make the guild's feature list say the bridge module is on, and answer the bridge's own calls. */
async function fakeBridge(
  page: Page,
  guildId: string,
  maps: { id: string; channel_id: string; shared: boolean; dch: string; name: string }[],
  withErasure: boolean,
) {
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/sr/features$`), async (route) => {
    const res = await route.fetch();
    const list = (await res.json()) as { id: string; enabled: boolean }[];
    await route.fulfill({
      response: res,
      json: list.map((m) => (m.id === 'bridge.enabled' ? { ...m, enabled: true } : m)),
    });
  });
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/bridge/status$`), (route) =>
    route.fulfill({
      json: {
        platform_on: true,
        module_on: true,
        connected: true,
        bridge: {
          discord_guild_id: '1100000000000000001',
          discord_guild_name: 'Friends',
          bot_user_id: '1100000000000000002',
          bot_name: 'Relay',
          mode: 'selective',
          status: 'connected',
          intents_granted: (1 << 15) | 1,
          created_by: '1',
          created_at: 0,
          invite_url: 'https://discord.com/oauth2/authorize?client_id=1&scope=bot',
        },
        maps: maps.map((m) => ({
          id: m.id,
          channel_id: m.channel_id,
          discord_channel_id: m.dch,
          discord_channel_name: m.name,
          direction: 'both',
          has_webhook: true,
          queued: 0,
          dead: 0,
          shared_on_instance: m.shared,
          created_at: 0,
        })),
        queued: 0,
        dead_letters: withErasure ? 2 : 0,
        leftovers: [],
        erasure_leftovers: withErasure
          ? [
              { discord_channel_id: maps[0]!.dch, discord_message_id: '1170000000000000001' },
              {
                discord_channel_id: '1100000000000000099',
                discord_message_id: '1170000000000000002',
              },
            ]
          : [],
        is_owner: true,
      },
    }),
  );
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/bridge/discord-channels$`), (route) =>
    route.fulfill({
      json: maps.map((m) => ({ id: m.dch, name: m.name })),
    }),
  );
}

test('the bridge screen says when a Discord channel is shared and lists the erasure leftovers', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    octx.request,
    '/guilds',
    { name: 'Bridge Booth' },
  );
  const general = guild.channels.find((c) => c.type === 0)!;
  const chat = await apiPost<{ id: string }>(octx.request, `/guilds/${guild.id}/channels`, {
    name: 'chat',
    type: 0,
  });
  const maps = [
    {
      id: '1200000000000000001',
      channel_id: general.id,
      shared: true,
      dch: '1100000000000000010',
      name: 'game-night',
    },
    {
      id: '1200000000000000002',
      channel_id: chat.id,
      shared: false,
      dch: '1100000000000000011',
      name: 'off-topic',
    },
  ];

  for (const theme of THEMES) {
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const shot = async (name: string) => {
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };
      await fakeBridge(page, guild.id, maps, true);
      await page.goto(`/settings/server/${guild.id}/bridge`);
      const table = page.getByRole('table', { name: 'Mirrored channels' });
      await expect(table).toBeVisible();
      await expect(
        page.getByText(/Another server on this instance also bridges this Discord channel/),
      ).toHaveCount(1);
      await table.scrollIntoViewIfNeeded();
      await shot('bridge-maps');
      const erasure = page.getByRole('heading', { name: 'Messages to delete on Discord' });
      await expect(erasure).toBeVisible();
      await expect(page.getByText('#game-night, message 1170000000000000001')).toBeVisible();
      await expect(
        page.getByText('#1100000000000000099, message 1170000000000000002'),
      ).toBeVisible();
      await erasure.scrollIntoViewIfNeeded();
      await shot('bridge-erasure');

      // The same screen with nothing to say: no warning, no list.
      await page.unrouteAll({ behavior: 'ignoreErrors' });
      await fakeBridge(
        page,
        guild.id,
        maps.map((m) => ({ ...m, shared: false })),
        false,
      );
      await page.goto(`/settings/server/${guild.id}/bridge`);
      await expect(table).toBeVisible();
      await expect(page.getByText(/Another server on this instance/)).toHaveCount(0);
      await expect(
        page.getByRole('heading', { name: 'Messages to delete on Discord' }),
      ).toHaveCount(0);
      await table.scrollIntoViewIfNeeded();
      await shot('bridge-clean');
      await c.close();
    }
  }
});
