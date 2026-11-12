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
 * SEC-AUD-154 (the Friends page fits a 375 px phone, its row menu can be used), SEC-AUD-155 (D-640: the Block
 * dialog says what a block does in servers and group DMs) and SEC-AUD-156 (the other Lounge home pages at 375 px).
 * Editorial, Lounge Light and Lounge Dark, desktop and phone. Local only (SR_P63_PHONE=1); shots go to
 * SR_P63_PHONE_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P63_PHONE_OUT ?? resolve('test-results/p63-phone');
// Registration allows 20 accounts per address per day: a re-run reuses the accounts of the first one.
const SEEDED = process.env.SR_P63_PHONE_SEED ?? resolve('test-results/p63-phone-users.json');
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

/**
 * The page's real overflow. The shell lays its sidebar and main pane side by side on a phone and slides between
 * them, so the document's own width (and the body's) says nothing. What counts: the main pane is no wider than the
 * window, and nothing in it sticks out past the pane's right edge unless an ancestor clips or scrolls it (the tab
 * row scrolls by design, long names clip by design).
 */
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

/**
 * The phone opens on the sidebar (its home screen). Tapping Friends or Message Requests there does not reveal the
 * page (the shell only switches to the chat pane for a channel: see the handoff); a swipe does, as the shell's own
 * touch handler reads it.
 */
async function swipeToMain(page: Page, vp: string) {
  if (vp !== 'phone') return;
  await page.evaluate(() => {
    const el = document.querySelector('.lng-body')!;
    const fire = (type: string, x: number) => {
      const t = new Touch({ identifier: 1, target: el, clientX: x, clientY: 300 });
      el.dispatchEvent(
        new TouchEvent(type, {
          bubbles: true,
          cancelable: true,
          touches: type === 'touchend' ? [] : [t],
          changedTouches: [t],
        }),
      );
    };
    fire('touchstart', 300);
    fire('touchend', 100);
  });
  await page.waitForTimeout(400);
}

async function openFriends(page: Page, vp: string) {
  await page.goto('/channels/@me');
  if (vp === 'phone') {
    await page
      .getByRole('link', { name: /^Friends/ })
      .first()
      .click();
    await swipeToMain(page, vp);
  }
}

const scrollWidth = (page: Page) =>
  page.evaluate(() => {
    const bad: string[] = [];
    const root = document.querySelector<HTMLElement>('.lng-main') ?? document.body;
    const rr = root.getBoundingClientRect();
    let widest = Math.max(rr.width, document.documentElement.scrollWidth);
    if (rr.width > window.innerWidth + 1) bad.push(`main pane is ${Math.round(rr.width)} wide`);
    if (root.scrollWidth > root.clientWidth + 1)
      bad.push(`main scrolls ${root.scrollWidth}>${root.clientWidth}`);
    for (const el of Array.from(root.querySelectorAll<HTMLElement>('*'))) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.right <= rr.right + 1) continue;
      let clipped = false;
      for (let a: HTMLElement | null = el.parentElement; a && a !== root; a = a.parentElement) {
        if (getComputedStyle(a).overflowX !== 'visible') clipped = true;
      }
      if (clipped || getComputedStyle(el).position === 'fixed') continue;
      bad.push(
        `${el.tagName}.${String(el.className).slice(0, 30)}@${Math.round(r.right - rr.left)}`,
      );
      widest = Math.max(widest, r.right - rr.left);
    }
    return { widest, bad: bad.slice(0, 8) };
  });

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
    const u = await seeded(browser, label);
    const uctx = await browser.newContext({ baseURL: ORIGIN });
    await loginApi(uctx.request, u);
    await apiPut(o, `/users/@me/relationships/${u.id}`, { type: 1 }).catch(() => undefined);
    await apiPut(uctx.request, `/users/@me/relationships/${ownerId}`, { type: 1 }).catch(
      () => undefined,
    );
    await uctx.close();
    friends.push(u);
  }
  const [amy, ben] = friends as [(typeof friends)[number], (typeof friends)[number]];
  const dm = await apiPost<{ id: string }>(o, '/users/@me/channels', { recipient_id: amy.id });

  // dan asks to be a friend; a new stranger writes first and the owner's spam filter is on 'all' (D-230): a Message Request. Both share a server with the owner.
  try {
    const guild = await apiPost<{ channels: { id: string; type: number }[] }>(o, '/guilds', {
      name: 'Phone Booth',
    });
    const invite = await apiPost<{ code: string }>(
      o,
      `/channels/${guild.channels.find((c) => c.type === 0)!.id}/invites`,
      {},
    );
    await apiPatch(o, '/users/@me/privacy', { dm_spam_filter: 'all' });
    for (const [label, friendRequest] of [
      ['dan', true],
      [`g${Date.now().toString(36).slice(-5)}`, false],
    ] as const) {
      const u = await seeded(browser, label);
      const sctx = await browser.newContext({ baseURL: ORIGIN });
      await loginApi(sctx.request, u);
      await apiPost(sctx.request, `/invites/${invite.code}`);
      if (friendRequest) {
        await apiPut(sctx.request, `/users/@me/relationships/${ownerId}`, { type: 1 }).catch(
          () => undefined,
        );
      } else {
        await apiPost(sctx.request, '/users/@me/channels', { recipient_id: ownerId });
      }
      await sctx.close();
    }
    await apiPatch(o, '/users/@me/privacy', { dm_spam_filter: 'non_friends' });
  } catch (e) {
    console.log('stranger seed incomplete:', String(e).slice(0, 200));
  }

  const dms = (await (await o.get('/api/v1/users/@me/channels')).json()) as {
    id: string;
    is_message_request?: boolean;
  }[];
  const requestDm = dms.find((d) => d.is_message_request)?.id;

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
        const { widest, bad } = await scrollWidth(page);
        console.log(`scrollWidth ${name} ${tag}: ${widest} ${bad.join(' | ')}`);
        expect
          .soft(widest, `${name} at ${tag}: ${bad.join(' | ')}`)
          .toBeLessThanOrEqual(size.width);
      };

      // 1. Friends: All, Pending, Add Friend; the row menu inside the screen.
      await openFriends(page, vp);
      await expect(page.getByRole('tab', { name: 'All' })).toBeVisible();
      await fits('friends-home');
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
      await swipeToMain(page, vp);
      await shot('message-requests');
      await fits('message-requests');
      // One request, opened by link (O1): the request view.
      if (requestDm) {
        await page.goto(`/channels/@me/${requestDm}`);
        await expect(page.getByRole('button', { name: /^Accept/ }).first()).toBeVisible();
        await shot('message-request-one');
        await fits('message-request-one');
      }

      // The inbox lives in a channel's header (and the desktop title strip): open it from the DM.
      await page.goto(`/channels/@me/${dm.id}`);
      await page.getByRole('button', { name: 'Inbox', exact: true }).first().click();
      await page.getByRole('tab', { name: 'For You' }).click();
      const inbox = page.locator('.lng-inbox');
      await expect(inbox).toBeVisible();
      const ibox = (await inbox.boundingBox())!;
      expect.soft(ibox.x, `inbox left at ${tag}`).toBeGreaterThanOrEqual(0);
      expect.soft(ibox.x + ibox.width, `inbox right at ${tag}`).toBeLessThanOrEqual(size.width);
      expect.soft(ibox.y + ibox.height, `inbox bottom at ${tag}`).toBeLessThanOrEqual(size.height);
      await shot('inbox-for-you');
      await c.close();
    }
  }

  // Remove Friend, for real, on a phone: the row is gone.
  const c = await context(browser, octx, VIEWPORTS.phone, 'lounge-editorial');
  const page = await c.newPage();
  page.setDefaultTimeout(20_000);
  await openFriends(page, 'phone');
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
