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
  test,
} from '../support/fixtures';

/**
 * SEC-AUD-036 / 037 (wave 54): the error states a failed action or load now shows (history load, action toasts,
 * notification dialog, poll voters, profile and activity settings, server extensions), in Editorial, Lounge Light
 * and Lounge Dark, desktop and phone. Local only (SR_P54_WEBA=1); shots go to SR_P54_WEBA_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P54_WEBA_OUT ?? resolve('test-results/p54-weba');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const GUILD = 'Silent Failure Booth';

test.skip(process.env.SR_P54_WEBA !== '1', 'local visual check only');
test.setTimeout(1_200_000);

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

const boom = (r: Route) =>
  r.fulfill({ status: 500, contentType: 'application/json', body: '{"message":"boom"}' });
/** Fail the matching requests of one method (GET by default); everything else goes through. */
const failOn = (page: Page, glob: string | RegExp, method = 'GET') =>
  page.route(glob, (r) => (r.request().method() === method ? boom(r) : r.fallback()));

async function openServerMenu(page: Page, vp: string, guildId: string) {
  if (vp === 'phone') {
    await page
      .getByRole('button', { name: new RegExp(`^${GUILD}`) })
      .first()
      .click();
  } else {
    const rail = page.getByRole('navigation', { name: 'Servers' });
    await rail.locator(`a[href^="/channels/${guildId}"]`).click({ button: 'right' });
  }
}

test('silent failures now say so', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: GUILD },
  );
  const text = guild.channels.filter((c) => c.type === 0);
  const channelId = text[0]!.id;
  // A second channel whose history the page is told failed.
  const quiet = await apiPost<{ id: string }>(o, `/guilds/${guild.id}/channels`, {
    name: 'quiet-room',
    type: 0,
  });
  const m = await apiPost<{ id: string }>(o, `/channels/${channelId}/messages`, {
    content: 'react to me',
  });
  await apiPut(
    o,
    `/channels/${channelId}/messages/${m.id}/reactions/${encodeURIComponent('👍')}/@me`,
  );
  const poll = await apiPost<{ id: string }>(o, `/channels/${channelId}/messages`, {
    content: '',
    poll: {
      question: { text: 'Tea or coffee?' },
      answers: [{ poll_media: { text: 'Tea' } }, { poll_media: { text: 'Coffee' } }],
      duration: 24,
      allow_multiselect: false,
    },
  });
  await apiPut(o, `/channels/${channelId}/polls/${poll.id}/answers/1/voters/@me`);

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const shot = async (name: string) => {
        await page.waitForTimeout(400);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };

      // 1. A failed history load: an error with Try again, not "the start of #quiet-room".
      await failOn(page, new RegExp(`/api/v1/channels/${quiet.id}/messages(\\?|$)`));
      await page.goto(`/channels/${guild.id}/${quiet.id}`);
      await expect(page.getByText('Messages could not be loaded.')).toBeVisible();
      await shot('history-failed');
      await page.unrouteAll({ behavior: 'ignoreErrors' });

      // 2. A failed reaction removal: the pill stays and a toast says so.
      await failOn(page, /\/reactions\//, 'DELETE');
      await page.goto(`/channels/${guild.id}/${channelId}`);
      const log = page.getByRole('log');
      await log
        .getByRole('button', { name: /1 reaction/ })
        .first()
        .click();
      await expect(page.getByText('Your reaction was not saved. Try again.')).toBeVisible();
      await shot('toast-reaction-failed');

      // 3. Poll voters that failed to load are not "Nobody".
      await failOn(page, new RegExp(`/polls/${poll.id}/answers/1(\\?|$)`));
      await log
        .getByRole('button', { name: /Voters.*Tea/ })
        .first()
        .evaluate((b) => (b as HTMLButtonElement).click());
      await expect(page.getByText('The voters could not be loaded.')).toBeVisible();
      await shot('poll-voters-failed');
      await page.keyboard.press('Escape');

      // 4. A failed status change (the user panel is on the desktop layout; a phone has it under You).
      if (vp === 'desktop') {
        await failOn(page, /\/api\/v1\/users\/@me\/presence$/, 'PATCH');
        await failOn(page, /\/api\/v1\/users\/@me\/presence$/, 'PUT');
        await page.getByRole('button', { name: 'Set status' }).first().click();
        await page.getByRole('menuitem', { name: 'Idle' }).click();
        await expect(page.getByText('Your status was not changed. Try again.')).toBeVisible();
        await shot('toast-status-failed');
      }
      await page.unrouteAll({ behavior: 'ignoreErrors' });

      // 5. The notification settings dialog when its load fails.
      await failOn(page, /\/api\/v1\/users\/@me\/guild-settings/);
      await failOn(page, new RegExp(`/api/v1/users/@me/guilds/${guild.id}/settings`));
      // A phone opens the server menu from the channel list (no rail).
      await page.goto(
        vp === 'phone' ? `/channels/${guild.id}` : `/channels/${guild.id}/${channelId}`,
      );
      await page.waitForTimeout(2000);
      await openServerMenu(page, vp, guild.id);
      await page.getByRole('menuitem', { name: 'Notification Settings' }).click();
      await expect(
        page.getByText('Your notification settings for this server could not be loaded.'),
      ).toBeVisible();
      await shot('notif-dialog-failed');
      await page.unrouteAll({ behavior: 'ignoreErrors' });

      // 6. Settings > Profile: a failed load now has Try again.
      await failOn(page, /\/api\/v1\/users\/@me\/profile$/);
      await page.goto('/settings/user/profile');
      await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible();
      await shot('settings-profile-failed');
      await page.unrouteAll({ behavior: 'ignoreErrors' });

      // 7. Settings > Activity Privacy: locked switches while the account copy cannot be read.
      await failOn(page, /\/api\/v1\/users\/@me\/settings$/);
      await page.goto('/settings/user/activity');
      await expect(page.getByText(/sharing choices could not be loaded/)).toBeVisible();
      await shot('settings-activity-failed');
      await page.unrouteAll({ behavior: 'ignoreErrors' });

      // 8. Server Settings > Extensions: the turned-on list failed to load.
      await failOn(page, new RegExp(`/api/v1/guilds/${guild.id}/packages$`));
      await page.goto(`/settings/server/${guild.id}/extensions`);
      await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible();
      await shot('server-extensions-failed');
      await c.close();
    }
  }
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
});
