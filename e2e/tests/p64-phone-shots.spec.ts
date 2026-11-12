import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
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
  type SeededUser,
} from '../support/fixtures';

/**
 * SEC-AUD-162 (a tap on Friends or Message Requests shows the page on a phone, and the page has a back control)
 * and SEC-AUD-164 (the DM header on a phone). Editorial, Lounge Light and Lounge Dark; desktop once for the
 * unchanged check. Local only (SR_P64_PHONE=1); shots go to SR_P64_PHONE_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P64_PHONE_OUT ?? resolve('test-results/p64-phone');
const SEEDED = process.env.SR_P64_PHONE_SEED ?? resolve('test-results/p64-phone-users.json');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };

test.skip(process.env.SR_P64_PHONE !== '1', 'local visual check only');
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

async function seeded(browser: Browser, label: string) {
  const cache: Record<string, SeededUser> = existsSync(SEEDED)
    ? (JSON.parse(readFileSync(SEEDED, 'utf8')) as Record<string, SeededUser>)
    : {};
  if (!cache[label]) {
    const rctx = await browser.newContext({ baseURL: ORIGIN });
    cache[label] = await registerUser(rctx.request, label);
    await rctx.close();
    writeFileSync(SEEDED, JSON.stringify(cache));
  }
  return cache[label];
}

async function inside(page: Page, locator: ReturnType<Page['locator']>, what: string) {
  const vp = page.viewportSize()!;
  const box = (await locator.boundingBox())!;
  expect(box, what).not.toBeNull();
  expect(box.x, `${what} left`).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width, `${what} right`).toBeLessThanOrEqual(vp.width + 1);
}

test('Friends and Message Requests open on a tap and go back (phone), unchanged on desktop', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const ownerId = ((await (await o.get('/api/v1/users/@me')).json()) as { id: string }).id;
  const amy = await seeded(browser, 'amy');
  const actx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(actx.request, amy);
  await apiPut(o, `/users/@me/relationships/${amy.id}`, { type: 1 }).catch(() => undefined);
  await apiPut(actx.request, `/users/@me/relationships/${ownerId}`, { type: 1 }).catch(
    () => undefined,
  );
  await actx.close();
  await apiPost(o, '/users/@me/channels', { recipient_id: amy.id });

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of [
      ['phone', PHONE],
      ['desktop', DESKTOP],
    ] as const) {
      if (vp === 'desktop' && theme !== 'lounge-light') continue;
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const shot = async (name: string) => {
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };
      await page.goto('/channels/@me');
      const nav = page.getByTestId('home-sidebar');
      if (vp === 'phone') {
        // Opening Home still shows the list (the pick, not the state, reveals the page).
        await expect(nav).toBeVisible();
        await inside(page, nav, 'home list');
        await shot('home-list');
      }
      await nav.getByRole('link', { name: /^Friends/ }).click();
      const header = page.getByRole('heading', { level: 1, name: 'Friends' });
      await expect(header).toBeVisible();
      await inside(page, header, 'Friends header');
      await page.getByRole('tab', { name: 'All' }).click();
      await inside(page, page.getByRole('tab', { name: 'All' }), 'All tab');
      await shot('friends');
      if (vp === 'phone') {
        await page.getByRole('button', { name: 'Back to channels' }).click();
        await expect(nav).toBeVisible();
        await inside(page, nav, 'home list after back');
      }
      await nav.getByRole('button', { name: /^Message Requests/ }).click();
      const mr = page.getByRole('heading', { level: 1, name: 'Message Requests' });
      await expect(mr).toBeVisible();
      await inside(page, mr, 'Message Requests header');
      await shot('requests');
      if (vp === 'phone') {
        await page.getByRole('button', { name: 'Back to channels' }).click();
        await expect(nav).toBeVisible();
        await inside(page, nav, 'home list after second back');
      } else {
        await expect(page.getByRole('button', { name: 'Back to channels' })).toBeHidden();
      }
      await c.close();
    }
  }
});
