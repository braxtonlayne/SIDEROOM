import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
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
 * Wave 75 polish. SEC-AUD-296 (D-784): the ring round a status dot is the colour of the surface the avatar sits on,
 * hovered or selected rows included (member row, DM row, DM header identity, user panel; the four statuses and the phone
 * glyph form). SEC-AUD-297 (D-790): Server Settings > Discord bridge: the "What the bridge shares" box has the cards'
 * width and gap. Real stack, Editorial / Lounge Light / Lounge Dark, desktop 1280x800 and phone 375x812.
 * Local only (SR_P75_POLISH=1); shots go to SR_P75_POLISH_OUT; the measurements go to measure.json beside them.
 * The checks are soft: a run on the base lists every fault, a run on the fix passes.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P75_POLISH_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p75-polish';
const ALL_THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const THEMES = ALL_THEMES.filter((t) =>
  (process.env.SR_P75_POLISH_THEMES ?? t).split(',').includes(t),
);
const DESKTOP = { width: 1280, height: 800 };
const PHONE = { width: 375, height: 812 };
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(process.env.SR_P75_POLISH !== '1', 'local visual check only');
test.setTimeout(2_400_000);

type Size = { width: number; height: number };
const RUN = Date.now().toString(36).slice(-4);
const measures: Record<string, unknown> = {};

async function context(
  browser: Browser,
  base: BrowserContext,
  size: Size,
  theme: string,
  scale = 1,
) {
  const phone = size.width < 500;
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    deviceScaleFactor: scale,
    colorScheme: theme === 'lounge-dark' ? 'dark' : 'light',
    storageState: await base.storageState(),
    ...(phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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

const sessions: BrowserContext[] = [];

async function signIn(
  browser: Browser,
  who: { ctx: BrowserContext },
  agent: 'desktop' | 'phone',
  report: { status: string; afk: boolean } | null,
): Promise<void> {
  const c = await context(browser, who.ctx, agent === 'phone' ? PHONE : DESKTOP, 'lounge-light');
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

/** The ring of a dot (its border, or its background for the icon form) against the row's own surface. */
async function ringVsRow(dot: Locator, row: Locator) {
  const ring = await dot.evaluate((el) => {
    const cs = getComputedStyle(el);
    return parseFloat(cs.borderTopWidth) > 0 ? cs.borderTopColor : cs.backgroundColor;
  });
  const surface = await row.evaluate((el) => {
    let node: HTMLElement | null = el as HTMLElement;
    while (node) {
      const bg = getComputedStyle(node).backgroundColor;
      if (bg && bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent') return bg;
      node = node.parentElement;
    }
    return 'transparent';
  });
  return { ring, surface };
}

async function closeup(page: Page, avatar: Locator, path: string) {
  const b = (await avatar.boundingBox())!;
  const m = Math.round(b.width * 0.5) + 6;
  await page.screenshot({
    path,
    clip: {
      x: Math.max(0, b.x - m),
      y: Math.max(0, b.y - m),
      width: b.width + 2 * m,
      height: b.height + 2 * m,
    },
  });
}

const LABEL: Record<string, string> = {
  online: 'Online',
  idle: 'Idle',
  dnd: 'Do not disturb',
  offline: 'Offline',
  phone: 'Online on a phone',
};
const dotIn = (scope: Locator | Page, label: string) =>
  scope.locator(`[role="img"][aria-label="${label}"]`).first();

/** Measure, assert (soft: ring == surface) and shoot one state of one row. */
async function state(page: Page, dot: Locator, row: Locator, tag: string, hover: boolean) {
  if (hover) {
    await row.hover();
    await page.waitForTimeout(250);
  }
  const m = await ringVsRow(dot, row);
  measures[tag] = m;
  writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(measures, null, 1));
  expect.soft(m.ring, `${tag}: ring ${m.ring} vs surface ${m.surface}`).toBe(m.surface);
  await closeup(page, dot.locator('xpath=..'), resolve(OUT, `${tag}.png`));
  if (hover) await page.mouse.move(640, 400);
}

test('the status ring is the colour of the row under it (SEC-AUD-296)', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Ring Rooms' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const invite = await apiPost<{ code: string }>(o, `/channels/${channel}/invites`, {});
  const people = {
    online: await member(browser, 'onlinex' + RUN, invite.code),
    idle: await member(browser, 'idlex' + RUN, invite.code),
    dnd: await member(browser, 'dndx' + RUN, invite.code),
    offline: await member(browser, 'offlinex' + RUN, invite.code),
    phone: await member(browser, 'phonex' + RUN, invite.code),
  };
  await signIn(browser, people.online, 'desktop', null);
  await signIn(browser, people.idle, 'desktop', { status: 'online', afk: true });
  await signIn(browser, people.dnd, 'desktop', { status: 'dnd', afk: false });
  await signIn(browser, people.phone, 'phone', null);
  const dm: Record<string, string> = {};
  for (const [k, who] of Object.entries(people)) {
    const ch = await apiPost<{ id: string }>(o, '/users/@me/channels', {
      recipient_id: who.user.id,
    });
    dm[k] = ch.id;
  }
  const kinds = Object.keys(LABEL);

  for (const t of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme: t, zoom: 100 } } });
    for (const [form, size] of [
      ['desktop', DESKTOP],
      ['phone', PHONE],
    ] as const) {
      const c = await context(browser, octx, size, t, 3);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const tagOf = (s: string, k: string) => `${s}-${k}-${form}-${t}`;

      // ---- member rows (32 px): resting and hovered
      await page.goto(`/channels/${guild.id}/${channel}`);
      await expect(page.getByRole('main')).toBeVisible();
      if (form === 'phone') await swipeToMembers(page);
      const list = page.getByRole('list', { name: 'Members' });
      await expect(
        dotIn(list.getByRole('button', { name: new RegExp(`dndx${RUN}`) }), 'Do not disturb'),
      ).toBeVisible({ timeout: 25_000 });
      await expect(
        dotIn(list.getByRole('button', { name: new RegExp(`idlex${RUN}`) }), 'Idle'),
      ).toBeVisible({ timeout: 25_000 });
      await page.waitForTimeout(500);
      for (const k of kinds) {
        const row = list.getByRole('button', { name: new RegExp(`${k}x${RUN}`) });
        const dot = dotIn(row, LABEL[k]!);
        await state(page, dot, row, tagOf('member-rest', k), false);
        await state(page, dot, row, tagOf('member-hover', k), true);
      }

      // ---- DM rows (32 px) and the DM header identity (24 px), on each DM's own page
      for (const k of kinds) {
        await page.goto(`/channels/@me/${dm[k]}`);
        await expect(page.getByRole('main')).toBeVisible();
        const header = page.locator('.lng-header');
        const hdot = dotIn(header, LABEL[k]!);
        await expect(hdot).toBeVisible({ timeout: 25_000 });
        await page.waitForTimeout(600);
        const ident = page.locator('.lng-dm-identity');
        await state(page, hdot, ident, tagOf('identity-rest', k), false);
        await state(page, hdot, ident, tagOf('identity-hover', k), true);
        // the selected DM row (this DM), and a hovered row of another DM (the phone's list is a drawer: below)
        if (form === 'desktop') {
          const sidebar = page.locator('.lng-dm[data-selected]').first();
          if (await sidebar.count()) {
            const sdot = dotIn(sidebar, LABEL[k]!);
            if (await sdot.count()) {
              await state(page, sdot, sidebar, tagOf('dm-selected', k), false);
            }
          }
          const other = page.locator('.lng-dm:not([data-selected])', {
            has: dotIn(page, LABEL[k === 'online' ? 'idle' : 'online']!),
          });
          if (await other.count()) {
            const odot = dotIn(other.first(), LABEL[k === 'online' ? 'idle' : 'online']!);
            await state(page, odot, other.first(), tagOf('dm-rest', k), false);
            await state(page, odot, other.first(), tagOf('dm-hover', k), true);
          }
        }
      }
      if (form === 'phone') {
        await page.goto('/channels/@me');
        await page.waitForTimeout(1500);
        for (const k of kinds) {
          const row = page.locator('.lng-dm', { has: dotIn(page, LABEL[k]!) }).first();
          if (!(await row.count())) continue;
          const d = dotIn(row, LABEL[k]!);
          await state(page, d, row, tagOf('dm-rest', k), false);
          await state(page, d, row, tagOf('dm-hover', k), true);
        }
      }
      // ---- the user panel (32 px), if this layout shows one
      await page.goto('/channels/@me');
      await page.waitForTimeout(1200);
      const panel = page.locator('.lng-user-id').first();
      if (await panel.count()) {
        const pdot = panel.locator('[role="img"][aria-label]').first();
        if (await pdot.count()) {
          await state(page, pdot, panel, tagOf('userpanel-rest', 'me'), false);
          await state(page, pdot, panel, tagOf('userpanel-hover', 'me'), true);
        }
      }
      await c.close();
    }
    writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(measures, null, 1));
  }
  for (const c of sessions) await c.close();
  await octx.close();
  for (const p of Object.values(people)) await p.ctx.close();
});

// ---- SEC-AUD-297: the bridge page
async function fakeBridge(page: Page, guildId: string, mode: 'connected' | 'none' | 'fail') {
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/sr/features$`), async (route) => {
    const res = await route.fetch();
    const list = (await res.json()) as { id: string; enabled: boolean }[];
    await route.fulfill({
      response: res,
      json: list.map((m) => (m.id === 'bridge.enabled' ? { ...m, enabled: true } : m)),
    });
  });
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/bridge/status$`), (route) => {
    if (mode === 'fail') return route.fulfill({ status: 500, json: { code: 'internal' } });
    return route.fulfill({
      json: {
        platform_on: true,
        module_on: true,
        connected: mode === 'connected',
        bridge:
          mode === 'connected'
            ? {
                discord_guild_id: '1100000000000000001',
                discord_guild_name: 'Friends',
                bot_user_id: '1100000000000000002',
                bot_name: 'Relay',
                mode: 'selective',
                status: 'connected',
                intents_granted: (1 << 15) | 1,
                created_by: '1',
                created_at: 0,
                invite_url: 'https://discord.com/oauth2/authorize?client_id=1&scope=bot',
              }
            : null,
        maps: [],
        queued: 0,
        dead_letters: 0,
        leftovers: [],
        erasure_leftovers: [],
        is_owner: true,
      },
    });
  });
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/bridge/discord-channels$`), (route) =>
    route.fulfill({ json: [] }),
  );
}

test('the bridge info box has the cards width and gap (SEC-AUD-297)', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const guild = await apiPost<{ id: string }>(octx.request, '/guilds', { name: 'Bridge Rooms' });
  const geo: Record<string, unknown> = {};
  for (const t of THEMES) {
    await apiPatch(octx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: t, zoom: 100 } },
    });
    for (const [form, size] of [
      ['desktop', DESKTOP],
      ['phone', PHONE],
    ] as const) {
      for (const mode of ['none', 'connected', 'fail'] as const) {
        const c = await context(browser, octx, size, t, 1);
        const page = await c.newPage();
        page.setDefaultTimeout(20_000);
        await fakeBridge(page, guild.id, mode);
        await page.goto(`/settings/server/${guild.id}/bridge`);
        const heading = page.getByText('What the bridge shares').first();
        await expect(heading).toBeVisible();
        await page.waitForTimeout(800);
        const tag = `bridge-${mode}-${form}-${t}`;
        // the notice box: the closest ancestor of the heading that is a direct child of the column or the page
        const g = await heading.evaluate((h) => {
          const col = document.querySelector('.ss-bridge') as HTMLElement | null;
          const box = h.closest('[role=status],[role=alert]') as HTMLElement;
          const r = box.getBoundingClientRect();
          const sib = box.nextElementSibling as HTMLElement | null;
          const sr = sib ? sib.getBoundingClientRect() : null;
          // the first card: the first descendant of the column after the notice that has a border
          const cards = col
            ? Array.from(col.children).filter(
                (e) => e !== box && e.getBoundingClientRect().height > 0,
              )
            : [];
          const first = cards[0]?.getBoundingClientRect() ?? null;
          return {
            inColumn: !!col && col.contains(box),
            noticeRight: Math.round(r.right),
            noticeBottom: Math.round(r.bottom),
            firstCardRight: first ? Math.round(first.right) : null,
            firstCardTop: first ? Math.round(first.top) : null,
            nextTop: sr ? Math.round(sr.top) : null,
            overflowX: document.documentElement.scrollWidth > window.innerWidth,
          };
        });
        geo[tag] = g;
        if (mode === 'connected') {
          expect.soft(g.inColumn, `${tag}: the notice is in the bridge column`).toBe(true);
          expect
            .soft(g.noticeRight, `${tag}: notice right edge vs first card`)
            .toBe(g.firstCardRight);
          expect
            .soft((g.firstCardTop ?? 0) - g.noticeBottom, `${tag}: gap notice to first card`)
            .toBe(20);
        }
        expect.soft(g.overflowX, `${tag}: horizontal overflow`).toBe(false);
        await page.screenshot({ path: resolve(OUT, `${tag}.png`), fullPage: true });
        await c.close();
      }
    }
  }
  writeFileSync(resolve(OUT, 'bridge-geometry.json'), JSON.stringify(geo, null, 1));
  await octx.close();
});
