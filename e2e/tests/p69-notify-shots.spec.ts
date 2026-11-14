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
 * SEC-AUD-227..231 (wave 69; D-711, D-712, D-716, D-717): the smartphone glyph on every status, and the desktop
 * notification as plain text, with the text under a reaction and nothing from a blocked account. Real stack, a
 * faked `Notification` that records title and body. Editorial, Lounge Light and Lounge Dark, desktop and phone.
 * Local only (SR_P69_NOTIFY=1); shots go to SR_P69_NOTIFY_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P69_NOTIFY_OUT ?? resolve('test-results/p69-notify');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(process.env.SR_P69_NOTIFY !== '1', 'local visual check only');
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
  const ctx = await context(
    browser,
    who.ctx,
    VIEWPORTS.phone,
    'lounge-light',
    {},
    {
      userAgent: PHONE_UA,
      isMobile: true,
      hasTouch: true,
    },
  );
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

test('the smartphone glyph appears for a phone-only person and goes back to the dot', async ({
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
  for (const label of ['phoner', 'deskman']) {
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

  const { c: ownerCtx, page } = await watch('lounge-light', VIEWPORTS.desktop);
  await page.goto(`/channels/${guild.id}/${channel}`);
  const rowOn = (pg: Page) => (re: RegExp) =>
    pg.getByRole('list', { name: 'Members' }).getByRole('button', { name: re });
  const row = rowOn(page);
  // The real stack proves the whole path: phone user agent -> IDENTIFY properties -> client_status -> glyph.
  await expect(row(/phoner/).locator('[role="img"][aria-label="Online on a phone"]')).toBeVisible();
  await expect(row(/deskman/).locator('[role="img"][aria-label="Online"]')).toBeVisible();

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
      await expect(
        row(/phoner/).locator('[role="img"][aria-label="Online on a phone"]'),
      ).toBeVisible();
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `memberlist-${tag}.png`) });
      await row(/phoner/).evaluate((el) => (el as HTMLElement).scrollIntoView());
      await page
        .locator('.lng-member')
        .first()
        .focus()
        .catch(() => undefined);
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
        await page
          .getByRole('button', { name: /Friends/ })
          .first()
          .click()
          .catch(() => undefined);
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
      const c = await context(
        browser,
        people.phoner!.ctx,
        size,
        theme,
        {},
        {
          userAgent: PHONE_UA,
          isMobile: true,
          hasTouch: true,
        },
      );
      const p = await c.newPage();
      await p.goto('/channels/@me');
      await p.waitForTimeout(1200);
      if (vp === 'desktop') {
        await expect(p.getByRole('button', { name: /currently Online on a phone/ })).toBeVisible();
      }
      await p.screenshot({ path: resolve(OUT, `self-${vp}-${theme}.png`) });
      await c.close();
    }
  }
  await phoner.ctx.close();
  await desk.close();
  await octx.close();
});

/** On a phone the member list is the pane right of the chat: swipe left, as a finger does. */
async function swipeToMembers(page: Page) {
  await page.evaluate(() => {
    const el = (document.querySelector('[data-pane="chat"]') ?? document.body) as HTMLElement;
    const touch = (x: number) => new Touch({ identifier: 1, target: el, clientX: x, clientY: 400 });
    el.dispatchEvent(
      new TouchEvent('touchstart', {
        bubbles: true,
        touches: [touch(320)],
        changedTouches: [touch(320)],
      }),
    );
    el.dispatchEvent(
      new TouchEvent('touchend', { bubbles: true, touches: [], changedTouches: [touch(60)] }),
    );
  });
}

async function signInDesktop(
  browser: Browser,
  who: { ctx: BrowserContext },
): Promise<BrowserContext> {
  const ctx = await context(browser, who.ctx, VIEWPORTS.desktop, 'lounge-light');
  const page = await ctx.newPage();
  await page.goto('/channels/@me');
  await expect(page.getByRole('main')).toBeVisible();
  return ctx;
}

test('a notification is plain text, a blocked account raises none, a reaction says which message', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const me = (await (await o.get('/api/v1/users/@me')).json()) as {
    id: string;
    username: string;
    global_name?: string | null;
  };
  const myName = me.global_name || me.username;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Notify Booth' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const invite = await apiPost<{ code: string }>(o, `/channels/${channel}/invites`, {});
  const bob = await member(browser, 'bob', invite.code);
  const older = await apiPost<{ id: string }>(o, `/channels/${channel}/messages`, {
    content: 'my older message that gets a reaction',
  });

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
  await page.waitForTimeout(2500);
  const notes = () =>
    page.evaluate(() => (window as unknown as { __notes: unknown[] }).__notes) as Promise<
      { title: string; tag?: string; body?: string }[]
    >;

  // D-716: a spoiler and a mention, as plain text. The recorded body goes into the handoff.
  await apiPost(bob.ctx.request, `/channels/${channel}/messages`, {
    content: `psst ||the ending|| <@${me.id}>`,
  });
  await expect.poll(async () => (await notes()).length).toBe(1);
  const [first] = await notes();
  console.log('RECORDED message notification:', JSON.stringify(first));
  expect(first!.body).toBe(`psst [spoiler] @${myName}`);
  expect(first!.body).not.toContain('ending');
  expect(first!.body).not.toContain(me.id);

  // D-712: the reaction on a message the page never loaded says which message.
  await apiPut(
    bob.ctx.request,
    `/channels/${channel}/messages/${older.id}/reactions/%F0%9F%91%8D/@me`,
    {},
  );
  await expect.poll(async () => (await notes()).length).toBe(2);
  const second = (await notes())[1]!;
  console.log('RECORDED reaction notification:', JSON.stringify(second));
  expect(second.tag).toBe(`react-${older.id}`);
  expect(second.body).toBe('my older message that gets a reaction');

  // D-717: block bob; the same mention raises nothing.
  await apiPut(o, `/users/@me/relationships/${bob.user.id}`, { type: 2 });
  await page.waitForTimeout(800);
  await apiPost(bob.ctx.request, `/channels/${channel}/messages`, {
    content: `again <@${me.id}>`,
  });
  await page.waitForTimeout(2000);
  expect(await notes()).toHaveLength(2);
  await c.close();
  await bob.ctx.close();
  await octx.close();
});
