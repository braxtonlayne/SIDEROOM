import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { BrowserContext, Page } from '@playwright/test';
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
 * SEC-AUD-237 / 239 / 240 / 241 (wave 70; D-721, D-725, D-726, D-727): the mention badge counts the same live and
 * after a reload (a blocked account's mention gives no badge, a role mention gives one), a reply pings unless the
 * ping is off, and the desktop notification follows the engine (a role mention notifies, a muted category does
 * not). Real stack, two accounts in one server, a faked `Notification` that records title and body.
 * Editorial, Lounge Light and Lounge Dark, desktop and phone. Local only (SR_P70_MENTIONS=1); shots go to
 * SR_P70_MENTIONS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P70_MENTIONS_OUT ?? resolve('test-results/p70-mentions');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P70_MENTIONS !== '1', 'local visual check only');
test.setTimeout(3_600_000);

interface Note {
  title: string;
  tag?: string;
  body?: string;
}

/** The count in a channel row's label ("talk, unread, 2 mentions"): 0 when it names none. */
const countIn = (label: string | null) => Number(/(\d+) mentions/.exec(label ?? '')?.[1] ?? 0);

for (const theme of THEMES) {
  for (const [vp, size] of Object.entries(VIEWPORTS)) {
    test(`mentions count the same live and after a reload, a reply pings: ${vp} ${theme}`, async ({
      browser,
    }) => {
      mkdirSync(OUT, { recursive: true });
      const tag = `${vp}-${theme}`;
      const owner = bootstrapState().owner;
      const octx = await browser.newContext({ baseURL: ORIGIN });
      await loginApi(octx.request, owner);
      const o = octx.request;
      const me = (await (await o.get('/api/v1/users/@me')).json()) as { id: string };
      const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
        o,
        '/guilds',
        { name: `Mentions ${tag}` },
      );
      // `talk` sits in a category, so the category's mute can be tried (D-727).
      const cat = await apiPost<{ id: string }>(o, `/guilds/${guild.id}/channels`, {
        name: 'Rooms',
        type: 4,
      });
      const talk = await apiPost<{ id: string }>(o, `/guilds/${guild.id}/channels`, {
        name: 'talk',
        type: 0,
        parent_id: cat.id,
      });
      const lobby = await apiPost<{ id: string }>(o, `/guilds/${guild.id}/channels`, {
        name: 'lobby',
        type: 0,
      });
      const invite = await apiPost<{ code: string }>(o, `/channels/${talk.id}/invites`, {});
      const role = await apiPost<{ id: string }>(o, `/guilds/${guild.id}/roles`, {
        name: 'crew',
        mentionable: true,
      });
      await apiPut(o, `/guilds/${guild.id}/members/${me.id}/roles/${role.id}`, {});
      const bobCtx = await browser.newContext({ baseURL: ORIGIN });
      const bob = await registerUser(bobCtx.request, `bob${vp.slice(0, 1)}${theme.slice(7, 9)}`);
      await loginApi(bobCtx.request, bob);
      await apiPost(bobCtx.request, `/invites/${invite.code}`, {});
      const carolCtx = await browser.newContext({ baseURL: ORIGIN });
      const carol = await registerUser(carolCtx.request, `cy${vp.slice(0, 1)}${theme.slice(7, 9)}`);
      await loginApi(carolCtx.request, carol);
      await apiPost(carolCtx.request, `/invites/${invite.code}`, {});
      const carolSays = (content: string) =>
        apiPost(carolCtx.request, `/channels/${talk.id}/messages`, { content });
      const say = (content: string, extra: Record<string, unknown> = {}) =>
        apiPost<{ id: string }>(bobCtx.request, `/channels/${talk.id}/messages`, {
          content,
          ...extra,
        });

      const mine = await apiPost<{ id: string }>(o, `/channels/${talk.id}/messages`, {
        content: 'my question for the room',
      });

      await apiPatch(o, '/users/@me/settings', {
        settings: {
          user_prefs: { theme, zoom: 100, desktopNotifications: true },
        },
      });
      const c: BrowserContext = await browser.newContext({
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
          const w = window as unknown as { __notes: Note[] };
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
        },
        { theme, zoom: 100, desktopNotifications: true },
      );
      await c.grantPermissions(['notifications'], { origin: ORIGIN });
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const notes = () => page.evaluate(() => (window as unknown as { __notes: Note[] }).__notes);
      // `list`: on a phone the channel list is its own pane (the badges live there): go back to it first.
      const shot = async (name: string, list = false) => {
        if (list && vp === 'phone') {
          const back = page.getByRole('button', { name: 'Back to channels' });
          if (await back.isVisible().catch(() => false)) await back.dispatchEvent('click');
        }
        await page.waitForTimeout(600);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };
      // The channel list is the left pane; on a phone it is the first pane of a server.
      const row = (p: Page) => p.locator('[aria-label^="talk"]').first();
      const rowLabel = async (p: Page) => (await row(p).getAttribute('aria-label')) ?? '';
      const readCount = async () => {
        const r = (await (await o.get('/api/v1/users/@me/read-states')).json()) as {
          entries: { id: string; mention_count: number }[];
        };
        return r.entries.find((e) => e.id === talk.id)?.mention_count ?? 0;
      };
      // The Owner reads the lobby: `talk` stays unread while mentions pile up in it.
      await page.goto(`/channels/${guild.id}/${lobby.id}`);
      await expect(page.getByRole('main')).toBeVisible();
      await page.waitForTimeout(2000);

      // 1. A direct mention: a badge on the channel, live.
      await say(`hello <@${me.id}>`);
      await expect.poll(async () => countIn(await rowLabel(page))).toBe(1);
      expect(await readCount()).toBe(1);
      await shot('a-direct-badge', true);

      // 2. A role the Owner holds: the badge counts it live too (it did only after a reload before D-721).
      await say(`crew <@&${role.id}>`);
      await expect.poll(async () => countIn(await rowLabel(page))).toBe(2);
      expect(await readCount()).toBe(2);
      await shot('b-role-badge', true);
      // D-727: one desktop notification per mention (the role one included, with the "mentioned you" title).
      const seen = await notes();
      console.log(`RECORDED notifications (${tag}):`, JSON.stringify(seen));
      expect(seen).toHaveLength(2);
      expect(seen[1]!.title).toMatch(/mentioned you/);

      // 3. A reply with the ping on (the default), then with it off.
      await say('replying with the ping on', { message_reference: { message_id: mine.id } });
      await expect.poll(async () => countIn(await rowLabel(page))).toBe(3);
      expect(await readCount()).toBe(3);
      expect(await notes()).toHaveLength(3);
      await say('replying quietly', {
        message_reference: { message_id: mine.id },
        allowed_mentions: { replied_user: false, parse: ['users', 'roles', 'everyone'] },
      });
      await page.waitForTimeout(1500);
      expect(countIn(await rowLabel(page))).toBe(3);
      expect(await readCount()).toBe(3);
      await shot('c-reply-ping-off-no-new-badge', true);

      // 4. Open the channel: the pinging reply's row is highlighted, the quiet one is not.
      await page.goto(`/channels/${guild.id}/${talk.id}`);
      await expect(page.getByText('replying with the ping on')).toBeVisible();
      await expect(page.getByText('replying quietly')).toBeVisible();
      const marked = async (text: string) =>
        page.locator('[data-mention]').filter({ hasText: text }).count();
      expect(await marked('replying with the ping on')).toBeGreaterThan(0);
      expect(await marked('replying quietly')).toBe(0);
      await shot('d-reply-highlight');

      // 5. Back to the lobby (the badge is read), block Bob: his next mention gives no badge, live or reloaded.
      await page.goto(`/channels/${guild.id}/${lobby.id}`);
      await page.waitForTimeout(1500);
      await apiPut(o, `/users/@me/relationships/${bob.id}`, { type: 2 });
      await page.waitForTimeout(1500);
      expect(countIn(await rowLabel(page))).toBe(0);
      const notesBefore = (await notes()).length;
      await say(`blocked, but <@${me.id}>`);
      await page.waitForTimeout(2000);
      const liveLabel = await rowLabel(page);
      expect(liveLabel).toMatch(/unread/);
      expect(countIn(liveLabel)).toBe(0);
      expect(await readCount()).toBe(0);
      // The blocked account raised no notification and no sound either.
      expect(await notes()).toHaveLength(notesBefore);
      await shot('e-blocked-no-badge-live', true);
      await page.reload();
      await expect(page.getByRole('main')).toBeVisible();
      await page.waitForTimeout(2500);
      const reloadLabel = await rowLabel(page);
      expect(reloadLabel).toMatch(/unread/);
      expect(countIn(reloadLabel)).toBe(countIn(liveLabel));
      expect(await readCount()).toBe(0);
      await shot('f-blocked-after-reload', true);

      // 6. D-727: a third member (not blocked); a muted category silences the role notification (the page reloads to read the setting).
      const parent = (await (await o.get(`/api/v1/channels/${talk.id}`)).json()) as {
        parent_id?: string | null;
      };
      const category = parent.parent_id ?? cat.id;
      if (category) {
        await page.goto(`/channels/${guild.id}/${lobby.id}`);
        await page.waitForTimeout(1500);
        await carolSays(`one more for <@&${role.id}>`);
        await expect.poll(async () => (await notes()).length).toBe(1);
        await apiPatch(o, `/users/@me/guilds/${guild.id}/settings`, {
          channel_overrides: {
            [category]: { muted: true, mute_config: { selected_time_window: -1 } },
          },
        });
        await page.reload();
        await expect(page.getByRole('main')).toBeVisible();
        await page.waitForTimeout(2500);
        await carolSays(`and another for <@&${role.id}>`);
        await page.waitForTimeout(2500);
        // Muted category: no notification at all (the page reloaded, so the record started empty).
        expect(await notes()).toHaveLength(0);
        console.log(`RECORDED after the category mute (${tag}):`, JSON.stringify(await notes()));
      }
      await c.close();
      await bobCtx.close();
      await carolCtx.close();
      await octx.close();
    });
  }
}
