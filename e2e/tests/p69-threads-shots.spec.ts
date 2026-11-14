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
 * SEC-AUD-232 and 233 (wave 69; D-715, D-718), on the real stack, two accounts in one channel:
 *  1. account B starts a thread without a message: account A's open channel shows the notice row with its chip at
 *     once (shot); B writes in the thread twice: A's chip shows B's message, "just now" and the count moved (shot);
 *     B starts a thread from a message: A sees one chip, under the starter (shot);
 *  2. a thread of 50 or more (written for real: 52 messages) reads "50+ messages" in the chip,
 *     the Threads popover and a forum channel.
 * Editorial, Lounge Light and Lounge Dark, desktop and phone, and compact density for one set. Local only
 * (SR_P69_THREADS=1); shots go to SR_P69_THREADS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P69_THREADS_OUT ?? resolve('test-results/p69-threads');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P69_THREADS !== '1', 'local visual check only');
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

const pause = (ms = 1300) => new Promise((r) => setTimeout(r, ms));

async function world(browser: Browser, name: string) {
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const a = octx.request;
  const bctx = await browser.newContext({ baseURL: ORIGIN });
  const bUser = await registerUser(bctx.request, `bob69${name.replace(/\W/g, '').toLowerCase()}`);
  await loginApi(bctx.request, bUser);
  const b = bctx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    {
      name,
    },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const invite = await apiPost<{ code: string }>(a, `/channels/${channel}/invites`, {});
  await apiPost(b, `/invites/${invite.code}`, {});
  const say = async (as: typeof a, ch: string, content: string) => {
    await pause();
    return apiPost<{ id: string }>(as, `/channels/${ch}/messages`, { content });
  };
  return { octx, bctx, a, b, guild, channel, say };
}

test('a thread that starts while the channel is open shows its chip, and the count moves', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const w = await world(browser, 'Live Booth');
  const combos: { vp: keyof typeof VIEWPORTS; theme: string; compact: boolean }[] = [];
  for (const theme of THEMES)
    for (const vp of ['desktop', 'phone'] as const) combos.push({ vp, theme, compact: false });
  combos.push({ vp: 'desktop', theme: 'lounge-light', compact: true });

  for (const { vp, theme, compact } of combos) {
    const tag = `${vp}-${theme}${compact ? '-compact' : ''}`;
    await apiPatch(w.a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    const c = await context(browser, w.octx, VIEWPORTS[vp], theme, {
      messageDisplay: compact ? 'compact' : 'cozy',
    });
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    await page.goto(`/channels/${w.guild.id}/${w.channel}`);
    await expect(page.getByRole('log')).toBeVisible();

    // 1. A thread without a message: the notice row has its chip at once, no reload.
    const loose = `Loose ${tag}`;
    await pause();
    const t = await apiPost<{ id: string }>(w.b, `/channels/${w.channel}/threads`, {
      name: loose,
      type: 11,
    });
    const notice = page.locator('[data-system-type="18"]', { hasText: loose });
    await expect(notice.locator('.lng-thread-chip')).toBeVisible();
    await expect(notice.locator('.lng-thread-chip')).toContainText('0 messages');
    await page.waitForTimeout(400);
    await page.screenshot({ path: resolve(OUT, `row-new-${tag}.png`) });

    // 2. B writes twice: the latest message, "just now" and the count follow.
    await w.say(w.b, t.id, 'First words in the loose thread');
    const words = `Second words, a little longer, to see the row hold (${tag})`;
    await w.say(w.b, t.id, words);
    const chip = notice.locator('.lng-thread-chip');
    await expect(chip).toContainText(words.slice(0, 30));
    await expect(chip).toContainText('2 messages');
    await expect(chip.locator('time')).toHaveText('just now');
    if (vp === 'phone') {
      const box = await chip.boundingBox();
      expect(box!.x + box!.width).toBeLessThanOrEqual(VIEWPORTS.phone.width);
    }
    await page.waitForTimeout(400);
    await page.screenshot({ path: resolve(OUT, `row-live-${tag}.png`) });

    // 3. A thread started from a message: one chip, under the starter, none in the notice.
    const name = `From ${tag}`;
    const starter = await w.say(w.b, w.channel, `Anyone up for a game? (${tag})`);
    await expect(
      page.locator('.sr-md', { hasText: `Anyone up for a game? (${tag})` }),
    ).toBeVisible();
    await pause();
    await apiPost(w.b, `/channels/${w.channel}/messages/${starter.id}/threads`, { name });
    await expect(page.locator('[data-system-type="18"]', { hasText: name })).toBeVisible();
    await expect(
      page.locator('.lng-msg:not(.lng-system) .lng-thread-chip', { hasText: name }),
    ).toBeVisible();
    await expect(
      page.locator('[data-system-type="18"]', { hasText: name }).locator('.lng-thread-chip'),
    ).toHaveCount(0);
    await expect(page.locator('.lng-thread-chip', { hasText: name })).toHaveCount(1);
    await page.waitForTimeout(400);
    await page.screenshot({ path: resolve(OUT, `row-from-message-${tag}.png`) });
    await c.close();
  }
  await w.octx.close();
  await w.bctx.close();
});

test('a thread of 50 or more reads "50+ messages" in the chip, the Threads list and a forum', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const w = await world(browser, 'Cap Booth');
  const big = 'Big thread';
  const starter = await w.say(w.b, w.channel, 'A very long conversation starts here');
  await pause();
  const th = await apiPost<{ id: string }>(
    w.b,
    `/channels/${w.channel}/messages/${starter.id}/threads`,
    { name: big },
  );
  await w.say(w.b, th.id, 'Message number one of very many, said a while ago and still going');
  const forum = await apiPost<{ id: string }>(w.a, `/guilds/${w.guild.id}/channels`, {
    name: 'help',
    type: 15,
  });
  await pause();
  const post = await apiPost<{ id: string }>(w.b, `/channels/${forum.id}/threads`, {
    name: 'A post with many answers',
    message: { content: 'Where do I start with all of this?' },
  });
  expect(post.id).toBeTruthy();

  // The server stops counting at 50 (MESSAGE_COUNT_CAP): write 52 messages in each, for real.
  for (let i = 2; i <= 52; i++) {
    await pause(1150);
    await apiPost(w.b, `/channels/${th.id}/messages`, {
      content: `Message number ${i} of very many`,
    });
  }
  for (let i = 2; i <= 52; i++) {
    await pause(1150);
    await apiPost(w.b, `/channels/${post.id}/messages`, { content: `Answer number ${i}` });
  }

  for (const theme of THEMES) {
    await apiPatch(w.a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, w.octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${w.guild.id}/${w.channel}`);
      const chip = page.locator('.lng-thread-chip', { hasText: big });
      await expect(chip).toContainText('50+ messages');
      const box = await chip.boundingBox();
      expect(box!.x + box!.width).toBeLessThanOrEqual(size.width);
      // The latest message's line is still there (not pushed out of the row).
      await expect(chip.locator('[data-thread-last]')).toBeVisible();
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `cap-chip-${tag}.png`) });

      await page.getByRole('button', { name: 'Threads' }).click();
      await expect(
        page.getByRole('button', { name: `${big} 50+ messages`, exact: true }),
      ).toContainText('50+ messages');
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `cap-threads-${tag}.png`) });

      await page.goto(`/channels/${w.guild.id}/${forum.id}`);
      const list = page.getByRole('list', { name: 'Posts' });
      await expect(list.locator('li', { hasText: 'A post with many answers' })).toContainText(
        '50+ messages',
      );
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `cap-forum-${tag}.png`) });
      await c.close();
    }
  }
  await w.octx.close();
  await w.bctx.close();
});
