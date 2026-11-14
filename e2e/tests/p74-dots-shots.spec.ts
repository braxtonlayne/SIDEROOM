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
 * SEC-AUD-294 (wave 74; D-779): the status dot on the smallest avatar that draws one (24 px: the DM channel header),
 * on the 80 px avatar of the profile popout, and on an avatar with a decoration. Four statuses plus the phone glyph.
 * Real stack, several accounts, Editorial / Lounge Light / Lounge Dark, desktop and phone.
 *
 * The profile catalogue holds no decoration (crates/sr-chat/src/profile.rs: `decorations are empty until artwork is
 * supplied`), so the server refuses any slug and a test account cannot get one. A decorated avatar is therefore a
 * FAKED profile answer (`page.route` adds `user_profile.avatar_decoration`) and a stand-in image for
 * `/cdn/avatar-decorations/<slug>.png`: the repository's own story decoration (packages/ui/src/stories/Misc.stories.tsx,
 * a gold ring) and, as a worst case, the same ring with a solid wedge over the bottom-right corner where the dot sits.
 * Nothing is downloaded.
 *
 * Wave 73's spec (p73-status-shots.spec.ts) already shot the 32 px avatar (member list, DM list, user panel, their
 * hovered rows) and the 80 px profile card for the phone glyph only.
 * Local only (SR_P74_DOTS=1); shots go to SR_P74_DOTS_OUT; the measurements go to measure.json beside them.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P74_DOTS_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p74-dots';
const ALL_THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
// SR_P74_DOTS_THEMES=lounge-dark reruns one theme (the sessions of the other people are not kept alive for ever).
const THEMES = ALL_THEMES.filter((t) =>
  (process.env.SR_P74_DOTS_THEMES ?? t).split(',').includes(t),
);
const DESKTOP = { width: 1280, height: 800 };
const PHONE = { width: 375, height: 812 };
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(process.env.SR_P74_DOTS !== '1', 'local visual check only');
test.setTimeout(2_400_000);

const RING =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><circle cx="20" cy="20" r="19" fill="none" stroke="gold" stroke-width="2"/></svg>';
const RING_WEDGE =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><circle cx="20" cy="20" r="19" fill="none" stroke="gold" stroke-width="2"/><path d="M40 22 A20 20 0 0 1 22 40 L40 40 Z" fill="#7a3fd0"/></svg>';

type Size = { width: number; height: number };
// Account names carry a run tag so a rerun on the same database never matches the last run's people.
const RUN = Date.now().toString(36).slice(-4);
// SR_P74_DOTS_ONLY=header: only the 24 px header part (a rerun for the hovered close-ups).
const ONLY_HEADER = process.env.SR_P74_DOTS_ONLY === 'header';
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

/**
 * Registration is rate limited per address (a long window after a few runs), so a rerun can reuse the people of an earlier
 * run on the same database: SR_P74_DOTS_PEOPLE='{"online":{"id":"..","username":".."},"idle":..,"dnd":..,"offline":..,"phone":..}'.
 * They share a server and a DM with the owner already.
 */
type Known = Record<string, { id: string; username: string }>;
const KNOWN: Known | null = process.env.SR_P74_DOTS_PEOPLE
  ? (JSON.parse(process.env.SR_P74_DOTS_PEOPLE) as Known)
  : null;

async function member(browser: Browser, label: string, inviteCode: string) {
  const known = KNOWN?.[label.replace(/x.*/, '')];
  if (known) {
    const ctx = await browser.newContext({ baseURL: ORIGIN });
    const user = {
      ...known,
      email: known.username + '@example.test',
      password: 'Seeded-Passw0rd-9!',
    };
    await loginApi(ctx.request, user);
    return { ctx, user };
  }
  const ctx = await browser.newContext({ baseURL: ORIGIN });
  const user = await registerUser(ctx.request, label);
  await loginApi(ctx.request, user);
  await apiPost(ctx.request, `/invites/${inviteCode}`, {});
  return { ctx, user };
}

const sessions: BrowserContext[] = [];

/** A session of `who`; `report` is sent as the first presence frame after READY (idle: afk; dnd), over the page's own socket. */
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

/** The faked profile answer: every profile read gets a decoration slug; the image is a stand-in. */
async function fakeDecoration(c: BrowserContext, wedge: boolean) {
  await c.route(/\/cdn\/avatar-decorations\//, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: wedge ? RING_WEDGE : RING,
    }),
  );
  await c.route(/\/users\/\d+\/profile(\?.*)?$/, async (route) => {
    const res = await route.fetch();
    const json = (await res.json()) as { user_profile?: Record<string, unknown> };
    json.user_profile = { ...(json.user_profile ?? {}), avatar_decoration: 'standin-ring' };
    await route.fulfill({ response: res, json });
  });
}

/** Box, colours and the surface under a status dot, measured in the page. */
async function measure(dot: Locator, hovered: boolean) {
  return dot.evaluate((el, h) => {
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    const avatar = el.parentElement!;
    const ar = avatar.getBoundingClientRect();
    let node: HTMLElement | null = avatar.parentElement;
    let surface = 'transparent';
    while (node) {
      const bg = getComputedStyle(node).backgroundColor;
      if (bg && bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent') {
        surface = bg;
        break;
      }
      node = node.parentElement;
    }
    const clip = (() => {
      // is the dot inside the avatar's overflow-clipping ancestors?
      let p: HTMLElement | null = avatar;
      while (p) {
        const o = getComputedStyle(p).overflow;
        if (o !== 'visible') {
          const pr = p.getBoundingClientRect();
          return {
            clipper: p.className,
            inside:
              r.left >= pr.left - 0.5 &&
              r.right <= pr.right + 0.5 &&
              r.top >= pr.top - 0.5 &&
              r.bottom <= pr.bottom + 0.5,
          };
        }
        p = p.parentElement;
      }
      return null;
    })();
    return {
      hovered: h,
      label: el.getAttribute('aria-label'),
      dot: { w: r.width, h: r.height },
      avatar: { w: ar.width, h: ar.height },
      dotOutsideAvatar: {
        right: +(r.right - ar.right).toFixed(2),
        bottom: +(r.bottom - ar.bottom).toFixed(2),
      },
      bg: cs.backgroundColor,
      ring: cs.borderTopColor,
      ringWidth: cs.borderTopWidth,
      surface,
      clip,
    };
  }, hovered);
}

/** A close-up of an avatar with room around it, so the dot's ring and a decoration are not cut off by the box. */
async function closeup(page: Page, avatar: Locator, path: string) {
  const b = (await avatar.boundingBox())!;
  const m = Math.round(b.width * 0.25) + 6;
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

const dotIn = (scope: Locator | Page, label: string) =>
  scope.locator(`[role="img"][aria-label="${label}"]`).first();

test('the status dot at 24 px, at 80 px, and on a decorated avatar, three themes, desktop and phone', async ({
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
    { name: 'Dot Rooms' },
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
  const LABEL: Record<string, string> = {
    online: 'Online',
    idle: 'Idle',
    dnd: 'Do not disturb',
    offline: 'Offline',
    phone: 'Online on a phone',
  };
  const kinds = Object.keys(LABEL);
  const memberList = (page: Page) => page.getByRole('list', { name: 'Members' });
  const settled = async (page: Page) => {
    const list = memberList(page);
    await expect(
      dotIn(list.getByRole('button', { name: new RegExp(`idlex${RUN}`) }), 'Idle'),
    ).toBeVisible({
      timeout: 25_000,
    });
    await expect(
      dotIn(list.getByRole('button', { name: new RegExp(`dndx${RUN}`) }), 'Do not disturb'),
    ).toBeVisible({ timeout: 25_000 });
    return list;
  };

  /** Open the profile popout of `kind` from the member list (desktop) and return its avatar. */
  const popoutOf = async (page: Page, kind: string) => {
    const list = await settled(page);
    await list.getByRole('button', { name: new RegExp(`${kind}x${RUN}`) }).click();
    const dialog = page.getByRole('dialog').last();
    await expect(dotIn(dialog, LABEL[kind]!)).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(500);
    return dialog;
  };

  for (const t of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme: t, zoom: 100 } } });

    for (const [form, size] of [
      ['desktop', DESKTOP],
      ['phone', PHONE],
    ] as const) {
      // ---- 24 px: the DM channel header, plain, hovered, and a scale-3 close-up.
      for (const scale of [1, 3]) {
        const c = await context(browser, octx, size, t, scale);
        const page = await c.newPage();
        page.setDefaultTimeout(20_000);
        for (const k of kinds) {
          await page.goto(`/channels/@me/${dm[k]}`);
          await expect(page.getByRole('main')).toBeVisible();
          const dot = dotIn(page.locator('.lng-header'), LABEL[k]!);
          await expect(dot).toBeVisible({ timeout: 25_000 });
          await page.waitForTimeout(500);
          const avatar = dot.locator('xpath=..');
          const tag = `${k}-${form}-${t}`;
          if (scale === 1) {
            measures[`header24-${tag}`] = await measure(dot, false);
            await page
              .locator('.lng-header')
              .first()
              .screenshot({
                path: resolve(OUT, `header24-${tag}.png`),
              });
            const identity = page.locator('.lng-header button', { has: dotIn(page, LABEL[k]!) });
            if (form === 'desktop' && (await identity.count())) {
              await identity.hover();
              await page.waitForTimeout(250);
              measures[`header24-hover-${tag}`] = await measure(dot, true);
              await page
                .locator('.lng-header')
                .first()
                .screenshot({
                  path: resolve(OUT, `header24-hover-${tag}.png`),
                });
              await page.mouse.move(700, 500);
            }
          } else {
            await closeup(page, avatar, resolve(OUT, `zoom-header24-${tag}.png`));
            const identity = page.locator('.lng-header button', { has: dotIn(page, LABEL[k]!) });
            if (form === 'desktop' && (await identity.count())) {
              await identity.hover();
              await page.waitForTimeout(250);
              await closeup(page, avatar, resolve(OUT, `zoom-header24-hover-${tag}.png`));
              await page.mouse.move(700, 500);
            }
          }
        }
        await c.close();
      }

      // ---- 80 px: the profile popout (desktop: from the member list). Phone: the member list is a swipe away; the
      // popout is shot from the DM's header button instead, which opens the same profile.
      for (const scale of ONLY_HEADER ? [] : [1, 3]) {
        const c = await context(browser, octx, size, t, scale);
        const page = await c.newPage();
        page.setDefaultTimeout(20_000);
        for (const k of kinds) {
          const tag = `${k}-${form}-${t}`;
          let dialog: Locator;
          if (form === 'desktop') {
            await page.goto(`/channels/${guild.id}/${channel}`);
            await expect(page.getByRole('main')).toBeVisible();
            dialog = await popoutOf(page, k);
          } else {
            await page.goto(`/channels/@me/${dm[k]}`);
            await expect(page.getByRole('main')).toBeVisible();
            await page.locator('.lng-header button', { has: dotIn(page, LABEL[k]!) }).click();
            dialog = page.getByRole('dialog').last();
            await expect(dotIn(dialog, LABEL[k]!)).toBeVisible({ timeout: 15_000 });
            await page.waitForTimeout(500);
          }
          const dot = dotIn(dialog, LABEL[k]!);
          if (scale === 1) {
            measures[`profile80-${tag}`] = await measure(dot, false);
            await page.screenshot({ path: resolve(OUT, `profile80-${tag}.png`) });
          } else {
            await closeup(page, dot.locator('xpath=..'), resolve(OUT, `zoom-profile80-${tag}.png`));
          }
          await page.keyboard.press('Escape');
          await page.waitForTimeout(200);
        }
        await c.close();
      }

      // ---- a decorated avatar (faked profile answer, stand-in image): the popout, with the ring and the wedge.
      for (const wedge of ONLY_HEADER ? [] : [false, true]) {
        for (const scale of [1, 3]) {
          const c = await context(browser, octx, size, t, scale);
          await fakeDecoration(c, wedge);
          const page = await c.newPage();
          page.setDefaultTimeout(20_000);
          for (const k of kinds) {
            const tag = `${wedge ? 'wedge' : 'ring'}-${k}-${form}-${t}`;
            let dialog: Locator;
            if (form === 'desktop') {
              await page.goto(`/channels/${guild.id}/${channel}`);
              await expect(page.getByRole('main')).toBeVisible();
              dialog = await popoutOf(page, k);
            } else {
              await page.goto(`/channels/@me/${dm[k]}`);
              await expect(page.getByRole('main')).toBeVisible();
              await page.locator('.lng-header button', { has: dotIn(page, LABEL[k]!) }).click();
              dialog = page.getByRole('dialog').last();
              await expect(dotIn(dialog, LABEL[k]!)).toBeVisible({ timeout: 15_000 });
              await page.waitForTimeout(500);
            }
            const dot = dotIn(dialog, LABEL[k]!);
            await expect(
              dot.locator('xpath=..').locator('img[aria-hidden], span[aria-hidden] img').first(),
            ).toBeAttached();
            if (scale === 1) {
              measures[`deco-${tag}`] = await measure(dot, false);
              await page.screenshot({ path: resolve(OUT, `deco-profile80-${tag}.png`) });
            } else {
              await closeup(
                page,
                dot.locator('xpath=..'),
                resolve(OUT, `zoom-deco-profile80-${tag}.png`),
              );
            }
            await page.keyboard.press('Escape');
            await page.waitForTimeout(200);
          }
          await c.close();
        }
      }
    }
    writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(measures, null, 1));
  }
  for (const c of sessions) await c.close();
  await octx.close();
  for (const p of Object.values(people)) await p.ctx.close();
});
