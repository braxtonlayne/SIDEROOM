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
 * SEC-AUD-168 (wave 64, D-647..D-650): the beta bug reporter's screens. The switch, the report route and the staff
 * routes are faked with `page.route` (the report builder's server is a separate branch), and `getDisplayMedia` is
 * replaced by a canvas stream, because a real capture prompt cannot be driven by a test: the key path (private
 * rectangles, capture, one frame, cover scaling) is the real one. Editorial, Lounge Light and Lounge Dark, desktop and
 * phone. Local only (SR_P64_REPORTUI=1); shots go to SR_P64_REPORTUI_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P64_REPORTUI_OUT ?? resolve('test-results/p64-reportui');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P64_REPORTUI !== '1', 'local visual check only');
test.setTimeout(1_800_000);

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
      // A canvas stream stands in for the browser's capture prompt.
      const md = navigator.mediaDevices;
      if (md) {
        md.getDisplayMedia = async () => {
          const cv = document.createElement('canvas');
          cv.width = window.innerWidth;
          cv.height = window.innerHeight;
          const ctx = cv.getContext('2d')!;
          const paint = () => {
            const g = ctx.createLinearGradient(0, 0, cv.width, cv.height);
            g.addColorStop(0, '#e8eefc');
            g.addColorStop(1, '#a9b8d8');
            ctx.fillStyle = g;
            ctx.fillRect(0, 0, cv.width, cv.height);
            ctx.fillStyle = '#233';
            ctx.font = '20px sans-serif';
            ctx.fillText('fixture picture of the tab', 24, 40);
            for (let i = 0; i < 12; i++) ctx.fillRect(24, 70 + i * 40, 120 + ((i * 53) % 300), 14);
          };
          paint();
          const stream = cv.captureStream(10);
          const id = setInterval(paint, 100);
          stream.getTracks().forEach((t) => {
            const stop = t.stop.bind(t);
            t.stop = () => {
              clearInterval(id);
              stop();
            };
          });
          return stream;
        };
      }
    },
    { theme, zoom: 100 },
  );
  return c;
}

const T = Date.now();
const ROW = {
  id: '12',
  created_ms: T - 3_600_000,
  status: 'open',
  route: 'lounge.:guildId.:channelId',
  commit: '0a1b2c3d4e5f6a7b',
  reporter: { id: '9', username: 'bea' },
  has_screenshot: true,
};
const DETAIL = {
  ...ROW,
  description: 'The message list stays empty after I reconnect. I have to reload the page.',
  closed_ms: null,
  closed_by: null,
  bundle: {
    commit: '0a1b2c3d4e5f6a7b',
    route: 'lounge.:guildId.:channelId',
    at: T - 3_600_000,
    viewport: { w: 1280, h: 800, dpr: 2 },
    caps: { engine: 'chromium', wasm: true, webgpu: false, web_audio: true },
    locale: 'en',
    theme: 'lounge-dark',
    online: true,
    gateway: { status: 'connected' },
    sw: 'active',
    voice: { connected: false },
    failed_requests: [
      { method: '?', path: '/api/v1/channels/:id/messages', status: 500, ms: 312, ago_s: 14 },
      { method: '?', path: '/api/v1/users/@me/guilds', status: 503, ms: 80, ago_s: 20 },
    ],
    crashes: [],
  },
};

async function fakeFlagsAndStaff(page: Page, reply: { status: number; body: unknown }) {
  await page.route('**/api/v1/sr/features/flags', async (r) => {
    const res = await r.fetch();
    const j = (await res.json()) as { flags: Record<string, boolean> };
    await r.fulfill({
      response: res,
      json: { flags: { ...j.flags, 'platform.beta_reports': true } },
    });
  });
  await page.route('**/api/v1/bug-reports', (r) =>
    r.request().method() === 'POST'
      ? r.fulfill({
          status: reply.status,
          contentType: 'application/json',
          body: JSON.stringify(reply.body),
        })
      : r.fallback(),
  );
}

test('the bug reporter: key, menu entry, dialog, errors, sent, You tab and the Admin page', async ({
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
    { name: 'Report Booth' },
  );
  const channelId = guild.channels.filter((c) => c.type === 0)[0]!.id;
  await apiPost(o, `/channels/${channelId}/messages`, { content: 'A message in the server' });

  const rctx = await browser.newContext({ baseURL: ORIGIN });
  const friend = await registerUser(rctx.request, 'dora');
  await rctx.close();
  const fctx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(fctx.request, friend);
  await apiPut(o, `/users/@me/relationships/${friend.id}`, { type: 1 });
  await apiPut(fctx.request, `/users/@me/relationships/${ownerId}`, { type: 1 });
  await fctx.close();
  const dm = await apiPost<{ id: string }>(o, '/users/@me/channels', { recipient_id: friend.id });
  await apiPost(o, `/channels/${dm.id}/messages`, {
    content: 'A private line nobody else should see',
  });

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const phone = vp === 'phone';
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const shot = async (name: string) => {
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };
      await fakeFlagsAndStaff(page, { status: 429, body: { message: 'slow down' } });

      // 1. The entry: the "?" menu on a desktop, the You tab on a phone. Both open the dialog without a picture.
      await page.goto(`/channels/${guild.id}/${channelId}`);
      await expect(page.getByText('A message in the server')).toBeVisible();
      if (phone) {
        await page
          .getByRole('navigation', { name: 'Primary' })
          .getByRole('button', { name: 'You' })
          .click();
        await expect(page.getByRole('button', { name: 'Report a bug' })).toBeVisible();
        await shot('you-tab');
        await page.getByRole('button', { name: 'Report a bug' }).click();
      } else {
        await page.getByRole('button', { name: 'Help' }).last().click();
        await expect(page.getByRole('menuitem', { name: 'Report a bug' })).toBeVisible();
        await shot('help-menu');
        await page.getByRole('menuitem', { name: 'Report a bug' }).click();
      }
      const dialog = page.getByRole('dialog', { name: 'Report a bug' });
      await expect(dialog).toBeVisible();
      await shot('dialog-empty');
      // Send is off until something is written.
      await expect(dialog.getByRole('button', { name: 'Send' })).toBeDisabled();

      // 2. Attach a picture (the path every browser has): the warning line, a drawn box, the exact JSON.
      const fixture = await page.screenshot();
      await dialog.getByTestId('br-file').setInputFiles({
        name: 'screen.png',
        mimeType: 'image/png',
        buffer: fixture,
      });
      const stage = dialog.getByTestId('br-stage');
      await expect(stage).toBeVisible();
      await expect(dialog.getByText(/Check the picture: black out/)).toBeVisible();
      const box = (await stage.boundingBox())!;
      await page.mouse.move(box.x + box.width * 0.1, box.y + box.height * 0.15);
      await page.mouse.down();
      await page.mouse.move(box.x + box.width * 0.55, box.y + box.height * 0.4, { steps: 5 });
      await page.mouse.up();
      await expect(dialog.getByRole('button', { name: 'Remove black box 1' })).toBeVisible();
      await dialog
        .getByLabel('What went wrong?')
        .fill('The message list stays empty after I reconnect.');
      await shot('dialog-attached');
      await dialog.getByText('What is sent').click();
      await expect(dialog.getByLabel(/debug information/)).toContainText('"failed_requests"');
      await dialog.getByLabel(/debug information/).scrollIntoViewIfNeeded();
      await shot('dialog-json');

      // 3. The server refuses (too many reports): said in words, the text is kept; then it goes through.
      await dialog.getByRole('button', { name: 'Send' }).click();
      await expect(dialog.getByRole('alert')).toContainText('several reports lately');
      await expect(dialog.getByLabel('What went wrong?')).toHaveValue(/stays empty/);
      await shot('dialog-error');
      await page.unroute('**/api/v1/bug-reports');
      await page.route('**/api/v1/bug-reports', (r) =>
        r.request().method() === 'POST'
          ? r.fulfill({ status: 201, contentType: 'application/json', body: '{"id":"12"}' })
          : r.fallback(),
      );
      await dialog.getByRole('button', { name: 'Send' }).click();
      await expect(page.getByText('Thank you. Report #12 was sent.')).toBeVisible();
      await shot('dialog-sent');
      await page
        .getByRole('dialog', { name: 'Report sent' })
        .getByRole('button', { name: 'Close' })
        .last()
        .click();
      await expect(page.getByRole('dialog', { name: 'Report sent' })).toHaveCount(0);

      // 4. The key, in a direct message: the capture is taken, and the list and the message box start covered.
      if (!phone) {
        await page.goto(`/channels/@me/${dm.id}`);
        await expect(page.getByText('A private line nobody else should see')).toBeVisible();
        await page.keyboard.press('Alt+Shift+B');
        const kd = page.getByRole('dialog', { name: 'Report a bug' });
        await expect(kd).toBeVisible();
        await expect(kd.getByRole('button', { name: 'Remove black box 1' })).toBeVisible();
        await expect(kd.getByText(/already blacked out/)).toBeVisible();
        await shot('dialog-key-covers');
        await kd.getByRole('button', { name: 'Remove black box 1' }).click();
        await shot('dialog-key-cover-removed');
        await kd.getByRole('button', { name: 'Cancel' }).click();
        await expect(page.getByRole('dialog', { name: 'Report a bug' })).toHaveCount(0);
      }

      // 5. Admin > Bug reports: list with the badge, the report, the picture on click, the close confirm.
      const shotPng = fixture;
      await page.route(/\/api\/v1\/admin\/bug-reports(\?.*)?$/, (r) =>
        r.fulfill({
          json: {
            reports: [
              ROW,
              { ...ROW, id: '11', has_screenshot: false, reporter: { id: '8', username: 'carl' } },
            ],
            open_count: 2,
          },
        }),
      );
      await page.route('**/api/v1/admin/bug-reports/12', (r) => r.fulfill({ json: DETAIL }));
      await page.route('**/api/v1/admin/bug-reports/12/screenshot', (r) =>
        r.fulfill({ status: 200, contentType: 'image/jpeg', body: shotPng }),
      );
      await page.goto('/admin/bug-reports');
      await expect(page.getByRole('table', { name: 'Bug reports' })).toBeVisible();
      await shot('admin-list');
      await page.getByRole('button', { name: /Open the report from bea/ }).click();
      await expect(page.getByText('The message list stays empty after I reconnect')).toBeVisible();
      await shot('admin-detail');
      await page.getByRole('button', { name: 'Show screenshot' }).click();
      await expect(page.getByRole('img', { name: /screenshot/ })).toBeVisible();
      await page.getByRole('img', { name: /screenshot/ }).scrollIntoViewIfNeeded();
      await shot('admin-detail-picture');
      await page.getByRole('button', { name: 'Close report' }).first().click();
      await expect(page.getByRole('dialog', { name: 'Close this report?' })).toBeVisible();
      await shot('admin-close-confirm');
      await page.unrouteAll({ behavior: 'ignoreErrors' });
      await c.close();
    }
  }
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
});
