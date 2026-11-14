import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * Wave 71 (commander): a text spoiler in the message list is hidden until it is clicked. markdown-react's own
 * stylesheet is not loaded by the app, so before this the spoiler words were readable in the list. Editorial,
 * Lounge Light and Lounge Dark, desktop and phone. Local only (SR_P71_SPOILER=1); shots go to SR_P71_SPOILER_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P71_SPOILER_OUT ?? resolve('test-results/p71-spoiler');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P71_SPOILER !== '1', 'local visual check only');
test.setTimeout(900_000);

test('a spoiler in the message list is hidden until clicked', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, bootstrapState().owner);
  const a = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    { name: 'Spoiler Booth' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  await apiPost(a, `/channels/${channel}/messages`, {
    content: 'The ending: ||the butler did it|| and that is all.',
  });
  for (const theme of THEMES) {
    await apiPatch(a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await browser.newContext({
        baseURL: ORIGIN,
        viewport: size,
        storageState: await octx.storageState(),
      });
      await c.addInitScript(
        (p) => {
          localStorage.setItem(
            'sr.user.prefs',
            JSON.stringify({ state: { prefs: p }, version: 2 }),
          );
        },
        { theme, zoom: 100 },
      );
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${guild.id}/${channel}`);
      const sp = page.locator('.sr-md-spoiler').first();
      await expect(sp).toBeVisible();
      await expect(sp).toHaveAttribute('data-revealed', 'false');
      const hidden = await sp
        .locator('.sr-md-spoiler-content')
        .evaluate((e) => getComputedStyle(e).opacity);
      expect(hidden).toBe('0');
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `hidden-${tag}.png`) });
      await sp.click();
      await expect(sp).toHaveAttribute('data-revealed', 'true');
      await page.waitForTimeout(300);
      await page.screenshot({ path: resolve(OUT, `revealed-${tag}.png`) });
      await c.close();
    }
  }
});
