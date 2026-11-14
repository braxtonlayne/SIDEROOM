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
  type SeededUser,
} from '../support/fixtures';

/**
 * SEC-AUD-206, 221, 222 (wave 68; D-702, D-703, D-704): the phone glyph, reaction notifications and the digest
 * switch leaving Settings. Real stack: a second and third account sign in with a phone user agent (the web app
 * then announces `os: android` at IDENTIFY), one with a desktop user agent; the Owner's member list, profile
 * popout, friends list and DM list show the glyph for the phone-only person, the dot for the desktop one, and the
 * dot again when the phone person opens a desktop tab too. The idle-on-phone person reports afk through a message
 * the test sends on its gateway socket. Reaction notifications count `Notification` constructions in the page.
 * Editorial, Lounge Light and Lounge Dark, desktop and phone. Local only (SR_P68_PRESENCE=1); shots go to
 * SR_P68_PRESENCE_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P68_PRESENCE_OUT ?? resolve('test-results/p68-presence');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(process.env.SR_P68_PRESENCE !== '1', 'local visual check only');
test.setTimeout(1_800_000);

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
  extra: Record<string, unknown> = {},
  ctxOptions: Record<string, unknown> = {},
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    ...ctxOptions,
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100, ...extra },
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

/** Sign `who` in on a phone: a phone user agent, so the app announces itself as `android`. */
async function onPhone(
  browser: Browser,
  who: { ctx: BrowserContext },
  afk = false,
): Promise<{ ctx: BrowserContext; page: Page }> {
  const ctx = await context(browser, who.ctx, VIEWPORTS.phone, 'lounge-light', {}, {
    userAgent: PHONE_UA,
    isMobile: true,
    hasTouch: true,
  });
  const page = await ctx.newPage();
  if (afk) {
    // The idle reporter fires after ten minutes of no input: send the same report now, over the page's own socket.
    await page.routeWebSocket(/gateway/, (ws) => {
      const server = ws.connectToServer();
      server.onMessage((m) => {
        ws.send(m);
        const text = typeof m === 'string' ? m : m.toString('utf8');
        if (/"t"\s*:\s*"READY"/.test(text)) {
          server.send(
            JSON.stringify({ op: 3, d: { status: 'online', afk: true, since: 1, activities: [] } }),
          );
        }
      });
      ws.onMessage((m) => server.send(m));
    });
  }
  await page.goto('/channels/@me');
  await expect(page.getByRole('main')).toBeVisible();
  return { ctx, page };
}

test('the phone glyph appears for a phone-only person and goes back to the dot', async ({
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
    { name: 'Glyph Booth' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const invite = await apiPost<{ code: string }>(o, `/channels/${channel}/invites`, {});

  const people: Record<string, { ctx: BrowserContext; user: SeededUser }> = {};
  for (const label of ['phoner', 'deskman', 'napper']) {
    people[label] = await member(browser, label, invite.code);
    const u = people[label]!;
    // Friends of the Owner (the friends list) with a DM each (the DM list).
    await apiPut(o, `/users/@me/relationships/${u.user.id}`, { type: 1 });
    await apiPut(u.ctx.request, `/users/@me/relationships/${ownerId}`, { type: 1 });
    const dm = await apiPost<{ id: string }>(o, '/users/@me/channels', { recipient_id: u.user.id });
    await apiPost(o, `/channels/${dm.id}/messages`, { content: 'Hello there' });
  }

  // The Owner's own page (desktop user agent): the observer.
  const watch = async (theme: string, size: { width: number; height: number }) => {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    const c = await context(browser, octx, size, theme);
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    return { c, page };
  };

  const phoner = await onPhone(browser, people.phoner!);
  const desk = await signInDesktop(browser, people.deskman!);
  const napper = await onPhone(browser, people.napper!, true);

  const { c: ownerCtx, page } = await watch('lounge-light', VIEWPORTS.desktop);
  await page.goto(`/channels/${guild.id}/${channel}`);
  const rowOn = (pg: Page) => (re: RegExp) =>
    pg.getByRole('list', { name: 'Members' }).getByRole('button', { name: re });
  const row = rowOn(page);
  // The real stack proves the whole path: phone user agent -> IDENTIFY properties -> client_status -> glyph.
  await expect(row(/phoner/).locator('[role="img"][aria-label="Online on a phone"]')).toBeVisible();
  await expect(row(/deskman/).locator('[role="img"][aria-label="Online"]')).toBeVisible();
  await expect(row(/napper/).locator('[role="img"][aria-label="Idle"]')).toBeVisible();

  // A desktop-user-agent tab of the phone person turns the glyph back into the dot.
  const phonerDesk = await signInDesktop(browser, people.phoner!);
  await expect(row(/phoner/).locator('[role="img"][aria-label="Online"]')).toBeVisible();
  await phonerDesk.close();
  await expect(row(/phoner/).locator('[role="img"][aria-label="Online on a phone"]')).toBeVisible();
  await ownerCtx.close();

  for (const theme of THEMES) {
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const { c, page } = await watch(theme, size);
      const row = rowOn(page);
      // Member list.
      await page.goto(`/channels/${guild.id}/${channel}`);
      if (vp === 'phone') {
        await swipeToMembers(page);
      }
      await expect(row(/phoner/).locator('[role="img"][aria-label="Online on a phone"]')).toBeVisible();
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `memberlist-${tag}.png`) });
      await row(/phoner/).evaluate((el) => (el as HTMLElement).scrollIntoView());
      await page.locator('.lng-member').first().focus().catch(() => undefined);
      await row(/phoner/).screenshot({ path: resolve(OUT, `glyph-closeup-${tag}.png`) });
      // Profile popout.
      await row(/phoner/).dispatchEvent('click');
      await expect(page.getByRole('dialog').first()).toBeVisible();
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `popout-${tag}.png`) });
      await page.keyboard.press('Escape');
      // Friends list and DM list.
      await page.goto('/channels/@me');
      await expect(page.getByRole('link', { name: /phoner/ }).first()).toBeVisible();
      await page.waitForTimeout(600);
      await page.screenshot({ path: resolve(OUT, `friends-dmlist-${tag}.png`) });
      if (vp === 'phone') {
        await page.getByRole('button', { name: /Friends/ }).first().click().catch(() => undefined);
      }
      await page.waitForTimeout(300);
      await page.screenshot({ path: resolve(OUT, `friends-${tag}.png`) });
      // DM header with the phone-only person.
      const dmLink = page.getByRole('link', { name: /phoner/ }).first();
      if (await dmLink.count()) {
        await dmLink.click();
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `dm-header-${tag}.png`) });
      }
      await c.close();
    }
  }
  // The viewer's own avatar: the phone person on a phone alone sees their own glyph.
  for (const theme of THEMES) {
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      await apiPatch(people.phoner!.ctx.request, '/users/@me/settings', {
        settings: { user_prefs: { theme, zoom: 100 } },
      });
      const c = await context(browser, people.phoner!.ctx, size, theme, {}, {
        userAgent: PHONE_UA,
        isMobile: true,
        hasTouch: true,
      });
      const p = await c.newPage();
      await p.goto('/channels/@me');
      await p.waitForTimeout(1200);
      if (vp === 'desktop') {
        await expect(
          p.getByRole('button', { name: /currently Online on a phone/ }),
        ).toBeVisible();
      }
      await p.screenshot({ path: resolve(OUT, `self-${vp}-${theme}.png`) });
      await c.close();
    }
  }
  await phoner.ctx.close();
  await desk.close();
  await napper.ctx.close();
  await octx.close();
});

/** On a phone the member list is the pane right of the chat: swipe left, as a finger does. */
async function swipeToMembers(page: Page) {
  await page.evaluate(() => {
    const el = (document.querySelector('[data-pane="chat"]') ?? document.body) as HTMLElement;
    const touch = (x: number) =>
      new Touch({ identifier: 1, target: el, clientX: x, clientY: 400 });
    el.dispatchEvent(
      new TouchEvent('touchstart', { bubbles: true, touches: [touch(320)], changedTouches: [touch(320)] }),
    );
    el.dispatchEvent(new TouchEvent('touchend', { bubbles: true, touches: [], changedTouches: [touch(60)] }));
  });
}

async function signInDesktop(browser: Browser, who: { ctx: BrowserContext }): Promise<BrowserContext> {
  const ctx = await context(browser, who.ctx, VIEWPORTS.desktop, 'lounge-light');
  const page = await ctx.newPage();
  await page.goto('/channels/@me');
  await expect(page.getByRole('main')).toBeVisible();
  return ctx;
}

test('a reaction on my message raises one notification, and Settings has no digest switch', async ({
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
    { name: 'Reaction Booth' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const invite = await apiPost<{ code: string }>(o, `/channels/${channel}/invites`, {});
  const fan = await member(browser, 'fan', invite.code);
  const mine = await apiPost<{ id: string }>(o, `/channels/${channel}/messages`, {
    content: 'my own message that gets a reaction',
  });
  const theirs = await apiPost<{ id: string }>(fan.ctx.request, `/channels/${channel}/messages`, {
    content: 'a message by somebody else',
  });

  // The synced preferences win over the page's own copy: the earlier test left a theme-only blob on the account.
  await apiPatch(o, '/users/@me/settings', {
    settings: {
      user_prefs: {
        theme: 'lounge-light',
        zoom: 100,
        desktopNotifications: true,
        reactionNotifications: true,
      },
    },
  });
  const c = await context(browser, octx, VIEWPORTS.desktop, 'lounge-light', {
    desktopNotifications: true,
    reactionNotifications: true,
  });
  await c.grantPermissions(['notifications'], { origin: ORIGIN });
  await c.addInitScript(() => {
    const w = window as unknown as { __notes: { title: string; tag?: string; body?: string }[] };
    w.__notes = [];
    class FakeNotification {
      static permission = 'granted';
      onclick: (() => void) | null = null;
      constructor(title: string, opts: NotificationOptions = {}) {
        w.__notes.push({ title, tag: opts.tag, body: opts.body });
      }
      close() {}
      static requestPermission() {
        return Promise.resolve('granted');
      }
    }
    (window as unknown as { Notification: unknown }).Notification = FakeNotification;
  });
  const page = await c.newPage();
  await page.goto('/channels/@me');
  await expect(page.getByRole('main')).toBeVisible();
  await page.waitForTimeout(1500);
  const notes = () =>
    page.evaluate(() => (window as unknown as { __notes: unknown[] }).__notes) as Promise<
      { title: string; tag?: string; body?: string }[]
    >;

  // Somebody else's message: no notification. Mine: exactly one, titled and tagged as D-703 says.
  await apiPut(fan.ctx.request, `/channels/${channel}/messages/${theirs.id}/reactions/%F0%9F%91%8D/@me`, {});
  await apiPut(o, `/channels/${channel}/messages/${mine.id}/reactions/%F0%9F%8E%89/@me`, {});
  await page.waitForTimeout(1000);
  expect(await notes()).toHaveLength(0);
  await apiPut(fan.ctx.request, `/channels/${channel}/messages/${mine.id}/reactions/%F0%9F%91%8D/@me`, {});
  await expect.poll(async () => (await notes()).length).toBe(1);
  const [n] = await notes();
  expect(n!.title).toMatch(/^fan_\w+ reacted .* to your message$/);
  expect(n!.tag).toBe(`react-${mine.id}`);
  // The channel is not open, so the message is not loaded and the body stays empty (D-703).
  expect(n!.body ?? '').toBe('');

  // Settings > Notifications: the reactions switch with its description, no digest switch.
  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const sc = await context(browser, octx, size, theme);
      const sp = await sc.newPage();
      await sp.goto('/settings/user/notifications');
      await expect(sp.getByText('Notify me about reactions to my messages').first()).toBeVisible();
      await expect(
        sp.getByText('When someone reacts to a message you wrote. Shown while the app is open.'),
      ).toBeVisible();
      await expect(sp.getByText(/digest/i)).toHaveCount(0);
      await sp.getByText('Notify me about reactions to my messages').scrollIntoViewIfNeeded();
      await sp.waitForTimeout(500);
      await sp.screenshot({ path: resolve(OUT, `settings-notifications-${vp}-${theme}.png`) });
      // The card after the removal: the push select and its note, with no digest switch.
      await sp.getByText(/doesn't deliver Web Push yet/).scrollIntoViewIfNeeded();
      await sp.waitForTimeout(300);
      await sp.screenshot({ path: resolve(OUT, `settings-push-card-${vp}-${theme}.png`) });
      await sc.close();
    }
  }
  await c.close();
  await fan.ctx.close();
  await octx.close();
});
