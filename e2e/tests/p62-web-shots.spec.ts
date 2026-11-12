import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import {
  apiPatch,
  apiPost,
  apiPut,
  bootstrapState,
  expect,
  loginApi,
  registerUser,
  test,
} from '../support/fixtures';

/**
 * SEC-AUD-146 (the bridge's map table on a phone) and SEC-AUD-147 (D-638: Block, Remove Friend, End Poll and End
 * Stage ask first). Editorial, Lounge Light and Lounge Dark, desktop and phone. Local only (SR_P62_WEB=1); shots go
 * to SR_P62_WEB_OUT. The bridge status is fulfilled by the page; the stage instance is faked (no voice backend).
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P62_WEB_OUT ?? resolve('test-results/p62-web');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const LONG = 'announcements-and-events';

test.skip(process.env.SR_P62_WEB !== '1', 'local visual check only');
test.setTimeout(1_800_000);

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    colorScheme: theme === 'lounge-dark' ? 'dark' : 'light',
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

type MapSpec = { id: string; channel_id: string; shared: boolean; dch: string; name: string };

async function fakeBridge(page: Page, guildId: string, maps: MapSpec[], removed: string[]) {
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
        dead_letters: 0,
        leftovers: [],
        erasure_leftovers: [],
        is_owner: true,
      },
    }),
  );
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/bridge/discord-channels$`), (route) =>
    route.fulfill({ json: maps.map((m) => ({ id: m.dch, name: m.name })) }),
  );
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/bridge/maps/[^/]+$`), (route) => {
    if (route.request().method() !== 'DELETE') return route.fallback();
    removed.push(route.request().url().split('/').pop()!);
    return route.fulfill({ status: 204 });
  });
}

test('the bridge map table, and Block, Remove Friend, End Poll and End Stage ask first', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const ownerId = ((await (await o.get('/api/v1/users/@me')).json()) as { id: string }).id;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Confirm Booth' },
  );
  const general = guild.channels.find((c) => c.type === 0)!;
  const long = await apiPost<{ id: string }>(o, `/guilds/${guild.id}/channels`, {
    name: LONG,
    type: 0,
  });
  const stage = await apiPost<{ id: string }>(o, `/guilds/${guild.id}/channels`, {
    name: 'Town Hall',
    type: 13,
  });
  const maps: MapSpec[] = [
    {
      id: '1200000000000000001',
      channel_id: long.id,
      shared: true,
      dch: '1100000000000000010',
      name: LONG,
    },
    {
      id: '1200000000000000002',
      channel_id: general.id,
      shared: false,
      dch: '1100000000000000011',
      name: 'off-topic',
    },
  ];
  await apiPost(o, `/channels/${general.id}/messages`, {
    content: '',
    poll: {
      question: { text: 'Tea or coffee?' },
      answers: [{ poll_media: { text: 'Tea' } }, { poll_media: { text: 'Coffee' } }],
      duration: 24,
      allow_multiselect: false,
    },
  });

  const friends: { id: string; username: string }[] = [];
  for (const label of ['amy', 'ben']) {
    const rctx = await browser.newContext({ baseURL: ORIGIN });
    const u = await registerUser(rctx.request, label);
    await rctx.close();
    const uctx = await browser.newContext({ baseURL: ORIGIN });
    await loginApi(uctx.request, u);
    await apiPut(o, `/users/@me/relationships/${u.id}`, { type: 1 });
    await apiPut(uctx.request, `/users/@me/relationships/${ownerId}`, { type: 1 });
    await uctx.close();
    friends.push(u);
  }
  const [amy, ben] = friends as [(typeof friends)[number], (typeof friends)[number]];
  const dm = await apiPost<{ id: string }>(o, '/users/@me/channels', { recipient_id: amy.id });

  const stageInstance = [
    { channel_id: stage.id, guild_id: guild.id, topic: 'Quarterly town hall', privacy_level: 2 },
  ];

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const shot = async (name: string) => {
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };

      // 1. The bridge's map table: stacked blocks on a phone, Remove reachable, the warning inside its map.
      const removed: string[] = [];
      await fakeBridge(page, guild.id, maps, removed);
      await page.goto(`/settings/server/${guild.id}/bridge`);
      const table = page.getByRole('table', { name: 'Mirrored channels' });
      await expect(table).toBeVisible();
      await table.scrollIntoViewIfNeeded();
      await shot('bridge-maps');
      const scrollW = await page.evaluate(() => document.documentElement.scrollWidth);
      expect(scrollW).toBeLessThanOrEqual(size.width);
      const removes = page.getByRole('button', { name: /^Stop mirroring #/ });
      await expect(removes).toHaveCount(2);
      for (const i of [0, 1]) {
        const box = await removes.nth(i).boundingBox();
        expect(box!.x).toBeGreaterThanOrEqual(0);
        expect(box!.x + box!.width).toBeLessThanOrEqual(size.width);
      }
      if (vp === 'phone') {
        await removes.nth(1).scrollIntoViewIfNeeded();
        await shot('bridge-maps-remove');
        await removes.nth(1).click();
        await expect.poll(() => removed).toEqual([maps[1]!.id]);
      }
      await page.unrouteAll({ behavior: 'ignoreErrors' });

      // 2. Friends page: Remove Friend and Block ask; Cancel calls nothing.
      await page.goto('/channels/@me');
      await page.getByRole('tab', { name: 'All' }).click();
      const more = page.getByRole('button', {
        name: new RegExp(`More actions for .*${amy.username}`, 'i'),
      });
      await more.click();
      await page.getByRole('menuitem', { name: 'Remove Friend' }).click();
      const rf = page.getByRole('dialog', { name: /^Remove .* as a friend\?$/ });
      await expect(rf).toBeVisible();
      await shot('confirm-remove-friend');
      await rf.getByRole('button', { name: 'Cancel' }).click();
      await expect(rf).toHaveCount(0);
      await more.click();
      await page.getByRole('menuitem', { name: 'Block' }).click();
      const bl = page.getByRole('dialog', { name: /^Block .*\?$/ });
      await expect(bl).toBeVisible();
      await shot('confirm-block');
      await page.keyboard.press('Escape');
      await expect(bl).toHaveCount(0);

      // 3. End Poll.
      await page.goto(`/channels/${guild.id}/${general.id}`);
      await page.getByRole('button', { name: 'End Poll' }).click();
      const ep = page.getByRole('dialog', { name: 'End this poll now?' });
      await expect(ep).toBeVisible();
      await shot('confirm-end-poll');
      await ep.getByRole('button', { name: 'Cancel' }).click();
      await expect(ep).toHaveCount(0);

      // 4. End Stage (the instance is faked).
      let ended = 0;
      await page.route(new RegExp(`/api/v1/guilds/${guild.id}/stage-instances$`), (r) =>
        r.fulfill({ json: stageInstance }),
      );
      await page.route(new RegExp(`/api/v1/stage-instances/${stage.id}$`), (r) => {
        if (r.request().method() !== 'DELETE') return r.fallback();
        ended += 1;
        return r.fulfill({ status: 204 });
      });
      await page.goto(`/channels/${guild.id}/${stage.id}`);
      await page.getByRole('button', { name: 'End Stage' }).click();
      const es = page.getByRole('dialog', { name: 'End the stage for everyone?' });
      await expect(es).toBeVisible();
      await shot('confirm-end-stage');
      await es.getByRole('button', { name: 'Cancel' }).click();
      await expect(es).toHaveCount(0);
      expect(ended).toBe(0);
      if (vp === 'desktop' && theme === 'lounge-editorial') {
        // Confirm once for the stage (faked), here.
        await page.getByRole('button', { name: 'End Stage' }).click();
        await es.getByRole('button', { name: 'End Stage' }).click();
        await expect.poll(() => ended).toBe(1);
      }
      await page.unrouteAll({ behavior: 'ignoreErrors' });

      // 5. DM welcome: Block asks first (shot); Confirm once at the very end, for real.
      await page.goto(`/channels/@me/${dm.id}`);
      await page.getByRole('button', { name: 'Block', exact: true }).first().click();
      const dmb = page.getByRole('dialog', { name: /^Block .*\?$/ });
      await expect(dmb).toBeVisible();
      await shot('confirm-block-dm');
      await dmb.getByRole('button', { name: 'Cancel' }).click();
      await c.close();
    }
  }

  // Confirm once each, for real: End Poll, Remove Friend (DM welcome), Block (Friends page).
  const c = await context(browser, octx, VIEWPORTS.desktop, 'lounge-editorial');
  const page = await c.newPage();
  page.setDefaultTimeout(20_000);
  await page.goto(`/channels/${guild.id}/${general.id}`);
  await page.getByRole('button', { name: 'End Poll' }).click();
  await page
    .getByRole('dialog', { name: 'End this poll now?' })
    .getByRole('button', { name: 'End Poll' })
    .click();
  await expect(page.getByRole('button', { name: 'End Poll' })).toHaveCount(0);
  await page.goto(`/channels/@me/${dm.id}`);
  await page.getByRole('button', { name: 'Remove Friend' }).first().click();
  await page
    .getByRole('dialog', { name: /^Remove .* as a friend\?$/ })
    .getByRole('button', { name: 'Remove Friend' })
    .click();
  await expect(page.getByRole('button', { name: 'Remove Friend' })).toHaveCount(0);
  await page.goto('/channels/@me');
  await page.getByRole('tab', { name: 'All' }).click();
  await page
    .getByRole('button', { name: new RegExp(`More actions for .*${ben.username}`, 'i') })
    .click();
  await page.getByRole('menuitem', { name: 'Block' }).click();
  await page
    .getByRole('dialog', { name: /^Block .*\?$/ })
    .getByRole('button', { name: 'Block', exact: true })
    .click();
  await expect(
    page.getByRole('button', { name: new RegExp(`More actions for .*${ben.username}`, 'i') }),
  ).toHaveCount(0);
  await c.close();
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
});
