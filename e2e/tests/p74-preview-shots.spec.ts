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
 * SEC-AUD-292 (wave 74; D-782), on the real stack, two accounts: the latest message of a thread and of a Message
 * Request is three headings, or `intro` + a heading + subtext. The thread row and the request row show the words and
 * no `#`, loaded from the server and arriving live. Editorial, Lounge Light and Lounge Dark, desktop and phone.
 * Local only (SR_P74_PREVIEW=1); shots go to SR_P74_PREVIEW_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P74_PREVIEW_OUT ?? resolve('test-results/p74-preview');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(process.env.SR_P74_PREVIEW !== '1', 'local visual check only');
test.setTimeout(1_800_000);

const HEADINGS = '# Alpha heading\n## Beta heading\n### Gamma heading';
const MIXED = 'intro words\n# Title words\n-# small print words';

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
) {
  const phone = size.width < 500;
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    ...(phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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

test('a thread row and a request row hold the words, not the heading markers', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const a = octx.request;
  const me = await (await a.get('/api/v1/users/@me')).json();
  const bctx = await browser.newContext({ baseURL: ORIGIN });
  const bUser = await registerUser(bctx.request, 'bob74prev');
  await loginApi(bctx.request, bUser);
  const b = bctx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    { name: 'Marker Booth' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const invite = await apiPost<{ code: string }>(a, `/channels/${channel}/invites`, {});
  await apiPost(b, `/invites/${invite.code}`, {});
  const dm = await apiPost<{ id: string }>(b, '/users/@me/channels', { recipient_id: me.id });

  for (const theme of THEMES) {
    await apiPatch(a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      // Two threads under their starters: the latest message of one is three headings, of the other a mix.
      const names: string[] = [];
      for (const [kind, text] of [
        ['headings', HEADINGS],
        ['mixed', MIXED],
      ] as const) {
        await pause();
        const starter = await apiPost<{ id: string }>(b, `/channels/${channel}/messages`, {
          content: `Starter ${kind} ${tag}`,
        });
        await pause();
        const name = `Markers ${kind} ${tag}`;
        names.push(name);
        const th = await apiPost<{ id: string }>(
          b,
          `/channels/${channel}/messages/${starter.id}/threads`,
          { name },
        );
        await pause();
        await apiPost(b, `/channels/${th.id}/messages`, { content: text });
      }
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${guild.id}/${channel}`);
      const chips = names.map((n) =>
        page.locator('.lng-msg:not(.lng-system) .lng-thread-chip', { hasText: n }),
      );
      for (const chip of chips) await expect(chip).toBeVisible();
      await expect(chips[0]).toContainText('Alpha heading Beta heading Gamma heading');
      await expect(chips[1]).toContainText('intro words Title words small print words');
      for (const chip of chips) expect(await chip.innerText()).not.toContain('#');
      await chips[1].scrollIntoViewIfNeeded();
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `thread-loaded-${tag}.png`) });
      // Live: the next message in the second thread, while the channel is open.
      await pause();
      const threads = (await (
        await b.get(`/api/v1/channels/${channel}/messages?limit=10`)
      ).json()) as { id: string; thread?: { id: string; name: string } }[];
      const live = threads.find((m) => m.thread?.name === names[0])!.thread!.id;
      await apiPost(b, `/channels/${live}/messages`, { content: `${MIXED}\n### live heading` });
      await page.waitForTimeout(1500);
      await chips[0].scrollIntoViewIfNeeded();
      expect(await chips[0].innerText()).not.toContain('#');
      await page.screenshot({ path: resolve(OUT, `thread-live-${tag}.png`) });

      // The Message Requests row: loaded, then live.
      await pause();
      await apiPost(b, `/channels/${dm.id}/messages`, { content: HEADINGS });
      await page.goto('/channels/@me');
      await page.getByRole('button', { name: /Message Requests/ }).click();
      const region = page.getByRole('region', { name: 'Message Requests' });
      const row = region.getByRole('listitem').filter({ hasText: bUser.username });
      const line = row.locator('[data-request-preview]');
      const showIt = row.getByRole('button', { name: /Show the message/ });
      await row.locator('.lng-request-preview').waitFor();
      if (await showIt.count()) await showIt.click();
      await expect(line).toContainText('Alpha heading Beta heading Gamma heading');
      expect(await line.innerText()).not.toContain('#');
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `request-loaded-${tag}.png`) });
      await pause();
      await apiPost(b, `/channels/${dm.id}/messages`, { content: MIXED });
      await page.waitForTimeout(1200);
      await expect(line).toContainText('intro words Title words small print words');
      expect(await line.innerText()).not.toContain('#');
      await page.screenshot({ path: resolve(OUT, `request-live-${tag}.png`) });
      await c.close();
    }
  }
  await bctx.close();
  await octx.close();
});
