import { expect, test } from '@playwright/test';

/**
 * XC-PWA / XC-SEC / SEC-ENT-002 against the production build served by `vite preview`: the manifests, the service
 * worker's boundaries (it must never cache API, gateway or media traffic, nor name a section of the app), offline
 * reload of a public shell, and a page load with the enforced CSP that produces no violations.
 */

/** Words that must appear in no public file: the app's sections and the endpoints behind the sign-in. */
const SECTION_WORDS =
  /Lounge|Cinema|Library|Soundboard|Activities|jukebox|\/channels|\/cinema|\/library|\/gateway|\/cdn|\/voice|\/developers|\/help|\/api\/|\/internal/i;

test('the document carries the enforced security headers', async ({ page }) => {
  const res = await page.goto('/');
  const h = res!.headers();
  expect(h['content-security-policy']).toContain("script-src 'self'");
  expect(h['content-security-policy']).toContain("frame-ancestors 'none'");
  expect(h['x-content-type-options']).toBe('nosniff');
  expect(h['referrer-policy']).toBeTruthy();
  expect(h['permissions-policy']).toBeTruthy();
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

test('the public manifest is linked, valid, installable, opens the neutral door and names no section', async ({
  page,
  request,
}) => {
  await page.goto('/');
  const href = await page.locator('link[rel="manifest"]').getAttribute('href');
  expect(href).toBe('/manifest.webmanifest');
  const res = await request.get(href!);
  const text = await res.text();
  const m = JSON.parse(text);
  expect(m.name).toBe('SIDE ROOM');
  expect(m.display).toBe('standalone');
  expect(m.start_url).toBe('/app');
  expect(m.shortcuts).toBeUndefined();
  expect(text).not.toMatch(SECTION_WORDS);
  const sizes = m.icons.map((i: { sizes: string }) => i.sizes);
  expect(sizes).toEqual(expect.arrayContaining(['192x192', '512x512']));
  for (const i of m.icons) expect((await request.get(i.src)).ok()).toBe(true);
});

test('the signed-in manifest is not served from the public root', async ({ request }) => {
  const res = await request.get('/app-manifest.webmanifest');
  // The preview server falls back to the shell for an unknown path; what matters is that it is not the manifest.
  expect(res.headers()['content-type'] ?? '').not.toContain('json');
  expect(await res.text()).not.toContain('shortcuts');
});

test('the worker, the offline page and the error pages name no section and carry no script', async ({
  request,
}) => {
  const sw = await (await request.get('/sw.js')).text();
  expect(sw).not.toMatch(SECTION_WORDS);
  expect(sw).not.toContain('__SR_BUILD__');
  expect(sw).not.toContain('/*__SR_SURFACES__*/');
  for (const f of ['/offline.html', '/404.html', '/50x.html', '/offline.css']) {
    const res = await request.get(f);
    expect(res.ok(), f).toBe(true);
    const body = await res.text();
    expect(body, f).not.toMatch(SECTION_WORDS);
    expect(body, f).not.toMatch(/<script|\son[a-z]+=/i);
  }
});

test('the service worker installs, serves a public shell offline, and caches nothing from the API', async ({
  page,
  context,
}) => {
  await page.goto('/');
  await page.evaluate(() => navigator.serviceWorker.ready.then(() => undefined));
  // A reload lets the now-active worker control the page and write its shell copy.
  await page.reload();
  await page.waitForFunction(() => !!navigator.serviceWorker.controller);
  // A second public page, online, so its shell is stored whichever surface it belongs to.
  await page.goto('/login');

  // Traffic the worker must not touch: it goes straight to the (here: unavailable) network.
  await page.evaluate(async () => {
    await fetch('/api/v1/users/@me', { credentials: 'include' }).catch(() => undefined);
    await fetch('/cdn/attachments/1/2/a.png').catch(() => undefined);
    await fetch('/gateway').catch(() => undefined);
    await fetch('/api/v1/sr/auth/login', { method: 'POST', body: '{}' }).catch(() => undefined);
  });
  const stored = await page.evaluate(async () => {
    const urls: string[] = [];
    const names = await caches.keys();
    for (const name of names) {
      const c = await caches.open(name);
      for (const r of await c.keys()) urls.push(name.split('-')[1] + ':' + new URL(r.url).pathname);
    }
    return { urls, names };
  });
  expect(stored.urls.length).toBeGreaterThan(3);
  expect(stored.urls).toContain('static:/index.html');
  expect(stored.urls).toContain('static:/offline.html');
  expect(stored.urls).toContain('static:/404.html');
  for (const p of stored.urls) expect(p).not.toMatch(/^[a-z]+:\/(api|cdn|gateway|internal|oauth)/);
  // An anonymous visitor has no signed-in cache, and nothing signed-in sits in the public one.
  expect(stored.names.filter((n) => n.startsWith('sr-app-'))).toEqual([]);
  expect(stored.urls.filter((p) => p.startsWith('static:/app'))).toEqual([]);

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
  expect(src).toContain('PURGE_APP');
  expect(src).not.toContain('__SR_BUILD__');
});
