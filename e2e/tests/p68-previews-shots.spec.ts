import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
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
 * SEC-AUD-218, 219, 220 (wave 68; D-699, D-700, D-701), on the real stack:
 *  1. a stranger writes to the Owner, the Owner's Message Requests page shows the line, the stranger deletes the
 *     message and the line is gone without a reload (before and after shots);
 *  2. the thread row's relative time: threads whose last messages are seconds, minutes, hours and days old (the
 *     old ones by rewriting the message list answer with `page.route`), and the phone layout still holds;
 *  3. the "started a thread" row carries the chip: a thread started without a message (chip, then after a live
 *     reply) and one started from a message (one chip only: under the starter).
 * Editorial, Lounge Light and Lounge Dark, desktop and phone (and compact density for 2 and 3). Local only
 * (SR_P68_PREVIEWS=1); shots go to SR_P68_PREVIEWS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P68_PREVIEWS_OUT ?? resolve('test-results/p68-previews');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P68_PREVIEWS !== '1', 'local visual check only');
test.setTimeout(1_800_000);

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
  extra: Record<string, unknown> = {},
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
    { theme, zoom: 100, ...extra },
  );
  return c;
}

async function apiDelete(r: BrowserContext['request'], path: string) {
  const token = ((await (await r.get('/api/v1/sr/csrf')).json()) as { token: string }).token;
  const res = await r.delete(`/api/v1${path}`, { headers: { Origin: ORIGIN, 'X-SR-CSRF': token } });
  if (!res.ok()) throw new Error(`DELETE ${path} -> ${res.status()}: ${await res.text()}`);
}

test('a deleted message leaves the request preview without a reload', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const me = await (await o.get('/api/v1/users/@me')).json();

  const sctx = await browser.newContext({ baseURL: ORIGIN });
  const stranger = await registerUser(sctx.request, 'stranger68');
  await loginApi(sctx.request, stranger);
  const shared = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Request Booth' },
  );
  const invite = await apiPost<{ code: string }>(
    o,
    `/channels/${shared.channels.find((c) => c.type === 0)!.id}/invites`,
    {},
  );
  await apiPost(sctx.request, `/invites/${invite.code}`, {});
  const dm = await apiPost<{ id: string }>(sctx.request, '/users/@me/channels', {
    recipient_id: me.id,
  });

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const text = `Hi, it is me again (${tag}). Would you like to read the same book this month?`;
      await new Promise((r) => setTimeout(r, 1500));
      const sent = await apiPost<{ id: string }>(sctx.request, `/channels/${dm.id}/messages`, {
        content: text,
      });
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto('/channels/@me');
      await page.getByRole('button', { name: /Message Requests/ }).click();
      const region = page.getByRole('region', { name: 'Message Requests' });
      await expect(region.locator('[data-request-preview]')).toContainText(`(${tag})`);
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `requests-before-${tag}.png`) });
      // The stranger takes the message back: the line goes at once, the row stays.
      await apiDelete(sctx.request, `/channels/${dm.id}/messages/${sent.id}`);
      await expect(region.locator('[data-request-preview]')).toHaveCount(0);
      await expect(region.getByText(/stranger68/).first()).toBeVisible();
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `requests-after-${tag}.png`) });
      await c.close();
    }
  }
  await sctx.close();
  await octx.close();
});

/** Rewrite the age of the named threads' last messages in the message list answer (the stack cannot be old). */
async function ageThreads(page: Page, channel: string, agesMs: Record<string, number>) {
  await page.route(new RegExp(`/api/v1/channels/${channel}/messages(\\?.*)?$`), async (route) => {
    const res = await route.fetch();
    const list = (await res.json()) as {
      thread?: { name?: string; sr_last_message?: { timestamp: string } } | null;
    }[];
    for (const m of list) {
      const age = m.thread?.name ? agesMs[m.thread.name] : undefined;
      if (age !== undefined && m.thread?.sr_last_message) {
        m.thread.sr_last_message.timestamp = new Date(Date.now() - age).toISOString();
      }
    }
    await route.fulfill({ response: res, json: list });
  });
}

test('the thread row shows how long ago, in short words', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Clock Booth' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  // The server limits how fast one account may post: space the seeding out.
  const say = async (ch: string, content: string) => {
    await new Promise((r) => setTimeout(r, 1200));
    return apiPost<{ id: string }>(o, `/channels/${ch}/messages`, { content });
  };
  const ages: Record<string, number> = {
    'Fresh thread': 10_000,
    'Minutes thread': 5 * 60_000,
    'Hours thread': 3 * 3_600_000,
    'Days thread': 2 * 86_400_000,
    'Old thread': 45 * 86_400_000,
  };
  for (const [name, _age] of Object.entries(ages)) {
    void _age;
    const starter = await say(channel, `Starting: ${name}`);
    const t = await apiPost<{ id: string }>(
      o,
      `/channels/${channel}/messages/${starter.id}/threads`,
      { name },
    );
    await say(t.id, `The last words of ${name.toLowerCase()}, with a little more text after them`);
  }
  const expected: Record<string, string> = {
    'Fresh thread': 'just now',
    'Minutes thread': '5m ago',
    'Hours thread': '3h ago',
    'Days thread': '2d ago',
    'Old thread': 'more than 30 days ago',
  };

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      for (const density of ['cozy', 'compact'] as const) {
        const tag = `${vp}-${theme}${density === 'compact' ? '-compact' : ''}`;
        const c = await context(browser, octx, size, theme, { messageDisplay: density });
        const page = await c.newPage();
        page.setDefaultTimeout(20_000);
        await ageThreads(page, channel, ages);
        await page.goto(`/channels/${guild.id}/${channel}`);
        await expect(page.locator('.lng-thread-chip')).toHaveCount(5);
        for (const [name, text] of Object.entries(expected)) {
          const chip = page.locator('.lng-thread-chip', { hasText: name });
          await expect(chip.locator('time')).toHaveText(text);
          // The full date and time stay in the title.
          await expect(chip.locator('time')).toHaveAttribute('title', /\d/);
        }
        // The phone layout of wave 67 holds with the shorter time: the text sits on its own line.
        if (vp === 'phone') {
          const chip = page.locator('.lng-thread-chip', { hasText: 'Hours thread' });
          const box = await chip.boundingBox();
          expect(box!.x + box!.width).toBeLessThanOrEqual(size.width);
        }
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `times-${tag}.png`) });
        await c.close();
      }
    }
  }
  await octx.close();
});

test('the "started a thread" row carries the chip, once', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Notice Booth' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const say = (ch: string, content: string) =>
    apiPost<{ id: string }>(o, `/channels/${ch}/messages`, { content });
  // A thread started without a message: its "started a thread" row is the only place it shows.
  const loose = await apiPost<{ id: string }>(o, `/channels/${channel}/threads`, {
    name: 'Loose thread',
    type: 11,
  });
  await say(loose.id, 'The first reply of the loose thread');
  // A thread started from a message: the chip sits under the message, the notice row has none.
  const starter = await say(channel, 'Who is up for a game night?');
  const from = await apiPost<{ id: string }>(
    o,
    `/channels/${channel}/messages/${starter.id}/threads`,
    { name: 'Game night' },
  );
  await say(from.id, 'I can bring snacks');

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      for (const density of ['cozy', 'compact'] as const) {
        const tag = `${vp}-${theme}${density === 'compact' ? '-compact' : ''}`;
        const c = await context(browser, octx, size, theme, { messageDisplay: density });
        const page = await c.newPage();
        page.setDefaultTimeout(20_000);
        await page.goto(`/channels/${guild.id}/${channel}`);
        // Two chips in all: under the loose thread's notice, and under the starter message (not its notice).
        await expect(page.locator('.lng-thread-chip')).toHaveCount(2);
        const notices = page.locator('[data-system-type="18"]');
        await expect(notices).toHaveCount(2);
        await expect(
          notices.filter({ hasText: 'Loose thread' }).locator('.lng-thread-chip'),
        ).toContainText(/reply/);
        await expect(
          notices.filter({ hasText: 'Game night' }).locator('.lng-thread-chip'),
        ).toHaveCount(0);
        await expect(
          page.locator('.lng-msg:not(.lng-system) .lng-thread-chip', { hasText: 'Game night' }),
        ).toContainText('I can bring snacks');
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `notice-${tag}.png`) });
        // Live: a reply in the loose thread moves its chip without a reload.
        if (density === 'cozy') {
          const text = `A late reply (${tag})`;
          await say(loose.id, text);
          await expect(
            notices.filter({ hasText: 'Loose thread' }).locator('.lng-thread-chip'),
          ).toContainText(text);
          await page.waitForTimeout(300);
          await page.screenshot({ path: resolve(OUT, `notice-live-${tag}.png`) });
        }
        await c.close();
      }
    }
  }
  await octx.close();
});
