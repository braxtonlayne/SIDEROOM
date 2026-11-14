import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext } from '@playwright/test';
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
 * SEC-AUD-190..193 (wave 66, loose): developer docs (the Gateway guide and its reference, the bot wording),
 * /library/upload landing on the submissions, the sign-in page, the debug bundle's two numbers in the report dialog,
 * the server menu, the welcome, an age-restricted channel in the sidebar, the invite dialog's temporary switch, the
 * member menu's Call, the custom status emoji and the rail's NEW badge. Editorial, Lounge Light and Lounge Dark,
 * desktop and phone. Local only (SR_P66_LOOSE=1); shots go to SR_P66_LOOSE_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P66_LOOSE_OUT ?? resolve('test-results/p66-loose');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P66_LOOSE !== '1', 'local visual check only');
test.setTimeout(1_800_000);

async function context(
  browser: Browser,
  base: BrowserContext | null,
  size: { width: number; height: number },
  theme: string,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    ...(base ? { storageState: await base.storageState() } : {}),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
      // A canvas stream stands in for the browser's capture prompt.
      const md = navigator.mediaDevices;
      if (md) {
        md.getDisplayMedia = async () => {
          const cv = document.createElement('canvas');
          cv.width = window.innerWidth;
          cv.height = window.innerHeight;
          const ctx = cv.getContext('2d')!;
          ctx.fillStyle = '#c9d4ee';
          ctx.fillRect(0, 0, cv.width, cv.height);
          return cv.captureStream(5);
        };
      }
    },
    { theme, zoom: 100 },
  );
  return c;
}

test('wave 66 loose: the docs, the debug numbers, the server menu and the small Lounge rows', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  // The server's create-guild limit: earlier runs may still count.
  await new Promise((r) => setTimeout(r, 35_000));
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const ownerId = ((await (await o.get('/api/v1/users/@me')).json()) as { id: string }).id;

  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Loose Booth' },
  );
  const general = guild.channels.filter((c) => c.type === 0)[0]!.id;
  await apiPost(o, `/channels/${general}/messages`, { content: 'A first message here' });
  const spicy = await apiPost<{ id: string }>(o, `/guilds/${guild.id}/channels`, {
    name: 'spicy',
    type: 0,
  });
  await apiPatch(o, `/channels/${spicy.id}`, { nsfw: true });

  await new Promise((r) => setTimeout(r, 31_000));
  // A server with no text channel shows the welcome cards.
  const bare = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    {
      name: 'Bare Hall',
    },
  );
  const token = ((await (await o.get('/api/v1/sr/csrf')).json()) as { token: string }).token;
  for (const ch of bare.channels.filter((c) => c.type === 0))
    await o.delete(`/api/v1/channels/${ch.id}`, {
      headers: { Origin: ORIGIN, 'X-SR-CSRF': token },
    });

  // A friend who joins the first server, for the member menu.
  const rctx = await browser.newContext({ baseURL: ORIGIN });
  const friend = await registerUser(rctx.request, 'dora');
  await rctx.close();
  const fctx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(fctx.request, friend);
  const inv = await apiPost<{ code: string }>(o, `/channels/${general}/invites`, {
    max_age: 3600,
    max_uses: 5,
  });
  await apiPost(fctx.request, `/invites/${inv.code}`, {});
  await apiPut(o, `/users/@me/relationships/${friend.id}`, { type: 1 });
  await apiPut(fctx.request, `/users/@me/relationships/${ownerId}`, { type: 1 });
  await fctx.close();

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const phone = vp === 'phone';
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(25_000);
      const shot = async (name: string) => {
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };
      // The report dialog needs its platform switch, and Library submissions theirs.
      await page.route('**/api/v1/sr/features/flags', async (r) => {
        const res = await r.fetch();
        const j = (await res.json()) as { flags: Record<string, boolean> };
        await r.fulfill({
          response: res,
          json: {
            flags: { ...j.flags, 'platform.beta_reports': true, 'platform.ingest_queues': true },
          },
        });
      });

      // 1. Developer docs: the Gateway guide opens at its own URL and links to the reference; the bot wording.
      await page.goto('/developers/docs/gateway');
      await expect(page.getByRole('heading', { name: 'Gateway', level: 1 })).toBeVisible();
      await shot('docs-gateway-guide');
      await page.getByRole('link', { name: /opcode and event tables/ }).click();
      await expect(page.getByRole('table', { name: 'Gateway opcodes' })).toBeVisible();
      expect(new URL(page.url()).pathname).toBe('/developers/docs/gateway-events');
      await shot('docs-gateway-reference');
      await page.goto('/developers/docs/sdks');
      await expect(page.getByText(/not published to npm/)).toBeVisible();
      await shot('docs-sdks');

      // 2. /library/upload lands on the member's submissions.
      await page.goto('/library/upload');
      await expect(page).toHaveURL(/\/library\/submissions$/);
      await shot('library-upload-redirect');

      // 3. The server menu, the invite dialog and the sidebar with an age-restricted channel (desktop only:
      // the phone shows the channel view first).
      await page.goto(`/channels/${guild.id}/${general}`);
      await expect(page.getByText('A first message here')).toBeVisible();
      const sidebar = page.getByTestId('channel-sidebar');
      if (!phone) {
        await expect(sidebar.getByRole('link', { name: /spicy, age-restricted/ })).toBeVisible();
        await shot('sidebar-age-restricted');
        await sidebar.getByRole('button', { name: /Loose Booth/ }).click();
        await expect(page.getByRole('menuitem', { name: 'Invite to Server' })).toBeVisible();
        await expect(page.getByRole('menuitem', { name: 'Server Support' })).toHaveCount(0);
        await expect(page.getByRole('menuitem', { name: 'App Directory' })).toHaveCount(0);
        await shot('server-menu');
        await page.getByRole('menuitem', { name: 'Invite to Server' }).click();
        const dlg = page.getByRole('dialog', { name: /Invite people to Loose Booth/ });
        await expect(dlg).toBeVisible();
        await dlg.getByRole('switch', { name: /Grant temporary membership/ }).click();
        await shot('invite-temporary');
        await page.keyboard.press('Escape');
      } else {
        await shot('phone-channel');
      }
      // The welcome keeps its three cards (D-670 adds none).
      await page.goto(`/channels/${bare.id}`);
      await expect(
        page.getByRole('list', { name: 'Getting started' }).getByRole('listitem'),
      ).toHaveCount(3);
      await shot('welcome');

      // 4. Member menu: Call (desktop, the list is open).
      if (!phone) {
        await page.goto(`/channels/${guild.id}/${general}`);
        const members = page.getByRole('list', { name: 'Members' });
        await members
          .getByRole('button', { name: new RegExp(`^${friend.username}`, 'i') })
          .click({ button: 'right' });
        await expect(page.getByRole('menuitem', { name: 'Call' })).toBeVisible();
        await shot('member-menu-call');
        await page.keyboard.press('Escape');
      }

      // 5. The rail's NEW badge (nothing opened yet in this browser), then the custom status with an emoji.
      await page.evaluate(() => localStorage.removeItem('sr.lounge.openedServers'));
      await page.goto('/channels/@me');
      await expect(page.getByText('NEW').first()).toBeVisible();
      await shot('rail-new-badge');
      await page
        .getByRole('button', { name: /Set status|status/i })
        .first()
        .click();
      await page.getByRole('menuitem', { name: /Custom Status/ }).click();
      const st = page.getByRole('dialog', { name: 'Set a custom status' });
      await st.getByRole('button', { name: 'Choose an emoji' }).click();
      await st.getByRole('button', { name: 'books' }).click();
      await st.getByLabel("What's happening?").fill('Reading manga');
      await shot('status-emoji-dialog');
      await st.getByRole('button', { name: 'Save' }).click();
      await expect(page.getByText('📚 Reading manga').first()).toBeVisible();
      await shot('status-emoji-shown');

      // 6. The report dialog with the preview of what is sent: the two numbers.
      if (phone) {
        await page
          .getByRole('navigation', { name: 'Primary' })
          .getByRole('button', { name: 'You' })
          .click();
        await page.getByRole('button', { name: 'Report a bug' }).click();
      } else {
        await page.keyboard.press('Alt+Shift+B');
      }
      const rd = page.getByRole('dialog', { name: 'Report a bug' });
      await expect(rd).toBeVisible();
      await rd.getByText('What is sent').click();
      await expect(rd.getByLabel(/debug information/)).toContainText('"reconnects"');
      await rd.getByLabel(/debug information/).scrollIntoViewIfNeeded();
      await shot('report-preview');
      await rd.getByRole('button', { name: 'Cancel' }).click();

      await page.unrouteAll({ behavior: 'ignoreErrors' });
      await c.close();
    }
  }
});

test('wave 66 loose: the sign-in page is unchanged and ignores ?methods=', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  for (const theme of THEMES) {
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const c = await context(browser, null, size, theme);
      const page = await c.newPage();
      await page.goto('/login?methods=passkey');
      await expect(page.getByRole('heading', { name: 'Come on in.' })).toBeVisible();
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `signin-${vp}-${theme}.png`) });
      await c.close();
    }
  }
});
