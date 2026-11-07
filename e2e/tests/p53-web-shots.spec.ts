import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPost, bootstrapState, expect, loginApi, registerUser, test } from '../support/fixtures';

/**
 * SEC-AUD-013 / 014 (wave 53): the two admin reason dialogs (staff removal, invite revoke) and the profile and
 * note dialogs in the failed-load state, in Editorial, Lounge Light and Lounge Dark, desktop and phone. Local only
 * (SR_P53_WEB=1); shots go to SR_P53_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P53_OUT ?? resolve('test-results/p53-web');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P53_WEB !== '1', 'local visual check only');
test.setTimeout(900_000);

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

const json = (page: Page, glob: string, body: unknown, status = 200) =>
  page.route(glob, (r) =>
    r.request().method() === 'GET'
      ? r.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) })
      : r.fallback(),
  );

test('admin reason dialogs and failed-load dialogs', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Failed Load Booth' },
  );
  const channelId = guild.channels.find((c) => c.type === 0)!.id;
  const member = await registerUser(
    (await browser.newContext({ baseURL: ORIGIN })).request,
    'notemate',
  );
  const mctx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(mctx.request, member);
  const invite = await apiPost<{ code: string }>(o, `/channels/${channelId}/invites`, {});
  await apiPost(mctx.request, `/invites/${invite.code}`);

  for (const theme of THEMES)
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);

      // Staff removal.
      await json(page, '**/api/v1/admin/staff', [
        {
          account_id: '7',
          username: 'bob',
          display_name: 'Bob',
          appointed_at_ms: 1_700_000_000_000,
          appointed_by: '1',
          is_owner: false,
          platform_permissions: '4096',
          reason: 'new mod',
          roles: [],
        },
      ]);
      await page.goto('/admin/staff');
      await page.getByRole('button', { name: 'Remove bob from staff' }).click();
      let dialog = page.getByRole('dialog');
      await expect(dialog).toBeVisible();
      await dialog.getByRole('textbox').fill('left the team');
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `admin-staff-remove-${tag}.png`) });

      // Invite revoke.
      await json(page, '**/api/v1/admin/invites?**', {
        items: [
          {
            id: '9',
            code: 'ABCDE-FGHJK',
            issuer_account_id: '1',
            state: 'open',
            emergency: false,
            note: 'for a friend',
            created_at_ms: 1_700_000_000_000,
            redeemed_by_account_id: null,
          },
        ],
        next_after: null,
      });
      await page.goto('/admin/invites');
      await page.getByRole('button', { name: 'Revoke invite ABCDE-FGHJK' }).click();
      dialog = page.getByRole('dialog');
      await expect(dialog).toBeVisible();
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `admin-invite-revoke-${tag}.png`) });
      await page.unroute('**/api/v1/admin/staff');
      await page.unroute('**/api/v1/admin/invites?**');

      // Per-server profile, failed load.
      await page.route('**/api/v1/users/@me/guilds/*/member', (r) =>
        r.fulfill({ status: 500, contentType: 'application/json', body: '{"message":"boom"}' }),
      );
      await page.route('**/api/v1/users/@me/notes/*', (r) =>
        r.request().method() === 'GET'
          ? r.fulfill({ status: 500, contentType: 'application/json', body: '{"message":"boom"}' })
          : r.fallback(),
      );
      await page.goto(`/channels/${guild.id}/${channelId}`);
      await page.waitForTimeout(2500);
      const rail = page.getByRole('navigation', { name: 'Servers' });
      await rail.getByRole('link', { name: /Failed Load Booth/ }).click({ button: 'right' });
      await page.getByRole('menuitem', { name: 'Edit Per-server Profile' }).click();
      dialog = page.getByRole('dialog', { name: 'Edit Per-server Profile' });
      await expect(dialog.getByText(/saving is off/)).toBeVisible();
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `profile-load-failed-${tag}.png`) });
      await page.keyboard.press('Escape');

      // Note dialog, failed load.
      const members = page.getByRole('list', { name: 'Members' });
      await members.getByRole('button', { name: /^notemate,/ }).click({ button: 'right' });
      await page.getByRole('menuitem', { name: 'Add Note' }).click();
      dialog = page.getByRole('dialog', { name: /Note for/ });
      await expect(dialog.getByText(/saving is off/)).toBeVisible();
      await page.waitForTimeout(400);
      await page.screenshot({ path: resolve(OUT, `note-load-failed-${tag}.png`) });
      await c.close();
    }
});
