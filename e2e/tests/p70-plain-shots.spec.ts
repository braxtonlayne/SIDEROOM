import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext } from '@playwright/test';
import {
  apiPatch,
  apiPost,
  bootstrapState,
  expect,
  loginApi,
  registerUser,
  test,
} from '../support/fixtures';

/**
 * SEC-AUD-242 (wave 70; D-728), on the real stack, two accounts: the latest message of a thread and of a Message
 * Request is `look ||` + a long sentence + `||`. The thread row and the request row show "look" and an ellipsis
 * (or the word for a spoiler), never the sentence, whether the row is loaded from the server or arrives live.
 * Editorial, Lounge Light and Lounge Dark, desktop and phone. Local only (SR_P70_PLAIN=1); shots go to
 * SR_P70_PLAIN_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P70_PLAIN_OUT ?? resolve('test-results/p70-plain');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P70_PLAIN !== '1', 'local visual check only');
test.setTimeout(1_800_000);

const SECRET = 'swordfish';
const spoiler = (tag: string) =>
  `look ||${`the ${SECRET} is under the third stone from the left (${tag}), `.repeat(4)}||`;

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

const pause = (ms = 1300) => new Promise((r) => setTimeout(r, ms));

test('a long spoiler never reaches a thread row or a request row, loaded or live', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const a = octx.request;
  const me = await (await a.get('/api/v1/users/@me')).json();
  const bctx = await browser.newContext({ baseURL: ORIGIN });
  const bUser = await registerUser(bctx.request, 'bob70plain');
  await loginApi(bctx.request, bUser);
  const b = bctx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    { name: 'Plain Booth' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const invite = await apiPost<{ code: string }>(a, `/channels/${channel}/invites`, {});
  await apiPost(b, `/invites/${invite.code}`, {});
  const dm = await apiPost<{ id: string }>(b, '/users/@me/channels', { recipient_id: me.id });

  for (const theme of THEMES) {
    await apiPatch(a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;

      // The thread row. A thread started from a message, whose latest message is the spoiler: loaded.
      await pause();
      const starter = await apiPost<{ id: string }>(b, `/channels/${channel}/messages`, {
        content: `Thread starter (${tag})`,
      });
      await pause();
      const name = `Spoilers ${tag}`;
      const th = await apiPost<{ id: string }>(
        b,
        `/channels/${channel}/messages/${starter.id}/threads`,
        { name },
      );
      await pause();
      await apiPost(b, `/channels/${th.id}/messages`, { content: spoiler(`loaded ${tag}`) });
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${guild.id}/${channel}`);
      const chip = page.locator('.lng-msg:not(.lng-system) .lng-thread-chip', { hasText: name });
      await expect(chip).toBeVisible();
      await expect(chip).toContainText('look');
      expect(await chip.innerText()).not.toContain(SECRET);
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `thread-loaded-${tag}.png`) });

      // Arriving live: the next message in the thread, while the channel is open.
      await pause();
      await apiPost(b, `/channels/${th.id}/messages`, { content: spoiler(`live ${tag}`) });
      await expect(chip.locator('time')).toHaveText('just now');
      await page.waitForTimeout(800);
      expect(await chip.innerText()).not.toContain(SECRET);
      await expect(chip).toContainText('2 messages');
      await page.screenshot({ path: resolve(OUT, `thread-live-${tag}.png`) });

      // The Message Requests row: loaded, then live.
      await pause();
      await apiPost(b, `/channels/${dm.id}/messages`, { content: spoiler(`req loaded ${tag}`) });
      await page.goto('/channels/@me');
      await page.getByRole('button', { name: /Message Requests/ }).click();
      const region = page.getByRole('region', { name: 'Message Requests' });
      const row = region.getByRole('listitem').filter({ hasText: bUser.username });
      const line = row.locator('[data-request-preview]');
      // A new account is filed as likely spam: the line shows after "Show message" (the rule, D-688).
      const showIt = row.getByRole('button', { name: /Show the message/ });
      await row.locator('.lng-request-preview').waitFor();
      if (await showIt.count()) await showIt.click();
      await expect(line).toContainText('look');
      expect(await line.innerText()).not.toContain(SECRET);
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `request-loaded-${tag}.png`) });
      await pause();
      await apiPost(b, `/channels/${dm.id}/messages`, { content: spoiler(`req live ${tag}`) });
      await page.waitForTimeout(1200);
      expect(await line.innerText()).not.toContain(SECRET);
      await expect(line).toContainText('look');
      await page.screenshot({ path: resolve(OUT, `request-live-${tag}.png`) });
      await c.close();
    }
  }
  await bctx.close();
  await octx.close();
});
