import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
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
 * SEC-AUD-184 (wave 66): what a member sees when a create or a join meets one of the two server limits
 * (D-659, D-660): the create-server dialog, Join a Server, the invite page, the Server Directory card,
 * Server Settings > applications (approve) and the add-an-app consent page, each with the server's answer
 * faked by `page.route` (30001 and 30019), and Admin > Settings with the two new fields (set one, save,
 * reload). Editorial, Lounge Light and Lounge Dark, desktop and phone. Local only (SR_P66_CAPS=1); shots go to
 * SR_P66_CAPS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P66_CAPS_OUT ?? resolve('test-results/p66-caps');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P66_CAPS !== '1', 'local visual check only');
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

const json = (r: Route, body: unknown, status = 200) =>
  r.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });
const tooMany = (r: Route) =>
  json(r, { code: 30001, message: 'Maximum number of guilds reached (200)' }, 400);
const full = (r: Route) =>
  json(r, { code: 30019, message: 'Maximum number of server members reached' }, 400);

const MAX_SERVERS =
  'You’re in the most servers one account can be in here (200). Leave one to join or create another.';

async function pause(page: Page) {
  await page.waitForTimeout(400);
}

test('the limits say why, in every dialog that creates or joins', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Limit Booth' },
  );
  const channelId = guild.channels.filter((c) => c.type === 0)[0]!.id;
  const invite = await apiPost<{ code: string }>(o, `/channels/${channelId}/invites`, {});

  // An application waiting in Server Settings: the form is on, a second member has applied.
  await apiPut(o, `/guilds/${guild.id}/sr/applications/form`, {
    enabled: true,
    description: 'Tell us about you',
    questions: [{ label: 'Why join?', kind: 'short', required: false }],
  });
  const rctx = await browser.newContext({ baseURL: ORIGIN });
  const applicant = await registerUser(rctx.request, 'applicant');
  await rctx.close();
  const actx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(actx.request, applicant);
  await apiPost(actx.request, `/guilds/${guild.id}/sr/applications`, { answers: [] });
  await actx.close();

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const shot = async (name: string) => {
        await pause(page);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };
      const addServer = async () => {
        await page.goto(`/channels/${guild.id}/${channelId}`);
        const open = page.getByRole('button', { name: 'Add a Server' });
        await open.click();
      };

      // 1. Create My Own at the account limit (30001, with the number).
      await page.route(/\/api\/v1\/guilds$/, (r) =>
        r.request().method() === 'POST' ? tooMany(r) : r.fallback(),
      );
      await addServer();
      await page.getByRole('button', { name: 'Create My Own' }).click();
      await page.getByRole('button', { name: 'skip this question' }).click();
      await page.getByLabel('Server name').fill('One more');
      await page.getByRole('button', { name: 'Create' }).click();
      await expect(page.getByText(MAX_SERVERS)).toBeVisible();
      await shot('create-at-limit');
      await page.unrouteAll({ behavior: 'ignoreErrors' });

      // 2. Join a Server: the server is full (30019), then the account limit.
      let answer = full;
      await page.route(/\/api\/v1\/invites\/[^/]+$/, (r) =>
        r.request().method() === 'POST' ? answer(r) : r.fallback(),
      );
      await addServer();
      await page.getByRole('button', { name: 'Join a Server' }).click();
      await page.getByLabel('Invite link or code').fill('abc123');
      await page.getByRole('button', { name: 'Join Server' }).click();
      await expect(page.getByText('This server is full.')).toBeVisible();
      await shot('join-server-full');
      answer = tooMany;
      await page.getByRole('button', { name: 'Join Server' }).click();
      await expect(page.getByText(MAX_SERVERS)).toBeVisible();
      await shot('join-at-limit');

      // 3. The invite page.
      await page.goto(`/invite/${invite.code}`);
      await page.getByRole('button', { name: 'Accept invite' }).click();
      await expect(page.getByRole('alert')).toContainText(/most servers|full/);
      await shot('invite-page');
      await page.unrouteAll({ behavior: 'ignoreErrors' });

      // 4. The Server Directory card (the listing and the join are faked).
      await page.route(/\/api\/v1\/directory(\?.*)?$/, (r) =>
        json(r, [
          {
            id: '4242',
            name: 'Chess Club',
            description: 'Openings and endgames.',
            icon: null,
            banner: null,
            category: 'gaming',
            featured: false,
            approximate_member_count: 25000,
            preferred_locale: 'en-US',
            vanity_url_code: null,
            joined: false,
          },
        ]),
      );
      await page.route(/\/api\/v1\/directory\/categories$/, (r) => json(r, ['gaming']));
      await page.route(/\/api\/v1\/directory\/4242\/join$/, full);
      await page.goto(`/channels/${guild.id}/${channelId}`);
      await page.getByRole('button', { name: 'Discover' }).click();
      const dir = page.getByRole('dialog', { name: 'Server Directory' });
      await dir
        .getByRole('listitem', { name: 'Chess Club' })
        .getByRole('button', { name: 'Join' })
        .click();
      await expect(dir.getByText('This server is full.')).toBeVisible();
      await shot('directory-full');
      await page.unrouteAll({ behavior: 'ignoreErrors' });

      // 5. Server Settings > applications: approving meets the applicant's limit.
      await page.route(
        new RegExp(`/api/v1/guilds/${guild.id}/sr/applications/\\d+/decision$`),
        (r) =>
          json(
            r,
            { code: 30001, message: 'This member is in the most servers one account can be in.' },
            400,
          ),
      );
      await page.goto(`/settings/server/${guild.id}/community`);
      await page.getByRole('tab', { name: 'Access' }).click();
      await page.getByRole('button', { name: /^Approve / }).click();
      await expect(page.getByText(/application stays pending/)).toBeVisible();
      await shot('application-approve-at-limit');
      await page.unrouteAll({ behavior: 'ignoreErrors' });
      await page.route(
        new RegExp(`/api/v1/guilds/${guild.id}/sr/applications/\\d+/decision$`),
        full,
      );
      await page.getByRole('button', { name: /^Approve / }).click();
      await expect(page.getByText('This server is full.')).toBeVisible();
      await shot('application-approve-full');
      await page.unrouteAll({ behavior: 'ignoreErrors' });

      // 6. Adding an app to a server (the consent page; the preview and the answer are faked).
      await page.route(/\/api\/v1\/oauth2\/authorize(\?.*)?$/, (r) => {
        if (r.request().method() === 'POST') return full(r);
        return json(r, {
          application: {
            id: '500',
            name: 'Jukebox',
            description: 'Plays songs in voice.',
            icon: null,
            bot_public: true,
            release_state: 'released',
            privacy_policy_url: null,
            terms_of_service_url: null,
          },
          bot: { id: '501', username: 'jukebox', avatar: null },
          scopes: [
            { name: 'identify', description: 'See who you are' },
            { name: 'bot', description: 'Add a bot' },
          ],
          redirect_uri: null,
          state: null,
          user: { id: '1', username: 'owner', global_name: 'Owner' },
          guilds: [
            {
              id: guild.id,
              name: 'Limit Booth',
              icon: null,
              owner: true,
              permissions: '8',
              can_install: true,
            },
          ],
          permissions: '3072',
          already_authorized: false,
        });
      });
      await page.goto('/oauth2/authorize?client_id=500&scope=identify+bot');
      await page.getByLabel('Add to server').selectOption(guild.id);
      await page.getByRole('button', { name: 'Authorize' }).click();
      await expect(
        page.getByText('This server is full, so the app cannot be added.'),
      ).toBeVisible();
      await shot('add-app-full');
      await page.unrouteAll({ behavior: 'ignoreErrors' });
      await c.close();
    }
  }
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
});

test('Admin > Settings lists the two limits; a changed value is saved and survives a reload', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto('/admin/limits');
      const perUser = page.getByLabel('guilds.max_per_user');
      await expect(perUser).toBeVisible();
      await expect(page.getByLabel('guilds.max_members')).toBeVisible();
      await perUser.scrollIntoViewIfNeeded();
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `admin-settings-${tag}.png`) });
      await perUser.fill('150');
      await page.getByRole('button', { name: /^Save 1 change/ }).click();
      await page.getByRole('textbox').last().fill('Checking the new limit');
      await page.getByRole('button', { name: 'Save settings' }).click();
      await expect(page.getByText('Settings saved. They are live now.')).toBeVisible();
      await page.reload();
      await expect(page.getByLabel('guilds.max_per_user')).toHaveValue('150');
      await page.getByLabel('guilds.max_per_user').scrollIntoViewIfNeeded();
      await pause(page);
      await page.screenshot({ path: resolve(OUT, `admin-settings-saved-${tag}.png`) });
      // Back to the default so the next theme starts from the same page.
      await apiPatch(o, '/admin/settings', {
        values: { 'guilds.max_per_user': 200 },
        reason: 'reset',
      });
      await c.close();
    }
  }
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
});
