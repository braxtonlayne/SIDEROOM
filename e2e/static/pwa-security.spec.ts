import { expect, test } from '@playwright/test';

/**
 * XC-PWA / XC-SEC against the production build served by `vite preview`: the manifest, the service worker's
 * boundaries (it must never cache API, gateway or media traffic), offline reload of the shell, and a page
 * load with the enforced CSP that produces no violations.
 */

test('the document carries the enforced security headers', async ({ page }) => {
  const res = await page.goto('/');
  const h = res!.headers();
  expect(h['content-security-policy']).toContain("script-src 'self' 'wasm-unsafe-eval'");
  expect(h['content-security-policy']).toContain("frame-ancestors 'none'");
  expect(h['x-content-type-options']).toBe('nosniff');
  expect(h['referrer-policy']).toBeTruthy();
  expect(h['permissions-policy']).toContain('microphone=(self)');
});

test('entrance pages load under the enforced CSP with no violations', async ({ page }) => {
  const problems: string[] = [];
  page.on('console', (m) => {
    if (/content security policy|refused to/i.test(m.text())) problems.push(m.text());
  });
  page.on('pageerror', (e) => problems.push(e.message));
  for (const route of ['/', '/login', '/no-invitation']) {
    await page.goto(route);
    await page.waitForLoadState('networkidle');
    await expect(page.locator('main')).toBeVisible();
  }
  expect(problems).toEqual([]);
});

test('the manifest is linked, valid and installable', async ({ page, request }) => {
  await page.goto('/');
  const href = await page.locator('link[rel="manifest"]').getAttribute('href');
  expect(href).toBe('/manifest.webmanifest');
  const m = await (await request.get(href!)).json();
  expect(m.name).toBe('SIDE ROOM');
  expect(m.display).toBe('standalone');
  const sizes = m.icons.map((i: { sizes: string }) => i.sizes);
  expect(sizes).toEqual(expect.arrayContaining(['192x192', '512x512']));
  for (const i of m.icons) expect((await request.get(i.src)).ok()).toBe(true);
});

test('the service worker installs, serves the shell offline, and caches nothing from the API', async ({
  page,
  context,
}) => {
  await page.goto('/');
  await page.evaluate(() => navigator.serviceWorker.ready.then(() => undefined));
  // A reload lets the now-active worker control the page and write its shell copy.
  await page.reload();
  await page.waitForFunction(() => !!navigator.serviceWorker.controller);

  // Traffic the worker must not touch: it goes straight to the (here: unavailable) network.
  await page.evaluate(async () => {
    await fetch('/api/v1/users/@me', { credentials: 'include' }).catch(() => undefined);
    await fetch('/cdn/attachments/1/2/a.png').catch(() => undefined);
    await fetch('/gateway').catch(() => undefined);
    await fetch('/api/v1/sr/auth/login', { method: 'POST', body: '{}' }).catch(() => undefined);
  });
  const cached = await page.evaluate(async () => {
    const urls: string[] = [];
    for (const name of await caches.keys()) {
      const c = await caches.open(name);
      for (const r of await c.keys()) urls.push(new URL(r.url).pathname);
    }
    return urls;
  });
  expect(cached.length).toBeGreaterThan(3);
  expect(cached).toContain('/index.html');
  expect(cached).toContain('/offline.html');
  for (const p of cached) expect(p).not.toMatch(/^\/(api|cdn|gateway|internal|oauth)/);

  await context.setOffline(true);
  await page.goto('/login');
  await expect(page.getByRole('heading', { name: 'Come on in.' })).toBeVisible();
  await context.setOffline(false);
});

test('a worker update is offered, not forced', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(() => navigator.serviceWorker.ready.then(() => undefined));
  const src = await page.evaluate(async () => (await fetch('/sw.js')).text());
  // The built worker has no auto-skipWaiting on install, only on the page's SKIP_WAITING message.
  expect(src).not.toMatch(/install[\s\S]{0,400}skipWaiting\(\)/);
  expect(src).toContain('SKIP_WAITING');
  expect(src).not.toContain('__SR_BUILD__');
});
