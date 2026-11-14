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
 * Wave 71, signals, on the real stack. Local only (SR_P71_SIGNALS=1); shots go to SR_P71_SIGNALS_OUT.
 *
 * 1. SEC-AUD-259 (D-732): the person's own default level reaches the open app. The Owner has the Lounge open in one
 *    tab; in another tab of the same session they set the default to "Nothing" in Settings; a member writes in a
 *    server with nothing chosen: the faked `Notification` records none. They set it back to "All messages": one.
 * 2. SEC-AUD-260 (D-735): a thread whose latest message is one long spoiler and one whose latest message is one long
 *    bold sentence, and a Message Request of each: the chip and the row show text (the spoiler as a closed
 *    spoiler, never its words), loaded and live. Editorial, Lounge Light and Lounge Dark, desktop and phone.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P71_SIGNALS_OUT ?? resolve('test-results/p71-signals');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P71_SIGNALS !== '1', 'local visual check only');
test.setTimeout(3_600_000);

interface Note {
  title: string;
  tag?: string;
  body?: string;
}

const pause = (ms = 1300) => new Promise((r) => setTimeout(r, ms));

const FAKE_NOTIFICATION = () => {
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
};

test('the open app follows the default level set in another tab', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Default Level' },
  );
  const talk = guild.channels.find((c) => c.type === 0)!.id;
  const invite = await apiPost<{ code: string }>(o, `/channels/${talk}/invites`, {});
  const bobCtx = await browser.newContext({ baseURL: ORIGIN });
  const bob = await registerUser(bobCtx.request, 'bob71dflt');
  await loginApi(bobCtx.request, bob);
  await apiPost(bobCtx.request, `/invites/${invite.code}`, {});
  // The person's default starts as none chosen, so the server's own default ("all messages") rules.
  await apiPatch(o, '/users/@me/notification-defaults', { message_notifications: 0 });
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-light', zoom: 100, desktopNotifications: true } },
  });

  const c: BrowserContext = await browser.newContext({
    baseURL: ORIGIN,
    viewport: VIEWPORTS.desktop,
    storageState: await octx.storageState(),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme: 'lounge-light', zoom: 100, desktopNotifications: true },
  );
  await c.addInitScript(FAKE_NOTIFICATION);
  await c.grantPermissions(['notifications'], { origin: ORIGIN });
  // Tab A: the Lounge, on Home (not on the channel the member writes in).
  const app = await c.newPage();
  app.setDefaultTimeout(20_000);
  await app.goto('/channels/@me');
  await app.waitForTimeout(1500);
  const notes = () => app.evaluate(() => (window as unknown as { __notes: Note[] }).__notes);
  const say = (content: string) =>
    apiPost(bobCtx.request, `/channels/${talk}/messages`, { content });

  // Baseline: the default is "All messages", a message notifies. The gateway of a fresh page may still be
  // connecting: write until the first one is recorded (the count after it is the baseline).
  let said = 0;
  for (let i = 0; i < 8 && (await notes()).length === 0; i++) {
    await say(`baseline ${i}: you should hear this`);
    said++;
    await app.waitForTimeout(2500);
  }
  const base = (await notes()).length;
  console.log(`baseline: ${base} recorded after ${said} message(s)`);
  expect(base).toBeGreaterThan(0);

  // Tab B: Settings > Notifications, "Nothing".
  const settings = await c.newPage();
  settings.setDefaultTimeout(20_000);
  await settings.goto('/settings/user/notifications');
  await settings.getByRole('radio', { name: 'Nothing' }).click();
  await expect(settings.getByRole('radio', { name: 'Nothing' })).toBeChecked();
  await app.waitForTimeout(1500);
  await pause();
  await say('after nothing: silence');
  await app.waitForTimeout(2500);
  const afterNothing = await notes();
  console.log(`recorded after "Nothing": ${afterNothing.length - base}`);
  expect(afterNothing).toHaveLength(base);

  // And back: "All messages".
  await settings.getByRole('radio', { name: 'All messages' }).click();
  await expect(settings.getByRole('radio', { name: 'All messages' })).toBeChecked();
  await app.waitForTimeout(1500);
  await pause();
  await say('after all: heard again');
  await expect.poll(async () => (await notes()).length).toBe(base + 1);
  const final = await notes();
  console.log(
    `recorded after "All messages": ${final.length - base} (${JSON.stringify(final[base])})`,
  );
  expect(final[base]?.body).toContain('heard again');
  await c.close();
  await bobCtx.close();
  await octx.close();
});

const SECRET = 'swordfish';
const spoiler = (tag: string) =>
  `look ||${`the ${SECRET} is under the third stone from the left (${tag}), `.repeat(4)}||`;
const bold = (tag: string) =>
  `**${`the quick brown fox jumps over the lazy dog (${tag}) `.repeat(5).trim()}**`;

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

test('one long spoiler and one long bold sentence keep a thread chip and a request row', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const a = octx.request;
  const me = await (await a.get('/api/v1/users/@me')).json();
  const bctx = await browser.newContext({ baseURL: ORIGIN });
  const bUser = await registerUser(bctx.request, 'bob71prev');
  await loginApi(bctx.request, bUser);
  const b = bctx.request;
  const cctx = await browser.newContext({ baseURL: ORIGIN });
  const cUser = await registerUser(cctx.request, 'cat71prev');
  await loginApi(cctx.request, cUser);
  const cr = cctx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    { name: 'Closed Cuts' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const invite = await apiPost<{ code: string }>(a, `/channels/${channel}/invites`, {});
  await apiPost(b, `/invites/${invite.code}`, {});
  await apiPost(cr, `/invites/${invite.code}`, {});
  // Bob's request carries the spoiler, Cat's the bold sentence.
  const dmBob = await apiPost<{ id: string }>(b, '/users/@me/channels', { recipient_id: me.id });
  const dmCat = await apiPost<{ id: string }>(cr, '/users/@me/channels', { recipient_id: me.id });

  for (const theme of THEMES) {
    await apiPatch(a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const mkThread = async (kind: string, content: string) => {
        await pause();
        const starter = await apiPost<{ id: string }>(b, `/channels/${channel}/messages`, {
          content: `${kind} starter (${tag})`,
        });
        await pause();
        const name = `${kind} ${tag}`;
        const th = await apiPost<{ id: string }>(
          b,
          `/channels/${channel}/messages/${starter.id}/threads`,
          { name },
        );
        await pause();
        await apiPost(b, `/channels/${th.id}/messages`, { content });
        return { id: th.id, name };
      };
      const sp = await mkThread('Spoiler', spoiler(`loaded ${tag}`));
      const bo = await mkThread('Bold', bold(`loaded ${tag}`));

      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${guild.id}/${channel}`);
      const chipOf = (name: string) =>
        page.locator('.lng-msg:not(.lng-system) .lng-thread-chip', { hasText: name });
      const spChip = chipOf(sp.name);
      const boChip = chipOf(bo.name);
      await expect(spChip).toBeVisible();
      await expect(boChip).toBeVisible();
      await expect(spChip).toContainText('look');
      expect(await spChip.innerText()).not.toContain(SECRET);
      await expect(boChip).toContainText('the quick brown fox');
      await spChip.scrollIntoViewIfNeeded();
      await page.waitForTimeout(500);
      await page.screenshot({ path: resolve(OUT, `threads-spoiler-loaded-${tag}.png`) });
      await boChip.scrollIntoViewIfNeeded();
      await page.waitForTimeout(300);
      await page.screenshot({ path: resolve(OUT, `threads-bold-loaded-${tag}.png`) });

      // Live: the next message of each thread while the channel is open.
      await pause();
      await apiPost(b, `/channels/${sp.id}/messages`, { content: spoiler(`live ${tag}`) });
      await pause();
      await apiPost(b, `/channels/${bo.id}/messages`, { content: bold(`live ${tag}`) });
      await expect(spChip).toContainText('2 messages');
      await expect(boChip).toContainText('2 messages');
      await page.waitForTimeout(800);
      expect(await spChip.innerText()).not.toContain(SECRET);
      await expect(boChip).toContainText('the quick brown fox');
      await spChip.scrollIntoViewIfNeeded();
      await page.screenshot({ path: resolve(OUT, `threads-spoiler-live-${tag}.png`) });
      await boChip.scrollIntoViewIfNeeded();
      await page.waitForTimeout(300);
      await page.screenshot({ path: resolve(OUT, `threads-bold-live-${tag}.png`) });

      // Message Requests: loaded, then live.
      await pause();
      await apiPost(b, `/channels/${dmBob.id}/messages`, { content: spoiler(`req loaded ${tag}`) });
      await pause();
      await apiPost(cr, `/channels/${dmCat.id}/messages`, { content: bold(`req loaded ${tag}`) });
      await page.goto('/channels/@me');
      await page.getByRole('button', { name: /Message Requests/ }).click();
      const region = page.getByRole('region', { name: 'Message Requests' });
      const rowOf = (name: string) => region.getByRole('listitem').filter({ hasText: name });
      const bobRow = rowOf(bUser.username);
      const catRow = rowOf(cUser.username);
      await bobRow.locator('.lng-request-preview').waitFor();
      await catRow.locator('.lng-request-preview').waitFor();
      // A new account is filed as likely spam: the line shows after "Show message" (D-688).
      for (const row of [bobRow, catRow]) {
        const showIt = row.getByRole('button', { name: /Show the message/ });
        if (await showIt.count()) await showIt.click();
      }
      const bobLine = bobRow.locator('[data-request-preview]');
      const catLine = catRow.locator('[data-request-preview]');
      await expect(bobLine).toContainText('look');
      expect(await bobLine.innerText()).not.toContain(SECRET);
      await expect(catLine).toContainText('the quick brown fox');
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `requests-loaded-${tag}.png`) });
      await pause();
      await apiPost(b, `/channels/${dmBob.id}/messages`, { content: spoiler(`req live ${tag}`) });
      await pause();
      await apiPost(cr, `/channels/${dmCat.id}/messages`, { content: bold(`req live ${tag}`) });
      await page.waitForTimeout(1500);
      expect(await bobLine.innerText()).not.toContain(SECRET);
      await expect(bobLine).toContainText('look');
      await expect(catLine).toContainText('the quick brown fox');
      await page.screenshot({ path: resolve(OUT, `requests-live-${tag}.png`) });
      await c.close();
    }
  }
  await cctx.close();
  await bctx.close();
  await octx.close();
});
