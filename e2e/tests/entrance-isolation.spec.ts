import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { APIRequestContext, Page } from '@playwright/test';
import { apiPost, expect, loginApi, test, type SeededUser } from '../support/fixtures';
import { PUBLIC_URL } from '../support/cli';

/**
 * SEC-ENT-004 (D-385; docs/dev/entrance-isolation.md section 8): the browser-side proof of entrance isolation.
 * It iterates `public-surfaces.json`, so a surface added later is tested without a new spec.
 *
 *  - Anonymous, per surface: every request the page makes stays inside that surface's own files (nothing under
 *    /app/, no app.html, no .wasm, no hashed file outside /entrance/ and /fonts/, no other surface's entry) and
 *    the shell and the scripts it loads contain none of the smoke words of the denylist. Plain-HTML surfaces
 *    load no script at all.
 *  - The gate: the status table of section 3, identical for files that exist and files that do not.
 *  - Signed in: the move to /app is a document load, /app/* is private and varies on the cookie, logout sends
 *    Clear-Site-Data and leaves no app cache, a reload of /channels is a 302.
 *  - Revoked elsewhere: the next fetch under /app/ is a 401 and the app caches are purged.
 *
 * It needs the built `apps/web/dist` served by sr-server (`SR_E2E_WEB=server`, which sets SR_WEB__ROOT and
 * SR_WEB__SERVE_PUBLIC=true) or the installed Caddy stack (`SR_E2E_ISOLATION=1`, prod-smoke.yml). The Vite dev
 * server serves no gate, so the spec skips itself there.
 */
test.skip(
  process.env.SR_E2E_WEB !== 'server' && process.env.SR_E2E_ISOLATION !== '1',
  'Runs against the built dist served by sr-server (SR_E2E_WEB=server) or through Caddy (SR_E2E_ISOLATION=1).',
);

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');

interface Surface {
  surface: string;
  kind: 'bundle' | 'html';
  shell?: string;
  paths: string[];
  samples?: string[];
  reserved?: boolean;
}

function loadSurfaces(): Surface[] {
  for (const f of [
    resolve(repo, 'apps/web/public-surfaces.json'),
    resolve(repo, 'apps/web/dist/surfaces.json'),
  ]) {
    if (existsSync(f))
      return (JSON.parse(readFileSync(f, 'utf8')) as { surfaces: Surface[] }).surfaces;
  }
  throw new Error('no public-surfaces.json: build the web app first');
}

/** A smoke copy of the denylist: the three section names of O-7 first, then others, from the hand-kept file. */
function smokeWords(): string[] {
  const j = JSON.parse(readFileSync(resolve(repo, 'scripts/entrance-denylist.json'), 'utf8')) as {
    words: { token: string }[];
  };
  const tokens = j.words.map((w) => w.token).filter((t) => /^[A-Za-z]/.test(t));
  return tokens.slice(0, 12);
}

const SURFACES = loadSurfaces().filter((s) => !s.reserved);
const WORDS = smokeWords();
const NAV = { Accept: 'text/html,application/xhtml+xml', 'Sec-Fetch-Mode': 'navigate' };
const noRedirect = { maxRedirects: 0 };

const containsWord = (text: string, word: string) =>
  new RegExp(
    `(?<![A-Za-z0-9_])${word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![A-Za-z0-9_])`,
    'i',
  ).test(text);

/** Script URLs a shell references, as paths. */
const shellScripts = (html: string) =>
  [...html.matchAll(/<script\b[^>]*\ssrc="([^"]+)"/gi)].map(
    (m) => new URL(m[1]!, 'http://x').pathname,
  );

async function entryScripts(request: APIRequestContext, shell: string): Promise<string[]> {
  const res = await request.get(shell === 'index.html' ? '/' : `/${shell}`, {
    headers: NAV,
    ...noRedirect,
  });
  return res.ok() ? shellScripts(await res.text()) : [];
}

for (const s of SURFACES) {
  const samples =
    s.samples && s.samples.length ? s.samples : s.paths.filter((p) => !p.includes('*'));

  test.describe(`anonymous: ${s.surface}`, () => {
    for (const path of samples.filter((p) => !/\.(md|txt)$/.test(p))) {
      test(`${path} loads only its own files`, async ({ page, request }) => {
        const seen: { url: URL; type: string }[] = [];
        const texts: { path: string; text: string }[] = [];
        page.on('request', (r) => seen.push({ url: new URL(r.url()), type: r.resourceType() }));
        page.on('response', async (r) => {
          const u = new URL(r.url());
          if (u.origin !== new URL(PUBLIC_URL).origin) return;
          if (!/\.(js|mjs|css)$/.test(u.pathname)) return;
          texts.push({ path: u.pathname, text: await r.text().catch(() => '') });
        });
        const res = await page.goto(path, { waitUntil: 'networkidle' });
        expect(res?.status(), `${path} is served`).toBe(200);
        const html = (await res?.text()) ?? '';
        const mine = new URL(page.url()).pathname;
        expect(mine, 'an anonymous visitor stays on a public page').not.toMatch(/^\/app(\/|$)/);

        const paths = seen.map((x) => x.url.pathname);
        expect(paths.filter((p) => p.startsWith('/app/') || p === '/app.html')).toEqual([]);
        expect(paths.filter((p) => p.endsWith('.wasm'))).toEqual([]);
        expect(paths.filter((p) => p.startsWith('/assets/'))).toEqual([]);
        const hashed = paths.filter(
          (p) =>
            /\.(js|mjs|css|woff2?)$/.test(p) &&
            !p.startsWith('/entrance/') &&
            !p.startsWith('/fonts/') &&
            p !== '/sw.js' &&
            p !== '/offline.css',
        );
        expect(hashed, 'a script, style or font from outside /entrance/ and /fonts/').toEqual([]);

        if (s.kind === 'html') {
          expect(
            seen.filter((x) => x.type === 'script'),
            'plain HTML loads no script',
          ).toEqual([]);
          expect(html).not.toMatch(/<script\b/i);
          expect(html).not.toMatch(/<[a-z][^>]*\son[a-z]+\s*=/i);
        } else {
          // The scripts it loaded are its own: not another surface's entry.
          const loaded = paths.filter((p) => p.endsWith('.js') && p.startsWith('/entrance/'));
          for (const other of SURFACES.filter(
            (o) => o.kind === 'bundle' && o.shell && o.surface !== s.surface,
          )) {
            const theirs = await entryScripts(request, other.shell!);
            const clash = loaded.filter((p) => theirs.includes(p));
            expect(clash, `${s.surface} loaded the ${other.surface} entry`).toEqual([]);
          }
        }

        // A smoke copy of the denylist over the document and what it loaded.
        const bodies = [{ path: mine, text: html }, ...texts];
        for (const word of WORDS) {
          for (const b of bodies) {
            expect(containsWord(b.text, word), `"${word}" in ${b.path}`).toBe(false);
          }
        }
      });
    }
  });
}

test.describe('the gate, anonymous', () => {
  const pathsToLogin = [
    '/channels',
    '/admin',
    '/help',
    '/nonexistent-xyz',
    '/app',
    '/app/',
    '/settings',
  ];
  for (const path of pathsToLogin) {
    test(`${path} is a 302 to /login with next`, async ({ request }) => {
      const res = await request.get(path, { headers: NAV, ...noRedirect });
      expect(res.status()).toBe(302);
      const loc = res.headers()['location'] ?? '';
      expect(loc).toMatch(/^\/login(\?next=|$)/);
      expect(res.headers()['cache-control'] ?? '').toContain('no-store');
      expect((await res.body()).length).toBe(0);
    });
  }

  test('a hostile next is not echoed', async ({ request }) => {
    for (const bad of ['//evil.example', '/%2F%2Fevil.example', `/${'a'.repeat(3000)}`]) {
      // Absolute, so that '//evil.example' is a path on this host and not a protocol-relative URL.
      const res = await request.get(`${PUBLIC_URL}${bad}`, { headers: NAV, ...noRedirect });
      expect(res.status()).toBe(302);
      const loc = res.headers()['location'] ?? '';
      expect(loc).not.toMatch(/evil\.example/);
      expect(loc.length).toBeLessThan(2100);
    }
  });

  test('/app/* is a 401 with an empty body, the same for files that do not exist', async ({
    request,
  }) => {
    const a = await request.get('/app/assets/anything.js', noRedirect);
    const b = await request.get('/app/assets/another-nonexistent-0123.js', noRedirect);
    for (const r of [a, b]) {
      expect(r.status()).toBe(401);
      expect((await r.body()).length).toBe(0);
      expect(r.headers()['www-authenticate']).toBeUndefined();
      expect(r.headers()['cache-control'] ?? '').toContain('no-store');
    }
    expect(Object.keys(a.headers()).sort()).toEqual(Object.keys(b.headers()).sort());
    for (const path of [
      '/app.html',
      '/app/manifest.webmanifest',
      '/app/THIRD_PARTY_NOTICES.md',
      '/app/csp.txt',
      '/app/routes.json',
    ]) {
      const r = await request.get(path, noRedirect);
      expect([401, 302], path).toContain(r.status());
      expect((await r.body()).length, path).toBe(0);
    }
  });

  test('the API document and the docs page need a principal; an unknown API path is a 401', async ({
    request,
  }) => {
    for (const path of ['/api/v1/openapi.json', '/developers/docs/api', '/api/v1/nope']) {
      const r = await request.get(path, { headers: NAV, ...noRedirect });
      expect([401, 302], path).toContain(r.status());
      const body = await r.text();
      expect(body, path).not.toContain('"paths"');
    }
  });

  test('other methods on /app are refused', async ({ request }) => {
    const r = await request.post('/app/assets/x.js', {
      ...noRedirect,
      headers: { Origin: new URL(PUBLIC_URL).origin },
    });
    expect([401, 403, 405]).toContain(r.status());
  });

  test('/house-notes is plain HTML under the no-script policy', async ({ page, request }) => {
    const res = await request.get('/house-notes', noRedirect);
    expect(res.status()).toBe(200);
    expect(res.headers()['content-type'] ?? '').toContain('text/html');
    const csp = res.headers()['content-security-policy'] ?? '';
    expect(csp).toContain("default-src 'none'");
    expect(csp).not.toContain('script-src');
    expect(csp).not.toContain('connect-src');
    expect(await res.text()).not.toMatch(/<script\b/i);
    // And with JavaScript off the page still reads.
    const ctx = await page
      .context()
      .browser()!
      .newContext({ baseURL: PUBLIC_URL, javaScriptEnabled: false });
    const p = await ctx.newPage();
    await p.goto('/house-notes');
    await expect(p.locator('body')).not.toBeEmpty();
    await expect(p.getByText(/rules/i).first()).toBeVisible();
    await ctx.close();
  });
});

/**
 * The account the signed-in cases use: a seeded member, or, in the prod-smoke stack (no database reachable from the
 * runner), the stack's Owner, whose password the install stage left in a file (SR_E2E_OWNER_PASSWORD_FILE).
 */
async function account(seed: (label?: string) => Promise<SeededUser>): Promise<SeededUser> {
  const file = process.env.SR_E2E_OWNER_PASSWORD_FILE;
  if (!file) return seed('iso');
  return {
    id: '',
    username: 'owner',
    email: 'owner@example.test',
    password: readFileSync(file, 'utf8').trim(),
  };
}

const contextOptions = { baseURL: PUBLIC_URL, ignoreHTTPSErrors: true };

/** App file names the signed-in run saw, for the anonymous 401 check that follows it. */
const appFiles = new Set<string>();

test.describe.serial('signed in', () => {
  test('sign-in is a document load into /app, /app/* is private, logout purges', async ({
    page,
    context,
    seedUser,
  }) => {
    const user = await account(seedUser);
    const appResponses: { path: string; cc: string; vary: string }[] = [];
    page.on('response', (r) => {
      const u = new URL(r.url());
      if (u.pathname.startsWith('/app/') || u.pathname === '/app') {
        appResponses.push({
          path: u.pathname,
          cc: r.headers()['cache-control'] ?? '',
          vary: r.headers()['vary'] ?? '',
        });
        if (u.pathname.startsWith('/app/') && r.status() === 200) appFiles.add(u.pathname);
      }
    });

    await page.goto('/login');
    await page.evaluate(() => {
      (window as unknown as { __doc: number }).__doc = 1;
    });
    await page.getByLabel('Username or email').fill(user.username);
    await page.getByLabel('Password', { exact: true }).fill(user.password);
    await page.getByRole('button', { name: 'Step inside' }).click();
    await expect(page).toHaveURL(/\/(app|channels)(\/|$)/, { timeout: 30_000 });
    await expect(page.locator('#root')).not.toBeEmpty();
    // The marker set on the login document is gone: the hand-off was a navigation, not a router transition.
    expect(
      await page.evaluate(() => (window as unknown as { __doc?: number }).__doc),
    ).toBeUndefined();

    expect(appResponses.length, 'the app loaded files from /app/').toBeGreaterThan(0);
    for (const r of appResponses) {
      expect(r.cc.toLowerCase(), `${r.path} cache-control`).toContain('private');
      expect(r.vary.toLowerCase(), `${r.path} vary`).toContain('cookie');
    }

    // Logout through the UI: the response clears site data, no app cache stays, /channels is a 302 again.
    const logout = page.waitForResponse(
      (r) =>
        r.request().method() === 'POST' &&
        /\/sr\/auth\/logout(-all)?$/.test(new URL(r.url()).pathname),
    );
    await page.goto('/settings/user/account');
    await page.getByRole('button', { name: 'Log Out' }).first().click();
    await page
      .getByRole('dialog', { name: 'Log out?' })
      .getByRole('button', { name: 'Log Out' })
      .click();
    const lr = await logout;
    expect(lr.headers()['clear-site-data'] ?? '').toMatch(/"cache"/);
    expect(lr.headers()['clear-site-data'] ?? '').toMatch(/"storage"/);
    await expect(page).toHaveURL(/\/login|\/$/, { timeout: 30_000 });
    const left = await page.evaluate(async () =>
      (await caches.keys()).filter((k) => k.startsWith('sr-app-')),
    );
    expect(left, 'app caches after logout').toEqual([]);
    const again = await context.request.get('/channels', { headers: NAV, ...noRedirect });
    expect(again.status()).toBe(302);
    expect(again.headers()['location'] ?? '').toMatch(/^\/login/);
  });

  test('a real app file name, read from the signed-in run, is a 401 without a session', async ({
    request,
  }) => {
    test.skip(appFiles.size === 0, 'the signed-in run above did not record an app file');
    for (const p of [...appFiles].slice(0, 5)) {
      const r = await request.get(p, noRedirect);
      expect(r.status(), p).toBe(401);
      expect((await r.body()).length, p).toBe(0);
    }
  });

  test('a session revoked elsewhere is refused at the next /app fetch and the app caches go', async ({
    browser,
    seedUser,
  }) => {
    const user = await account(seedUser);
    const a = await browser.newContext(contextOptions);
    await loginApi(a.request, user);
    const page: Page = await a.newPage();
    await page.goto('/app');
    await expect(page.locator('#root')).not.toBeEmpty();
    const ok = await page.evaluate(async () => (await fetch('/app/csp.txt')).status);
    expect([200, 404], 'a signed-in fetch under /app/ is answered').toContain(ok);

    // The same user signs in elsewhere and signs every session out.
    const b = await browser.newContext(contextOptions);
    await loginApi(b.request, user);
    await apiPost(b.request, '/sr/auth/logout-all');
    await b.close();

    const status = await page.evaluate(async () => (await fetch('/app/assets/anything.js')).status);
    expect(status).toBe(401);
    await expect
      .poll(
        async () =>
          page.evaluate(async () => (await caches.keys()).filter((k) => k.startsWith('sr-app-'))),
        { timeout: 15_000 },
      )
      .toEqual([]);
    await page.goto('/channels');
    await expect(page).toHaveURL(/\/login/);
    await a.close();
  });
});
