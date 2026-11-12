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
 * SEC-AUD-154 (the Friends page fits a 375 px phone, its row menu can be used), SEC-AUD-155 (D-640: the Block
 * dialog says what a block does in servers and group DMs) and SEC-AUD-156 (the other Lounge home pages at 375 px).
 * Editorial, Lounge Light and Lounge Dark, desktop and phone. Local only (SR_P63_PHONE=1); shots go to
 * SR_P63_PHONE_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P63_PHONE_OUT ?? resolve('test-results/p63-phone');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P63_PHONE !== '1', 'local visual check only');
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

const scrollWidth = (page: Page) => page.evaluate(() => document.documentElement.scrollWidth);

test('the Friends page and its row menu on a phone, the Block text, the other Home pages', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const ownerId = ((await (await o.get('/api/v1/users/@me')).json()) as { id: string }).id;

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

  // A stranger who writes first: a Message Request, and a pending friend request.
  const sctx0 = await browser.newContext({ baseURL: ORIGIN });
  const dan = await registerUser(sctx0.request, 'dan');
  await sctx0.close();
  const sctx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(sctx.request, dan);
  try {
    await apiPut(sctx.request, `/users/@me/relationships/${ownerId}`, { type: 1 });
    await apiPost(sctx.request, '/users/@me/channels', { recipient_id: ownerId });
  } catch (e) {
    console.log('stranger seed incomplete:', String(e).slice(0, 200));
  }
  await sctx.close();

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
      const fits = async (name: string) => {
        const w = await scrollWidth(page);
        console.log(`scrollWidth ${name} ${tag}: ${w}`);
        expect.soft(w, `${name} at ${tag}`).toBeLessThanOrEqual(size.width);
      };

      // 1. Friends: All, Pending, Add Friend; the row menu inside the screen.
      await page.goto('/channels/@me');
      await page.getByRole('tab', { name: 'All' }).click();
      await expect(page.getByRole('list', { name: 'All Friends' })).toBeVisible();
      await shot('friends-all');
      await fits('friends-all');
      const more = page.getByRole('button', {
        name: new RegExp(`More actions for .*${amy.username}`, 'i'),
      });
      await more.click();
      const remove = page.getByRole('menuitem', { name: 'Remove Friend' });
      const block = page.getByRole('menuitem', { name: 'Block' });
      await expect(remove).toBeVisible();
      await shot('friends-row-menu');
      for (const item of [remove, block]) {
        const box = (await item.boundingBox())!;
        expect.soft(box.x, `menu item left at ${tag}`).toBeGreaterThanOrEqual(0);
        expect.soft(box.x + box.width, `menu item right at ${tag}`).toBeLessThanOrEqual(size.width);
        expect.soft(box.y, `menu item top at ${tag}`).toBeGreaterThanOrEqual(0);
        expect
          .soft(box.y + box.height, `menu item bottom at ${tag}`)
          .toBeLessThanOrEqual(size.height);
      }
      await block.click();
      const bl = page.getByRole('dialog', { name: /^Block .*\?$/ });
      await expect(bl).toBeVisible();
      await expect(bl).toContainText('In servers and group DMs they can still write');
      await shot('confirm-block-friends');
      const dbox = (await bl.boundingBox())!;
      expect.soft(dbox.x).toBeGreaterThanOrEqual(0);
      expect.soft(dbox.x + dbox.width).toBeLessThanOrEqual(size.width);
      expect.soft(dbox.y + dbox.height).toBeLessThanOrEqual(size.height);
      await bl.getByRole('button', { name: 'Cancel' }).click();
      await expect(bl).toHaveCount(0);
      await page.getByRole('tab', { name: /^Pending/ }).click();
      await shot('friends-pending');
      await fits('friends-pending');
      await page.getByRole('tab', { name: 'Add Friend' }).click();
      await shot('friends-add');
      await fits('friends-add');

      // 2. The other Home pages (item 3).
      await page.goto(`/channels/@me/${dm.id}`);
      await expect(page.getByRole('button', { name: 'Remove Friend' }).first()).toBeVisible();
      await shot('dm-welcome');
      await fits('dm-welcome');
      await page.getByRole('button', { name: 'Block', exact: true }).first().click();
      const dmb = page.getByRole('dialog', { name: /^Block .*\?$/ });
      await expect(dmb).toBeVisible();
      await shot('confirm-block-dm');
      await dmb.getByRole('button', { name: 'Cancel' }).click();

      await page.goto('/channels/@me');
      await page
        .getByRole('button', { name: /Message Requests/ })
        .first()
        .click();
      await shot('message-requests');
      await fits('message-requests');
      const req = page.getByRole('list', { name: 'Message requests' }).locator('li').first();
      if (await req.count()) {
        await req.click().catch(() => undefined);
        await shot('message-request-one');
        await fits('message-request-one');
      }

      await page.goto('/channels/@me');
      await page
        .getByRole('button', { name: /^Inbox/ })
        .first()
        .click();
      await page
        .getByRole('tab', { name: 'For You' })
        .click()
        .catch(() => undefined);
      await shot('inbox-for-you');
      await fits('inbox-for-you');
      await c.close();
    }
  }

  // Remove Friend, for real, on a phone: the row is gone.
  const c = await context(browser, octx, VIEWPORTS.phone, 'lounge-editorial');
  const page = await c.newPage();
  page.setDefaultTimeout(20_000);
  await page.goto('/channels/@me');
  await page.getByRole('tab', { name: 'All' }).click();
  const moreBen = page.getByRole('button', {
    name: new RegExp(`More actions for .*${ben.username}`, 'i'),
  });
  await moreBen.click();
  await page.getByRole('menuitem', { name: 'Remove Friend' }).click();
  await page
    .getByRole('dialog', { name: /^Remove .* as a friend\?$/ })
    .getByRole('button', { name: 'Remove Friend' })
    .click();
  await expect(moreBen).toHaveCount(0);
  await c.close();
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
});
