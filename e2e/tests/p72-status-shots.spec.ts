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
 * SEC-AUD-276 (wave 72; D-741, D-756): the four statuses and the phone glyph on every surface a status sits on: the
 * member list (desktop, phone), a hovered row, the profile card, the user panel, the DM list; and a close-up at scale
 * 3 of every row. Real stack, several accounts. Editorial, Lounge Light and Lounge Dark.
 * Local only (SR_P72_STATUS=1); shots go to SR_P72_STATUS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P72_STATUS_OUT ?? resolve('test-results/p72-status');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const DESKTOP = { width: 1280, height: 800 };
const PHONE = { width: 375, height: 812 };
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(process.env.SR_P72_STATUS !== '1', 'local visual check only');
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

async function swipeToMembers(page: Page) {
  const button = page.getByRole('button', { name: 'Member List' });
  if ((await button.getAttribute('aria-pressed')) !== 'true') await button.click();
  const cdp = await page.context().newCDPSession(page);
  const touch = (type: 'touchStart' | 'touchMove' | 'touchEnd', x: number) =>
    cdp.send('Input.dispatchTouchEvent', {
      type,
      touchPoints: type === 'touchEnd' ? [] : [{ x, y: 400 }],
    });
  await touch('touchStart', 330);
  for (const x of [280, 220, 160, 100, 50]) await touch('touchMove', x);
  await touch('touchEnd', 50);
  await cdp.detach();
}

const sessions: BrowserContext[] = [];

/** A session of `who`; `report` is sent as the first presence frame after READY (idle: afk; dnd), over the page's own socket. */
async function signIn(
  browser: Browser,
  who: { ctx: BrowserContext },
  agent: 'desktop' | 'phone',
  report: { status: string; afk: boolean } | null,
): Promise<void> {
  const c = await context(
    browser,
    who.ctx,
    agent === 'phone' ? PHONE : DESKTOP,
    'lounge-light',
    1,
    agent,
  );
  const page = await c.newPage();
  if (report) {
    await page.routeWebSocket(/gateway/, (ws) => {
      const server = ws.connectToServer();
      server.onMessage((m) => {
        ws.send(m);
        const raw = typeof m === 'string' ? m : m.toString('utf8');
        if (/"t"\s*:\s*"READY"/.test(raw)) {
          server.send(
            JSON.stringify({
              op: 3,
              d: {
                status: report.status,
                afk: report.afk,
                since: report.afk ? 1 : null,
                activities: [],
              },
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
}

test('the four statuses and the phone glyph on every surface, three themes, desktop and phone', async ({
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
    { name: 'Status Rooms' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const invite = await apiPost<{ code: string }>(o, `/channels/${channel}/invites`, {});
  const phoner = await member(browser, 'phonerq', invite.code);
  const deskman = await member(browser, 'deskmanq', invite.code);
  const idler = await member(browser, 'idlerq', invite.code);
  const dnder = await member(browser, 'dnderq', invite.code);
  const gone = await member(browser, 'offlinerq', invite.code);
  await signIn(browser, phoner, 'phone', null);
  await signIn(browser, deskman, 'desktop', null);
  await signIn(browser, idler, 'desktop', { status: 'online', afk: true });
  await signIn(browser, dnder, 'desktop', { status: 'dnd', afk: false });
  for (const who of [phoner, deskman, idler, dnder, gone]) {
    await apiPost(o, '/users/@me/channels', { recipient_id: who.user.id });
  }
  const names = ['phonerq', 'deskmanq', 'idlerq', 'dnderq', 'offlinerq'];
  const ready = async (page: Page) => {
    const list = page.getByRole('list', { name: 'Members' });
    await expect(
      list.getByRole('button', { name: /idlerq/ }).locator('[role="img"][aria-label="Idle"]'),
    ).toBeVisible({ timeout: 20_000 });
    await expect(
      list
        .getByRole('button', { name: /dnderq/ })
        .locator('[role="img"][aria-label="Do not disturb"]'),
    ).toBeVisible({ timeout: 20_000 });
    return list;
  };

  for (const t of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme: t, zoom: 100 } } });
    {
      const c = await context(browser, octx, DESKTOP, t, 1);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${guild.id}/${channel}`);
      await expect(page.getByRole('main')).toBeVisible();
      const list = await ready(page);
      await page.waitForTimeout(600);
      await page.screenshot({ path: resolve(OUT, `list-desktop-${t}.png`) });
      await list.getByRole('button', { name: /idlerq/ }).hover();
      await page.waitForTimeout(300);
      await page.screenshot({ path: resolve(OUT, `list-hover-desktop-${t}.png`) });
      await list.getByRole('button', { name: /phonerq/ }).click();
      await page.waitForTimeout(700);
      await page.screenshot({ path: resolve(OUT, `profile-card-desktop-${t}.png`) });
      await page.keyboard.press('Escape');
      await page.goto('/channels/@me');
      await expect(page.getByRole('main')).toBeVisible();
      await page.waitForTimeout(1500);
      await page.screenshot({ path: resolve(OUT, `dmlist-desktop-${t}.png`) });
      await page.screenshot({
        path: resolve(OUT, `userpanel-desktop-${t}.png`),
        clip: { x: 0, y: 700, width: 360, height: 100 },
      });
      await c.close();
    }
    {
      const c = await context(browser, octx, DESKTOP, t, 3);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${guild.id}/${channel}`);
      await expect(page.getByRole('main')).toBeVisible();
      const list = await ready(page);
      await page.waitForTimeout(600);
      for (const n of names) {
        await list
          .getByRole('button', { name: new RegExp(n) })
          .screenshot({ path: resolve(OUT, `zoom-${n}-${t}.png`) });
      }
      await c.close();
    }
    {
      const c = await context(browser, octx, PHONE, t, 2);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${guild.id}/${channel}`);
      await expect(page.getByRole('main')).toBeVisible();
      await swipeToMembers(page);
      const list = page.getByRole('list', { name: 'Members' });
      await expect(
        list
          .getByRole('button', { name: /phonerq/ })
          .locator('[role="img"][aria-label="Online on a phone"]'),
      ).toBeVisible();
      await page.waitForTimeout(600);
      await page.screenshot({ path: resolve(OUT, `list-phone-${t}.png`) });
      await list
        .getByRole('button', { name: /phonerq/ })
        .screenshot({ path: resolve(OUT, `zoom-glyph-phone-${t}.png`) });
      await page.goto('/channels/@me');
      await expect(page.getByRole('main')).toBeVisible();
      await page.waitForTimeout(1500);
      await page.screenshot({ path: resolve(OUT, `dmlist-phone-${t}.png`) });
      await c.close();
    }
  }
  for (const c of sessions) await c.close();
  await octx.close();
  for (const p of [phoner, deskman, idler, dnder, gone]) await p.ctx.close();
});
