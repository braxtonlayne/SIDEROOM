import { existsSync, mkdirSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Browser, BrowserContext } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-187 (D-663): the browser-side proof that the staff message catalog is delivered to staff only.
 * Runs against the built dist served by sr-server (SR_E2E_WEB=server), like p65-gate.spec.ts.
 *
 *  - A member session fetches every non-staff file of the built app (the files the member's own page can reach)
 *    and none holds a known Admin section title.
 *  - The Owner (staff) opens /admin and sees real titles, not message keys, and the staff catalog was fetched from
 *    /app/staff/.
 *  - SR_P66_SHOTS=1 also writes the visual-check shots (SR_P66_OUT).
 */
test.skip(
  process.env.SR_E2E_WEB !== 'server',
  'Runs against the built dist served by sr-server (SR_E2E_WEB=server).',
);

const here = dirname(fileURLToPath(import.meta.url));
const distApp = resolve(
  process.env.SR_WEB__ROOT ?? resolve(here, '..', '..', 'apps', 'web', 'dist'),
  'app',
);
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:8080').origin;

/** Texts of Admin section titles that only the staff catalog holds (messages/en-staff/admin-a-1.ts). */
const ADMIN_TITLES = ['Platform bans', 'Staff & roles', 'Cinema transcoding', 'Ingest sources'];

function nonStaffFiles(dir = distApp, base = ''): string[] {
  expect(existsSync(dir), `${dir} exists: build the web app first`).toBe(true);
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const rel = base ? `${base}/${name}` : name;
    if (statSync(join(dir, name)).isDirectory()) {
      if (rel === 'staff') continue;
      out.push(...nonStaffFiles(join(dir, name), rel));
    } else if (/\.(js|css|json|html)$/.test(name)) out.push(rel);
  }
  return out;
}

test('a member fetches every non-staff file of the app and none holds an Admin section title', async ({
  seedUser,
  signedIn,
}) => {
  const files = nonStaffFiles();
  expect(files.length).toBeGreaterThan(10);
  // The built files hold the titles (the staff ones), so the test is not vacuous.
  const staffText = readdirSync(join(distApp, 'staff'))
    .map((f) => readFileSync(join(distApp, 'staff', f), 'utf8'))
    .join('\n');
  for (const t of ADMIN_TITLES) expect(staffText, `${t} is in dist/app/staff`).toContain(t);

  const member = await signedIn(await seedUser('catalog'));
  await member.page.goto('/');
  const leaks = await member.page.evaluate(
    async ([names, titles]) => {
      const bad: string[] = [];
      for (const n of names as string[]) {
        const r = await fetch(`/app/${n}`);
        if (!r.ok) continue;
        const text = await r.text();
        for (const t of titles as string[]) if (text.includes(t)) bad.push(`${n}: ${t}`);
      }
      return bad;
    },
    [files, ADMIN_TITLES],
  );
  expect(leaks).toEqual([]);
});

test('the Owner opens /admin and sees titles, not keys; the staff catalog came from /app/staff/', async ({
  owner,
  signedIn,
}) => {
  const s = await signedIn(owner);
  const staffUrls: string[] = [];
  s.page.on('response', (r) => {
    if (r.url().includes('/app/staff/') && r.ok()) staffUrls.push(r.url());
  });
  await s.page.goto('/admin/access-log');
  await expect(s.page.getByRole('heading', { name: 'Access log' })).toBeVisible();
  await expect(s.page.getByText('Platform bans', { exact: true }).first()).toBeVisible();
  expect(await s.page.locator('body').innerText()).not.toMatch(/\badmin\.[A-Za-z]+\.[A-Za-z.]+/);
  expect(staffUrls.length).toBeGreaterThan(1);
});

test('click through: staff opens Admin and three sections, signs out; a member gets not-found', async ({
  owner,
  seedUser,
  signedIn,
}) => {
  const s = await signedIn(owner);
  await s.page.goto('/admin');
  await expect(s.page.getByText('Platform bans', { exact: true }).first()).toBeVisible();
  for (const [label, path] of [
    ['Platform bans', '/admin/bans'],
    ['Invites', '/admin/invites'],
    ['Jobs', '/admin/jobs'],
  ] as const) {
    await s.page.getByText(label, { exact: true }).first().click();
    await expect(s.page).toHaveURL(new RegExp(`${path}$`));
    expect(await s.page.locator('body').innerText()).not.toMatch(/admin.[A-Za-z]+.[A-Za-z.]+/);
  }
  // Sign out (the session cookie goes), then a member signs in and opens /admin.
  await s.context.clearCookies();
  await s.page.goto('/admin');
  await expect(s.page.getByText('Platform bans', { exact: true })).toHaveCount(0);
  const member = await signedIn(await seedUser('catclick'));
  await member.page.goto('/admin');
  await expect(member.page.getByRole('heading', { name: 'Page not found' })).toBeVisible();
});

// ---- the visual check (local only) -------------------------------------------------------------------------

const OUT = process.env.SR_P66_OUT ?? resolve('test-results/p66-catalog');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const SECTIONS = ['', 'limits', 'features', 'bug-reports', 'review', 'runbook'];

async function themed(
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

test('shots: Admin sections as the Owner, Lounge and Settings as a member', async ({
  browser,
  seedUser,
}) => {
  test.skip(process.env.SR_P66_SHOTS !== '1', 'local visual check only');
  test.setTimeout(900_000);
  mkdirSync(OUT, { recursive: true });
  const memberCtx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(memberCtx.request, await seedUser('catshots'));
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(ownerCtx.request, bootstrapState().owner);
  for (const theme of THEMES) {
    for (const ctx of [memberCtx, ownerCtx]) {
      await apiPatch(ctx.request, '/users/@me/settings', {
        settings: { user_prefs: { theme, zoom: 100 } },
      });
    }
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const o = await themed(browser, ownerCtx, size, theme);
      const op = await o.newPage();
      for (const sec of SECTIONS) {
        await op.goto(`/admin${sec ? `/${sec}` : ''}`);
        await op.waitForLoadState('networkidle');
        await op.waitForTimeout(700);
        await op.screenshot({ path: resolve(OUT, `owner-admin-${sec || 'first'}-${tag}.png`) });
      }
      await o.close();

      const m = await themed(browser, memberCtx, size, theme);
      const mp = await m.newPage();
      for (const [name, path] of [
        ['lounge', '/'],
        ['settings', '/settings'],
      ] as const) {
        await mp.goto(path);
        await mp.waitForLoadState('networkidle');
        await mp.waitForTimeout(700);
        await mp.screenshot({ path: resolve(OUT, `member-${name}-${tag}.png`) });
      }
      await m.close();
    }
  }
});
