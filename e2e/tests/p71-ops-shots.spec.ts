import { spawnSync } from 'node:child_process';
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
 * SEC-AUD-266 and 267 (wave 71, D-647 and D-650): Admin > Bug reports on the real stack.
 *
 * - The list pages: 62 reports are written straight into the scratch database (the limiter allows an account five an
 *   hour, so the API cannot make them), the page shows the newest 50, "Load more" appends the other 12, and the status
 *   filter starts the list again.
 * - A report with parts missing opens: one stored in the old shape (seeded: only commit, route and time, as the
 *   server stored a body of a description alone before SEC-AUD-266), one posted by hand through the API with only
 *   `{"description": "x"}` (stored by the new server with every key), and a normal one made through the dialog.
 *
 * Editorial, Lounge Light and Lounge Dark, desktop 1280x800 and phone 375x812. Local only (SR_P71_OPS=1); shots go to
 * SR_P71_OPS_OUT. Needs DATABASE_URL (the scratch database the stack runs on) and a `psql` (SR_PSQL, default the
 * Windows install).
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P71_OPS_OUT ?? resolve('test-results/p71-ops');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const PSQL = process.env.SR_PSQL ?? 'C:/Program Files/PostgreSQL/17/bin/psql.exe';

test.skip(process.env.SR_P71_OPS !== '1', 'local visual check only');
test.setTimeout(1_800_000);

/** Run SQL on the scratch database. The password goes through the environment of the child, never an argument. */
function sql(text: string): string {
  const url = new URL(process.env.DATABASE_URL ?? process.env.SR_DATABASE__URL ?? '');
  const r = spawnSync(
    PSQL,
    [
      '-h',
      url.hostname,
      '-p',
      url.port || '5432',
      '-U',
      url.username,
      '-d',
      url.pathname.slice(1),
      '-tA',
      '-c',
      text,
    ],
    { env: { ...process.env, PGPASSWORD: decodeURIComponent(url.password) }, encoding: 'utf8' },
  );
  if (r.status !== 0)
    throw new Error(`psql failed: ${r.stderr.replace(/postgres:\/\/\S+/g, '<url>')}`);
  return r.stdout.trim();
}

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
) {
  const phone = size.width < 500;
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    ...(phone ? { isMobile: true, hasTouch: true } : {}),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100 },
  );
  return c;
}

const FULL_BUNDLE = `jsonb_build_object(
  'commit', '0a1b2c3d4e5f6a7b', 'route', r.route, 'at', (extract(epoch FROM now()) * 1000)::bigint,
  'viewport', jsonb_build_object('w', 1280, 'h', 800, 'dpr', 2),
  'caps', jsonb_build_object('engine', 'chromium', 'wasm', true, 'webgpu', false),
  'locale', 'en', 'theme', 'lounge-dark', 'online', true,
  'gateway', jsonb_build_object('status', 'connected', 'reconnects', 1, 'heartbeat_ms', 42),
  'sw', 'active', 'voice', jsonb_build_object('connected', false),
  'failed_requests', jsonb_build_array(jsonb_build_object('method', '?', 'path', '/api/v1/channels/:id/messages', 'status', 500, 'ms', 312, 'ago_s', 14)),
  'crashes', jsonb_build_array())`;

test('Admin > Bug reports: the list pages, and a report with parts missing opens', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  // The beta reporter is a platform switch, off by default.
  await apiPatch(o, '/admin/features/platform.beta_reports', { enabled: true });

  const rctx = await browser.newContext({ baseURL: ORIGIN });
  const dora = await registerUser(rctx.request, 'dora');
  await rctx.close();
  const ownerId = sql(`SELECT id FROM accounts WHERE username = '${owner.username}'`);
  const doraId = sql(`SELECT id FROM accounts WHERE id = ${dora.id}`);
  expect(ownerId).not.toBe('');

  // 1. Seed: the oldest report in the old shape (no viewport, gateway, voice, online, sw), then 60 full ones.
  sql(`INSERT INTO bug_reports (reporter_id, commit_sha, route, description, bundle, created_at)
       VALUES (${doraId}, '0a1b2c3d4e5f6a7b', 'unknown', 'Stored before the fix: only a description was sent.',
               jsonb_build_object('commit', '0a1b2c3d4e5f6a7b', 'route', 'unknown',
                                  'at', (extract(epoch FROM now()) * 1000)::bigint,
                                  'caps', jsonb_build_object('engine', 'other'), 'locale', 'und', 'theme', 'other',
                                  'failed_requests', jsonb_build_array(), 'crashes', jsonb_build_array()),
               now() - interval '3 days')`);
  sql(`INSERT INTO bug_reports (reporter_id, commit_sha, route, description, bundle, created_at)
       SELECT CASE WHEN n % 3 = 0 THEN ${doraId} ELSE ${ownerId} END, '0a1b2c3d4e5f6a7b', r.route,
              'Seeded report number ' || n || ': the list stays empty.', ${FULL_BUNDLE},
              now() - (interval '2 days') + n * interval '30 minutes'
         FROM generate_series(1, 60) AS n,
              LATERAL (SELECT (ARRAY['lounge.:guildId.:channelId', 'settings.account', 'library', 'cinema', 'arcade'])[1 + n % 5] AS route) AS r`);

  // 2. A report posted by hand with only a description, as any member can: the new server stores every key.
  const minimal = await apiPost<{ id: string }>(o, '/bug-reports', { description: 'x' });

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const phone = vp === 'phone';
      const c = await context(browser, octx, size, theme);
      const page: Page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const shot = async (name: string, full = false) => {
        await page.waitForTimeout(400);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`), fullPage: full });
      };
      const rowsOf = () => page.getByRole('table', { name: 'Bug reports' }).locator('tbody tr');

      // 3. The report made through the dialog (once on a desktop, once on a phone; the limiter allows five an hour).
      if (theme === 'lounge-editorial') {
        await page.goto('/channels/@me');
        if (phone) {
          await page
            .getByRole('navigation', { name: 'Primary' })
            .getByRole('button', { name: 'You' })
            .click();
          await page.getByRole('button', { name: 'Report a bug' }).click();
        } else {
          await page.getByRole('button', { name: 'Help' }).last().click();
          await page.getByRole('menuitem', { name: 'Report a bug' }).click();
        }
        const dialog = page.getByRole('dialog', { name: 'Report a bug' });
        await dialog
          .getByRole('textbox')
          .first()
          .fill(`Made through the dialog on a ${vp}: the send button does nothing.`);
        await dialog.getByRole('button', { name: 'Send' }).click();
        await expect(page.getByText(/Report #\d+ was sent/)).toBeVisible();
        await shot('dialog-sent');
      }

      // 4. The list: the newest 50, "Load more", then all of them; the filter starts again.
      await page.goto('/admin/bug-reports');
      await expect(page.getByRole('table', { name: 'Bug reports' })).toBeVisible();
      await expect(rowsOf()).toHaveCount(50);
      const more = page.getByRole('button', { name: 'Load more' });
      await expect(more).toBeVisible();
      await shot('list-first-page', true);
      const total = Number(sql(`SELECT count(*) FROM bug_reports WHERE status = 'open'`));
      await more.click();
      await expect(rowsOf()).toHaveCount(total);
      await expect(more).toHaveCount(0);
      await shot('list-after-load-more', true);
      await page.getByRole('combobox', { name: 'Show reports that are' }).click();
      await page.getByRole('option', { name: 'Closed' }).click();
      await expect(page.getByText('No reports')).toBeVisible();
      await shot('list-closed-filter');

      // 5. The three reports with parts missing or made by the dialog, opened from the list. The list is newest first,
      // so a report's place is the number of reports with a larger id.
      const place = (id: string) =>
        Number(sql(`SELECT count(*) FROM bug_reports WHERE id > ${id}`));
      for (const [name, id] of [
        ['detail-minimal-api', minimal.id],
        ['detail-old-shape', sql('SELECT min(id) FROM bug_reports')],
        ['detail-dialog', sql('SELECT max(id) FROM bug_reports')],
      ] as const) {
        await page.goto('/admin/bug-reports');
        await expect(page.getByRole('table', { name: 'Bug reports' })).toBeVisible();
        const at = place(id);
        // The oldest is on the second page; the others are on the first.
        if (at >= 50) await page.getByRole('button', { name: 'Load more' }).click();
        const row = rowsOf().nth(at);
        await expect(row).toBeVisible();
        await row.getByRole('button', { name: /Open the report/ }).click();
        await expect(page.getByRole('button', { name: 'Close report' })).toBeVisible();
        await expect(page.locator('#adm-bug-debug')).toBeVisible();
        await shot(name, true);
      }
      await c.close();
    }
  }
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
});
