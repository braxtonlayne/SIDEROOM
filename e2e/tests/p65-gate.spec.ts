import { existsSync, mkdirSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Browser, BrowserContext } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-170..172 (D-651, D-652): the browser-side proof that staff code is delivered to staff only.
 * Runs against the built dist served by sr-server (SR_E2E_WEB=server), like entrance-isolation.spec.ts.
 *
 *  - A member who opens /admin sees the not-found page and the page never requests anything under /app/staff/;
 *    a direct fetch of a real staff file answers 404.
 *  - The Owner (staff) loads Admin; every /app/staff/ response says no-store and Cache Storage holds none of it.
 *  - SR_P65_SHOTS=1 also writes the visual-check shots (SR_P65_OUT).
 */
test.skip(
  process.env.SR_E2E_WEB !== 'server',
  'Runs against the built dist served by sr-server (SR_E2E_WEB=server).',
);

const here = dirname(fileURLToPath(import.meta.url));
const distStaff = resolve(
  process.env.SR_WEB__ROOT ?? resolve(here, '..', '..', 'apps', 'web', 'dist'),
  'app',
  'staff',
);
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:8080').origin;

function realStaffFile(): string {
  expect(existsSync(distStaff), `${distStaff} exists: build the web app first`).toBe(true);
  const f = readdirSync(distStaff).find((n) => n.endsWith('.js'));
  expect(f, 'a staff chunk in dist/app/staff').toBeTruthy();
  return f!;
}

test('a member opening /admin sees not-found and requests no staff file', async ({
  seedUser,
  signedIn,
}) => {
  const member = await signedIn(await seedUser('gate'));
  const requested: string[] = [];
  member.page.on('request', (r) => requested.push(r.url()));
  await member.page.goto('/admin');
  await expect(member.page.getByRole('heading', { name: 'Page not found' })).toBeVisible();
  await member.page.waitForLoadState('networkidle');
  expect(requested.filter((u) => u.includes('/app/staff/'))).toEqual([]);
  // The door asked first, and was refused.
  expect(requested.some((u) => u.includes('/admin/me'))).toBe(true);

  // A real staff file by URL: the same 404 as a file that is not there.
  const name = realStaffFile();
  const statuses = await member.page.evaluate(async (n) => {
    const a = await fetch(`/app/staff/${n}`);
    const b = await fetch('/app/staff/zzzzzzzz.js');
    return [a.status, b.status, await a.text()];
  }, name);
  expect(statuses[0]).toBe(404);
  expect(statuses[1]).toBe(404);
  expect(statuses[2]).toBe('');
});

test('the Owner loads Admin; staff files are no-store and nothing is cached', async ({
  owner,
  signedIn,
}) => {
  const s = await signedIn(owner);
  const staff: { url: string; status: number; cc: string }[] = [];
  s.page.on('response', (r) => {
    if (r.url().includes('/app/staff/'))
      staff.push({ url: r.url(), status: r.status(), cc: r.headers()['cache-control'] ?? '' });
  });
  await s.page.goto('/admin/access-log');
  await expect(s.page.getByRole('heading', { name: 'Access log' })).toBeVisible();
  expect(staff.length).toBeGreaterThan(0);
  for (const r of staff) {
    expect(r.status, r.url).toBe(200);
    expect(r.cc, r.url).toContain('no-store');
  }
  const cached = await s.page.evaluate(async () => {
    const out: string[] = [];
    for (const k of await caches.keys()) {
      for (const req of await (await caches.open(k)).keys()) out.push(req.url);
    }
    return out;
  });
  expect(cached.filter((u) => u.includes('/app/staff/'))).toEqual([]);
});

// ---- the visual check (local only) -------------------------------------------------------------------------

const OUT = process.env.SR_P65_OUT ?? resolve('test-results/p65-gate');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

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

test('shots: /admin as a member and as the Owner', async ({ browser, seedUser }) => {
  test.skip(process.env.SR_P65_SHOTS !== '1', 'local visual check only');
  test.setTimeout(600_000);
  mkdirSync(OUT, { recursive: true });
  const memberCtx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(memberCtx.request, await seedUser('shots'));
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
      const m = await themed(browser, memberCtx, size, theme);
      const mp = await m.newPage();
      await mp.goto('/admin');
      await expect(mp.getByRole('heading', { name: 'Page not found' })).toBeVisible();
      await mp.waitForTimeout(500);
      await mp.screenshot({ path: resolve(OUT, `member-notfound-${tag}.png`) });
      await m.close();

      const o = await themed(browser, ownerCtx, size, theme);
      const op = await o.newPage();
      await op.goto('/admin');
      await op.waitForLoadState('networkidle');
      await op.waitForTimeout(800);
      await op.screenshot({ path: resolve(OUT, `owner-first-${tag}.png`) });
      await op.goto('/admin/access-log');
      await expect(op.getByRole('heading', { name: 'Access log' })).toBeVisible();
      await op.waitForTimeout(500);
      await op.screenshot({ path: resolve(OUT, `owner-accesslog-${tag}.png`) });
      await o.close();
    }
  }
});
