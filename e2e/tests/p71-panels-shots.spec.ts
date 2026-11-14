import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
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
 * SEC-AUD-262, 263, 265 (wave 71; D-739, D-740, D-741): a pin is drawn as a message, a search hit with a spoiler beside
 * the word, search on a phone, and Editorial's member list with an online, an idle and a phone-only person. Real
 * stack, several accounts. Editorial, Lounge Light and Lounge Dark; desktop 1280x800, phone 375x812 and 320x568.
 * Local only (SR_P71_PANELS=1); shots go to SR_P71_PANELS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P71_PANELS_OUT ?? resolve('test-results/p71-panels');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const DESKTOP = { width: 1280, height: 800 };
const PHONE = { width: 375, height: 812 };
const NARROW = { width: 320, height: 568 };
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(process.env.SR_P71_PANELS !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Size = { width: number; height: number };

async function context(
  browser: Browser,
  base: BrowserContext,
  size: Size,
  theme: string,
  scale = 1,
  agent: 'desktop' | 'phone' = size.width < 500 ? 'phone' : 'desktop',
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    deviceScaleFactor: scale,
    storageState: await base.storageState(),
    ...(agent === 'phone' ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100 },
  );
  return c;
}

async function member(browser: Browser, label: string, inviteCode: string) {
  const ctx = await browser.newContext({ baseURL: ORIGIN });
  const user = await registerUser(ctx.request, label);
  await loginApi(ctx.request, user);
  await apiPost(ctx.request, `/invites/${inviteCode}`, {});
  return { ctx, user };
}

const LONG = Array.from(
  { length: 16 },
  (_, i) =>
    `Line ${i + 1} of the long pin: the quick brown fox jumps over the lazy dog and keeps on going.`,
).join(' ');

test('pins drawn as messages, search with a spoiler beside the word, search on a phone', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const me = (await (await o.get('/api/v1/users/@me')).json()) as { id: string };
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Panels Booth' },
  );
  const g = guild.id;
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const channel2 = (
    await apiPost<{ id: string }>(o, `/guilds/${g}/channels`, { name: 'elsewhere', type: 0 })
  ).id;
  const invite = await apiPost<{ code: string }>(o, `/channels/${channel}/invites`, {});
  const bob = await member(browser, 'bobby', invite.code);
  const carol = await member(browser, 'carolyn', invite.code);
  const dave = await member(browser, 'davey', invite.code);

  const post = async (who: { ctx: BrowserContext }, content: string) =>
    (await apiPost<{ id: string }>(who.ctx.request, `/channels/${channel}/messages`, { content }))
      .id;
  const pin1 = await post(bob, `psst ||the ending|| <@${me.id}> **bold words** and *slanted*`);
  const pin2 = await post(
    carol,
    'Packing list:\n- tea\n- biscuits\n- a **map**\n\n```js\nconst answer = 42;\nconsole.log(answer);\n```',
  );
  const pin3 = await post(bob, LONG);
  for (const id of [pin3, pin2, pin1]) await apiPut(o, `/channels/${channel}/pins/${id}`, {});
  // The messages the search finds: a spoiler beside the word, a plain one, and a blocked person's.
  await post(bob, 'before the needlehaystack word comes ||a secret spoiler|| after it');
  await post(carol, 'the needlehaystack in carolyn words **loud** and <@' + me.id + '>');
  await post(dave, 'needlehaystack from davey the blocked one');
  await apiPut(o, `/users/@me/relationships/${dave.user.id}`, { type: 2 });
  await new Promise((r) => setTimeout(r, 2500));

  const open = async (t: string, size: Size, path = `/channels/${g}/${channel}`) => {
    await apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: t, zoom: 100, messageDisplay: 'cozy' } },
    });
    const c = await context(browser, octx, size, t);
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    await page.goto(path);
    await expect(page.getByRole('main')).toBeVisible();
    await page.waitForTimeout(900);
    return { c, page };
  };
  const shot = async (target: Locator | Page, name: string) => {
    await target.screenshot({ path: resolve(OUT, `${name}.png`) });
  };
  const text = async (l: Locator) => (await l.innerText()).replace(/\s+/g, ' ');

  for (const t of THEMES) {
    // ---- the pins panel, desktop and phone
    for (const [vp, size] of [
      ['desktop', DESKTOP],
      ['phone', PHONE],
    ] as const) {
      const tag = `${vp}-${t}`;
      const { c, page } = await open(t, size);
      await page.getByRole('button', { name: 'Pinned Messages' }).first().click();
      const pins = page.locator('.lng-pins-pop');
      await expect(pins).toBeVisible();
      await page.waitForTimeout(900);
      const body = pins.locator('.lng-pin-body:has(.sr-md-spoiler)').first();
      const before = await text(pins);
      console.log(`RECORDED pins before ${tag}:`, before.slice(0, 220));
      expect(before).not.toContain('<@');
      expect(before).not.toContain('**');
      expect(before).not.toContain('||');
      expect(before).toMatch(/@[A-Za-z]/);
      const spoiler = pins.locator('.sr-md-spoiler').first();
      await expect(spoiler).toHaveAttribute('data-revealed', 'false');
      await shot(page, `pins-before-${tag}`);
      await spoiler.click();
      await expect(spoiler).toHaveAttribute('data-revealed', 'true');
      expect(await text(body)).toContain('the ending');
      await page.waitForTimeout(200);
      await shot(page, `pins-after-${tag}`);
      // The long pin and the code block: scroll the panel to the end.
      await pins.evaluate((el) => (el.scrollTop = el.scrollHeight));
      await page.waitForTimeout(200);
      await shot(page, `pins-scrolled-${tag}`);
      await c.close();
    }

    // ---- search on desktop: the hit with a spoiler beside the word
    {
      const tag = `desktop-${t}`;
      const { c, page } = await open(t, DESKTOP);
      const box = page.getByRole('searchbox').first();
      await box.fill('needlehaystack');
      await box.press('Enter');
      const panel = page.getByRole('region', { name: 'Search results' });
      await expect(panel).toBeVisible();
      await page.waitForTimeout(1800);
      const found = await text(panel);
      console.log(`RECORDED search ${tag}:`, found.slice(0, 400));
      expect(found).not.toContain('**');
      expect(found).not.toContain(`<@${me.id}>`);
      expect(found).not.toContain('needlehaystack from davey');
      expect(found).toContain('blocked message');
      await shot(page, `search-desktop-${tag}`);
      await c.close();
    }

    // ---- search on a phone
    for (const [vp, size] of [
      ['phone', PHONE],
      ['narrow', NARROW],
    ] as const) {
      const tag = `${vp}-${t}`;
      const { c, page } = await open(t, size);
      const url = page.url();
      const button = page.getByRole('button', { name: 'Search this conversation' });
      await expect(button).toBeVisible();
      // No inline search box on a phone: the button is the way in.
      expect(await page.locator('.lng-header .lng-search').first().isVisible()).toBe(false);
      // The touch target is 44px (a pseudo-element around the 28px button).
      const target = await button.evaluate((el) => {
        const c = getComputedStyle(el, '::after');
        return { w: parseFloat(c.width), h: parseFloat(c.height) };
      });
      console.log(`RECORDED search button target ${tag}:`, JSON.stringify(target));
      expect(target.w).toBeGreaterThanOrEqual(44);
      expect(target.h).toBeGreaterThanOrEqual(44);
      await shot(page, `phone-header-${tag}`);

      await button.click();
      const dialog = page.getByRole('dialog', { name: 'Search' });
      await expect(dialog).toBeVisible();
      const input = dialog.getByRole('searchbox');
      await expect(input).toBeFocused();
      await page.waitForTimeout(300);
      await shot(page, `phone-sheet-empty-${tag}`);

      await input.fill('needlehaystack');
      await input.press('Enter');
      await expect(dialog.getByText(/result/i).first()).toBeVisible();
      await page.waitForTimeout(1800);
      const found = await text(dialog);
      console.log(`RECORDED phone search ${tag}:`, found.slice(0, 300));
      expect(found).toContain('blocked message');
      expect(found).not.toContain('needlehaystack from davey');
      await shot(page, `phone-sheet-results-${tag}`);
      const sheetBox = (await dialog.boundingBox())!;
      console.log(`RECORDED sheet box ${tag}:`, JSON.stringify(sheetBox));
      // Fills the chat pane, not the app: the tab bar stays below it.
      expect(sheetBox.y + sheetBox.height).toBeLessThanOrEqual(size.height - 40);

      if (vp === 'phone') {
        // A filter chip.
        await dialog.getByRole('button', { name: 'has:link' }).click();
        await expect(input).toHaveValue(/has:link/);
        await page.waitForTimeout(300);
        await shot(page, `phone-sheet-chip-${tag}`);
        // The results scroll inside the sheet and the filters stay reachable.
        await input.fill('needlehaystack');
        await input.press('Enter');
        await page.waitForTimeout(1500);

        // Escape closes it, and focus returns to the button.
        await page.keyboard.press('Escape');
        await expect(dialog).toBeHidden();
        await expect(button).toBeFocused();
        await page.waitForTimeout(400);
        expect(page.url()).toBe(url);

        // The back gesture closes it and stays in the channel.
        await button.click();
        await expect(dialog).toBeVisible();
        await page.goBack();
        await expect(dialog).toBeHidden();
        await page.waitForTimeout(500);
        expect(page.url()).toBe(url);
        await expect(page.getByRole('main')).toBeVisible();

        // Its close button.
        await button.click();
        await dialog.getByRole('button', { name: 'Close search' }).click();
        await expect(dialog).toBeHidden();
        expect(page.url()).toBe(url);

        // Choose a hit: the sheet closes and the message is shown.
        await button.click();
        await dialog.getByRole('searchbox').fill('needlehaystack');
        await dialog.getByRole('searchbox').press('Enter');
        await page.waitForTimeout(1800);
        await dialog.locator('.lng-search-hit').filter({ hasText: 'carolyn' }).first().click();
        await expect(dialog).toBeHidden();
        await page.waitForTimeout(1200);
        expect(page.url()).toMatch(/\/channels\/[^/]+\/[^/]+\/\d+/);
        await shot(page, `phone-after-jump-${tag}`);

        // The panes still switch by swipe after the sheet is gone.
        const root = page.locator('.lng-root');
        await expect(root).toHaveAttribute('data-pane', 'chat');
        const cdp = await page.context().newCDPSession(page);
        const touch = (type: 'touchStart' | 'touchMove' | 'touchEnd', x: number) =>
          cdp.send('Input.dispatchTouchEvent', {
            type,
            touchPoints: type === 'touchEnd' ? [] : [{ x, y: 400 }],
          });
        await touch('touchStart', 40);
        for (const x of [90, 150, 210, 270, 330]) await touch('touchMove', x);
        await touch('touchEnd', 330);
        await cdp.detach();
        await expect(root).toHaveAttribute('data-pane', 'list');
        await page.waitForTimeout(400);
        if (t === 'lounge-light') await shot(page, `phone-swiped-to-list-${tag}`);
      }
      await c.close();
    }
  }
  // A direct message on a phone: the header keeps back, the name and the call buttons; the search button sits with them.
  await apiPut(o, `/users/@me/relationships/${bob.user.id}`, { type: 1 });
  await apiPut(bob.ctx.request, `/users/@me/relationships/${me.id}`, { type: 1 });
  const dm = await apiPost<{ id: string }>(o, '/users/@me/channels', { recipient_id: bob.user.id });
  await apiPost(bob.ctx.request, `/channels/${dm.id}/messages`, {
    content: 'a needlehaystack in a direct message',
  });
  for (const t of THEMES) {
    for (const [vp, size] of [
      ['phone', PHONE],
      ['narrow', NARROW],
    ] as const) {
      const { c, page } = await open(t, size, `/channels/@me/${dm.id}`);
      await shot(page, `dm-header-${vp}-${t}`);
      await page.getByRole('button', { name: 'Search this conversation' }).click();
      const dialog = page.getByRole('dialog', { name: 'Search' });
      await dialog.getByRole('searchbox').fill('needlehaystack');
      await dialog.getByRole('searchbox').press('Enter');
      await page.waitForTimeout(1800);
      expect(await text(dialog)).toContain('direct message');
      await shot(page, `dm-sheet-${vp}-${t}`);
      await c.close();
    }
  }
  await octx.close();
  for (const p of [bob, carol, dave]) await p.ctx.close();
});

test('Editorial: the desktop member list with an online, an idle and a phone-only member', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Status Booth' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const invite = await apiPost<{ code: string }>(o, `/channels/${channel}/invites`, {});
  const phoner = await member(browser, 'phonerz', invite.code);
  const deskman = await member(browser, 'deskmanz', invite.code);
  const idler = await member(browser, 'idlerz', invite.code);

  const sessions: BrowserContext[] = [];
  const signIn = async (
    who: { ctx: BrowserContext },
    agent: 'desktop' | 'phone',
    afk: boolean,
  ): Promise<void> => {
    const c = await context(
      browser,
      who.ctx,
      agent === 'phone' ? PHONE : DESKTOP,
      'lounge-light',
      1,
      agent,
    );
    const page = await c.newPage();
    if (afk) {
      // The idle reporter fires after ten minutes: send the same report now, over the page's own socket.
      await page.routeWebSocket(/gateway/, (ws) => {
        const server = ws.connectToServer();
        server.onMessage((m) => {
          ws.send(m);
          const raw = typeof m === 'string' ? m : m.toString('utf8');
          if (/"t"\s*:\s*"READY"/.test(raw)) {
            server.send(
              JSON.stringify({
                op: 3,
                d: { status: 'online', afk: true, since: 1, activities: [] },
              }),
            );
          }
        });
        ws.onMessage((m) => server.send(m));
      });
    }
    await page.goto('/channels/@me');
    await expect(page.getByRole('main')).toBeVisible();
    sessions.push(c);
  };
  await signIn(phoner, 'phone', false);
  await signIn(deskman, 'desktop', false);
  await signIn(idler, 'desktop', true);

  for (const t of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme: t, zoom: 100 } } });
    for (const scale of [1, 2] as const) {
      const c = await context(browser, octx, DESKTOP, t, scale);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${guild.id}/${channel}`);
      await expect(page.getByRole('main')).toBeVisible();
      const list = page.getByRole('list', { name: 'Members' });
      const row = (re: RegExp) => list.getByRole('button', { name: re });
      await expect(
        row(/phonerz/).locator('[role="img"][aria-label="Online on a phone"]'),
      ).toBeVisible();
      await expect(row(/deskmanz/).locator('[role="img"][aria-label="Online"]')).toBeVisible();
      await expect(row(/idlerz/).locator('[role="img"][aria-label="Idle"]')).toBeVisible({
        timeout: 15_000,
      });
      await page.waitForTimeout(600);
      if (scale === 1) {
        await page.screenshot({ path: resolve(OUT, `memberlist-desktop-${t}.png`) });
      } else {
        for (const who of ['phonerz', 'deskmanz', 'idlerz']) {
          await row(new RegExp(who)).screenshot({ path: resolve(OUT, `status-${who}-${t}.png`) });
        }
      }
      await c.close();
    }
  }
  for (const c of sessions) await c.close();
  await octx.close();
  for (const p of [phoner, deskman, idler]) await p.ctx.close();
});
