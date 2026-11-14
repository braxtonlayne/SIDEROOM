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
 * SEC-AUD-203, 204, 205 (wave 67; D-688, D-689, D-673): the message request preview, the thread row's last
 * message and the staff flag. The requests come from the real server (a stranger writes to the Owner; READY
 * carries the preview), plus two synthetic ones (an attachment-only message and Likely spam) added to READY in
 * the page with `routeWebSocket`, since the in-memory stack cannot upload a file or flag spam on demand. The
 * threads are real (three threads: one with a last message, one with none, one whose last message is from a
 * blocked person). The status menu is shot as staff (the Owner) and as a member, and the member's network log is
 * read for `/admin/` requests. Editorial, Lounge Light and Lounge Dark, desktop and phone, and the compact
 * message density for the thread row. Local only (SR_P67_PREVIEWS=1); shots go to SR_P67_PREVIEWS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P67_PREVIEWS_OUT ?? resolve('test-results/p67-previews');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P67_PREVIEWS !== '1', 'local visual check only');
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

/** Add two synthetic requests to the READY the page receives (the rest of the gateway passes through). */
async function addRequestsToReady(page: Page) {
  await page.routeWebSocket(/gateway/, (ws) => {
    const server = ws.connectToServer();
    server.onMessage((m) => {
      const text = typeof m === 'string' ? m : m.toString('utf8');
      if (!/"t"\s*:\s*"READY"/.test(text)) {
        ws.send(m);
        return;
      }
      const ready = JSON.parse(text) as { d: { private_channels?: unknown[] } };
      ready.d.private_channels = [
        ...(ready.d.private_channels ?? []),
        {
          id: '9100000000000000001',
          type: 1,
          recipients: [{ id: '9100000000000000011', username: 'photo_sender', bot: false }],
          is_message_request: true,
          sr_request_preview: {
            content: '',
            attachments: 3,
            timestamp: '2026-11-13T10:00:00.000000+00:00',
          },
        },
        {
          id: '9100000000000000002',
          type: 1,
          recipients: [{ id: '9100000000000000012', username: 'prize_bot_fan', bot: false }],
          is_message_request: true,
          sr_request_reason: 'spam',
          sr_request_preview: {
            content: 'You won a free prize! Claim it now at the link in my bio',
            attachments: 0,
            timestamp: '2026-11-13T10:01:00.000000+00:00',
          },
        },
      ];
      ws.send(JSON.stringify(ready));
    });
  });
}

test('the message request rows show what was written', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const me = await (await o.get('/api/v1/users/@me')).json();

  // A stranger writes to the Owner: a real request with a real preview (a long line with hostile bits).
  const sctx = await browser.newContext({ baseURL: ORIGIN });
  const stranger = await registerUser(sctx.request, 'stranger');
  await loginApi(sctx.request, stranger);
  const dm = await apiPost<{ id: string }>(sctx.request, '/users/@me/channels', {
    recipient_id: me.id,
  });
  await apiPost(sctx.request, `/channels/${dm.id}/messages`, {
    content:
      'Hi! I saw you in the book club.\n\n<img src=x onerror=alert(1)> [read this](http://example.test/x) ' +
      'Would you like to read the same book this month? '.repeat(6),
  });
  await sctx.close();

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await addRequestsToReady(page);
      await page.goto('/channels/@me');
      await page.getByRole('button', { name: /Message Requests/ }).click();
      const region = page.getByRole('region', { name: 'Message Requests' });
      await expect(region).toBeVisible();
      // The real preview: plain text on one line, nothing clickable inside it.
      const real = region.locator('[data-request-preview]').first();
      await expect(real).toContainText('Hi! I saw you in the book club.');
      await expect(real.locator('img, a')).toHaveCount(0);
      await expect(region.getByText('Sent 3 attachments')).toBeVisible();
      // Likely spam: the words are hidden until "Show message" is pressed (for that row only).
      await expect(region.getByText(/You won a free prize/)).toHaveCount(0);
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `requests-${tag}.png`) });
      await region.getByRole('button', { name: /Show the message from prize_bot_fan/ }).click();
      await expect(region.getByText(/You won a free prize/)).toBeVisible();
      await page.waitForTimeout(300);
      await page.screenshot({ path: resolve(OUT, `requests-spam-shown-${tag}.png`) });
      await c.close();
    }
  }
  await octx.close();
});

test('the thread row shows the last message, as one line without one, and a blocked author’s as blocked', async ({
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
    { name: 'Thread Booth' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const invite = await apiPost<{ code: string }>(o, `/channels/${channel}/invites`, {});
  const join = async (label: string) => {
    const ctx = await browser.newContext({ baseURL: ORIGIN });
    const user = await registerUser(ctx.request, label);
    await loginApi(ctx.request, user);
    await apiPost(ctx.request, `/invites/${invite.code}`, {});
    return { ctx, user };
  };
  const friend = await join('friend');
  const pest = await join('pest');

  const say = (r: typeof o, ch: string, content: string) =>
    apiPost<{ id: string }>(r, `/channels/${ch}/messages`, { content });
  const thread = async (starter: string, name: string) =>
    apiPost<{ id: string }>(o, `/channels/${channel}/messages/${starter}/threads`, { name });

  const a = await say(o, channel, 'Who is up for a game night on Saturday?');
  const ta = await thread(a.id, 'Saturday plans');
  await say(o, ta.id, 'I can host if the numbers are small.');
  await say(
    friend.ctx.request,
    ta.id,
    'I can bring the **snacks** and a board game or two, see you at 7 \u{1F389}',
  );
  const b = await say(o, channel, 'A question about the house rules.');
  await thread(b.id, 'House rules question');
  const c = await say(o, channel, 'Anyone seen the new trailer?');
  const tc = await thread(c.id, 'Trailer talk');
  await say(pest.ctx.request, tc.id, 'this trailer is trash and so are you all');
  // The Owner blocks the pest; the third thread's last message becomes "1 blocked message".
  await apiPut(o, `/users/@me/relationships/${pest.user.id}`, { type: 2 });
  await friend.ctx.close();
  await pest.ctx.close();

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      for (const density of ['cozy', 'compact'] as const) {
        const tag = `${vp}-${theme}${density === 'compact' ? '-compact' : ''}`;
        const ctx = await context(browser, octx, size, theme, { messageDisplay: density });
        const page = await ctx.newPage();
        page.setDefaultTimeout(20_000);
        await page.goto(`/channels/${guild.id}/${channel}`);
        const rows = page.locator('.lng-thread-chip');
        await expect(rows).toHaveCount(3);
        await expect(page.getByText(/I can bring the snacks/)).toBeVisible();
        await expect(page.locator('.lng-thread-chip', { hasText: 'Trailer talk' })).toContainText(
          '1 blocked message',
        );
        await expect(
          page.locator('.lng-thread-chip', { hasText: 'House rules question' }).locator('[data-thread-last]'),
        ).toHaveCount(0);
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `threads-${tag}.png`) });
        // Live: a new reply moves the row without a reload.
        if (density === 'cozy') {
          await say(o, ta.id, `A late reply (${tag})`);
          await expect(page.getByText(`A late reply (${tag})`)).toBeVisible();
          await page.waitForTimeout(300);
          await page.screenshot({ path: resolve(OUT, `threads-live-${tag}.png`) });
        }
        await ctx.close();
      }
    }
  }
  await octx.close();
});

test('the status menu asks the server nothing: Platform Admin for staff, none for a member', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const mctx = await browser.newContext({ baseURL: ORIGIN });
  const member = await registerUser(mctx.request, 'plainmember');
  await loginApi(mctx.request, member);

  for (const theme of THEMES) {
    await apiPatch(octx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme, zoom: 100 } },
    });
    await apiPatch(mctx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme, zoom: 100 } },
    });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      for (const [who, base] of [
        ['staff', octx],
        ['member', mctx],
      ] as const) {
        const tag = `${who}-${vp}-${theme}`;
        const c = await context(browser, base, size, theme);
        const page = await c.newPage();
        page.setDefaultTimeout(20_000);
        const admin: string[] = [];
        page.on('request', (r) => {
          if (/\/api\/v1\/admin\//.test(r.url())) admin.push(r.url());
        });
        await page.goto('/channels/@me');
        const set = page.getByRole('button', { name: /Set status/ });
        await set.click();
        const entry = page.getByRole('menuitem', { name: 'Platform Admin' });
        if (who === 'staff') await expect(entry).toBeVisible();
        else {
          await expect(page.getByRole('menuitem', { name: 'Invisible' })).toBeVisible();
          await expect(entry).toHaveCount(0);
        }
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `status-menu-${tag}.png`) });
        // The member's network log over a Lounge session holds no /admin/ request at all.
        if (who === 'member') expect(admin, `admin requests as a member (${tag})`).toEqual([]);
        await c.close();
      }
    }
  }
  await octx.close();
  await mctx.close();
});
