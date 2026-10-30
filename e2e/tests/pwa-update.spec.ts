import { execFileSync, spawn, type ChildProcess } from 'node:child_process';
import {
  cpSync,
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test, type Page } from '@playwright/test';
import { finishServiceWorker } from '../../apps/web/vite/pwa';

/**
 * RDY-PWA-001 and SEC-ENT-002: the service worker's update, purge and offline behaviour, proven in a real browser
 * against two production builds served by `vite preview` (port 4174 by default, never the shared dev ports).
 *
 * Build A is the real `apps/web/dist` (built here when it is missing). Build B is a copy of A with a different embedded
 * version ("Phase 99"), different hashed file names and a different worker build id, which is what a second release
 * is to the browser; one `vite build` instead of two keeps the spec affordable on a runner. The two are served one
 * after the other on ONE origin (the service worker is per origin): A is installed, then A's server is stopped and
 * B's started, the way a deploy replaces a release.
 *
 * Two kinds of page: a public surface (`/login`, `/`, `/join/x`: precached shell, `sr-static-<build>`) and a page of
 * the app (`/channels`: the signed-in shell, `sr-app-<build>`, which exists only after a visit). `vite preview` has
 * no session gate, so the signed-in side is exercised by visiting an app path, which is all the worker sees of a
 * sign-in. When `dist` is still the single-build layout (no `auth.html`), the spec lays out the public shells as the
 * two-build output does (copies of `index.html`) and finishes the worker from its template, so it proves the worker
 * rather than the layout.
 *
 * It needs no API, database or login. Timing: the spec waits on service worker state events and polls conditions,
 * never sleeps for a fixed time.
 *
 * Chromium only. WebKit under Playwright does not expose the page's service worker controls reliably
 * (`navigator.serviceWorker.controller` stays null), and Firefox does not run the worker with an offline context the
 * same way, so a green WebKit or Firefox run would prove nothing. What stays untested here is the iOS side
 * (Add to Home Screen, the standalone window, the update on a cold start of the installed app, the 7-day script
 * eviction of Safari); that is the owner's device pass (docs/dev/handoff/feat-p26-pwa-bots.md).
 */

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../..');
const webDir = join(repo, 'apps/web');
const PORT = Number(process.env.SR_PWA_PORT ?? 4174);
const ORIGIN = `http://127.0.0.1:${PORT}`;
const VERSION_B = 'Phase 99';
const BUILD_B = 'b0b0b0b0b0b0';
const SEEN_KEY = 'sr.whatsNew.seen';

/** The version the build embeds: the first `##` heading of CHANGELOG.md without its date (vite.config.ts). */
function changelogVersion(): string {
  const text = readFileSync(join(repo, 'CHANGELOG.md'), 'utf8');
  const heading = /^## (.+?)\s*$/m.exec(text)?.[1] ?? '';
  return heading.replace(/\s+-\s+\d{4}-\d{2}-\d{2}$/, '') || 'dev';
}

function listFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? listFiles(join(dir, e.name)) : [join(dir, e.name)],
  );
}

/** The worker's build id, read from the worker file of a build. */
function buildId(dir: string): string {
  const m = /const BUILD = '([0-9a-f]+)'/.exec(readFileSync(join(dir, 'sw.js'), 'utf8'));
  if (!m) throw new Error(`no build id in ${dir}/sw.js`);
  return m[1]!;
}

/**
 * Build B from build A: new version text, new hashed names for the scripts and styles (cache-first assets with the
 * same name would otherwise be served from A's cache) and a new worker build id.
 */
function makeSecondBuild(a: string, b: string, versionA: string): void {
  cpSync(a, b, { recursive: true });
  // Hashed output lives in one directory (single build) or in /entrance and /app (two builds).
  const assetDirs = ['assets', 'entrance', 'app']
    .map((d) => join(b, d))
    .filter((d) => existsSync(d) && statSync(d).isDirectory());
  const renamed = new Map<string, string>();
  const dirOf = new Map<string, string>();
  for (const dir of assetDirs) {
    for (const f of readdirSync(dir)) {
      const m = /^(.*-)([A-Za-z0-9_]{8})(\.(?:js|css))$/.exec(f);
      if (!m) continue;
      const hash = m[2]!;
      renamed.set(f, `${m[1]}${hash.startsWith('Z') ? 'Y' : 'Z'}${hash.slice(1)}${m[3]}`);
      dirOf.set(f, dir);
    }
  }
  if (renamed.size < 5) throw new Error(`expected hashed assets, found ${renamed.size}`);
  let versionHits = 0;
  for (const file of listFiles(b)) {
    if (!/\.(js|css|html)$/.test(file)) continue;
    let text = readFileSync(file, 'utf8');
    const before = text;
    for (const [from, to] of renamed) text = text.split(from).join(to);
    if (file.endsWith('.js') && text.includes(versionA)) {
      versionHits += text.split(versionA).length - 1;
      text = text.split(versionA).join(VERSION_B);
    }
    if (file.endsWith('sw.js'))
      text = text.replace(/const BUILD = '[0-9a-f]+'/, `const BUILD = '${BUILD_B}'`);
    if (text !== before) writeFileSync(file, text);
  }
  for (const [from, to] of renamed) {
    const dir = dirOf.get(from)!;
    writeFileSync(join(dir, to), readFileSync(join(dir, from)));
    rmSync(join(dir, from));
  }
  if (versionHits === 0) throw new Error(`the version "${versionA}" is not in build A's scripts`);
}

/**
 * The single-build `dist` has one shell. Lay the public shells out the way the two-build output does and finish the
 * worker from its template, so the worker sees the same surfaces. Returns true when it had to.
 */
function ensureEntranceLayout(dir: string): boolean {
  if (existsSync(join(dir, 'auth.html'))) return false;
  const shell = readFileSync(join(dir, 'index.html'), 'utf8');
  for (const f of ['auth.html', 'invite.html', 'app.html']) writeFileSync(join(dir, f), shell);
  writeFileSync(join(dir, 'sw.js'), readFileSync(join(webDir, 'public/sw.js'), 'utf8'));
  finishServiceWorker(dir, join(webDir, 'public-surfaces.json'));
  return true;
}

let server: ChildProcess | undefined;

async function stopServer(): Promise<void> {
  const s = server;
  server = undefined;
  if (!s || s.exitCode !== null) return;
  await new Promise<void>((done) => {
    s.once('exit', () => done());
    s.kill();
  });
}

/** `vite preview` of one build on the spec's port; resolves when it answers. */
async function serve(outDir: string): Promise<void> {
  await stopServer();
  const vite = join(webDir, 'node_modules/vite/bin/vite.js');
  server = spawn(
    process.execPath,
    [
      vite,
      'preview',
      // The preview gate picks each public surface's shell (auth.html for /login, ...) the way the server does;
      // plain `vite preview` would answer every path with index.html, the landing page.
      '--config',
      join(repo, 'e2e/vite.static.config.ts'),
      '--outDir',
      outDir,
      '--host',
      '127.0.0.1',
      '--port',
      String(PORT),
      '--strictPort',
    ],
    { cwd: webDir, stdio: 'ignore' },
  );
  const deadline = Date.now() + 60_000;
  for (;;) {
    try {
      const res = await fetch(`${ORIGIN}/sw.js`);
      if (res.ok) return;
    } catch {
      // not listening yet
    }
    if (server.exitCode !== null) throw new Error('vite preview exited');
    if (Date.now() > deadline) throw new Error('vite preview did not start');
    await new Promise((r) => setTimeout(r, 200));
  }
}

const cacheNames = (page: Page) => page.evaluate(() => caches.keys());

/** The ledger the app uses (`sideroom-uploads`, version 2, store `chat`): one row with a file in it. */
async function putLedgerRow(page: Page, id: string): Promise<void> {
  await page.evaluate(
    (rowId) =>
      new Promise<void>((resolve, reject) => {
        const open = indexedDB.open('sideroom-uploads', 2);
        open.onupgradeneeded = () => {
          for (const s of ['uploads', 'chat']) {
            if (!open.result.objectStoreNames.contains(s)) {
              open.result.createObjectStore(s, { keyPath: 'id' });
            }
          }
        };
        open.onerror = () => reject(open.error);
        open.onsuccess = () => {
          const db = open.result;
          const tx = db.transaction('chat', 'readwrite');
          tx.objectStore('chat').put({
            id: rowId,
            channelId: '1',
            uploadFilename: 'holiday.png',
            size: 4,
            partSize: 4,
            partCount: 1,
            done: [],
            savedAt: Date.now(),
            file: new Blob(['p26!'], { type: 'image/png' }),
          });
          tx.oncomplete = () => {
            db.close();
            resolve();
          };
          tx.onerror = () => reject(tx.error);
        };
      }),
    id,
  );
}

async function readLedgerRow(
  page: Page,
  id: string,
): Promise<{ id: string; uploadFilename: string; text: string } | null> {
  return page.evaluate(
    (rowId) =>
      new Promise((resolve, reject) => {
        const open = indexedDB.open('sideroom-uploads');
        open.onerror = () => reject(open.error);
        open.onsuccess = () => {
          const db = open.result;
          const get = db.transaction('chat').objectStore('chat').get(rowId);
          get.onsuccess = async () => {
            const row = get.result as
              { id: string; uploadFilename: string; file: Blob } | undefined;
            db.close();
            resolve(
              row
                ? { id: row.id, uploadFilename: row.uploadFilename, text: await row.file.text() }
                : null,
            );
          };
          get.onerror = () => reject(get.error);
        };
      }),
    id,
  );
}

/**
 * The page has a worker that controls it. An app page here has no API behind it, so the app may leave for the
 * sign-in page on its own (its 401 exit) while this runs; a check cut short by that navigation is run again on the
 * page it landed on.
 */
async function controlled(page: Page): Promise<void> {
  for (let attempt = 0; ; attempt++) {
    try {
      await page.waitForLoadState('load');
      await page.evaluate(() => navigator.serviceWorker.ready.then(() => undefined));
      if (!(await page.evaluate(() => !!navigator.serviceWorker.controller))) await page.reload();
      await page.waitForFunction(() => !!navigator.serviceWorker.controller);
      return;
    } catch (e) {
      if (attempt >= 4 || !/context was destroyed|navigat/i.test(String(e))) throw e;
    }
  }
}

// Service worker timing is the risk: no retry in CI either, so a flake is seen (roadmap-p25 section 4, Risks).
test.describe.configure({ mode: 'serial', retries: 0 });

test.describe('the service worker update, the purge and the offline shells (RDY-PWA-001, SEC-ENT-002)', () => {
  test.skip(
    ({ browserName }) => browserName !== 'chromium',
    'Chromium only: WebKit does not expose the service worker controls under automation (see the file comment)',
  );

  let tmp = '';
  let dirA = '';
  let dirB = '';
  let versionA = '';
  let synthesized = false;

  test.beforeAll(async () => {
    test.setTimeout(900_000);
    versionA = changelogVersion();
    dirA = join(webDir, 'dist');
    if (!existsSync(join(dirA, 'sw.js')) || process.env.SR_PWA_REBUILD === '1') {
      execFileSync('pnpm', ['--filter', '@sideroom/web', 'build'], {
        cwd: repo,
        stdio: 'inherit',
        shell: true,
      });
    }
    expect(statSync(join(dirA, 'sw.js')).isFile()).toBe(true);
    // Work on copies: the real dist stays as built.
    tmp = mkdtempSync(join(tmpdir(), 'sr-pwa-'));
    const a = join(tmp, 'a');
    cpSync(dirA, a, { recursive: true });
    dirA = a;
    synthesized = ensureEntranceLayout(a);
    dirB = join(tmp, 'b');
    makeSecondBuild(a, dirB, versionA);
  });

  test.afterAll(async () => {
    await stopServer();
    if (tmp) rmSync(tmp, { recursive: true, force: true });
  });

  const appCaches = (page: Page) =>
    cacheNames(page).then((names) => names.filter((n) => n.startsWith('sr-app-')));
  const cached = (page: Page, cache: string, key: string) =>
    page.evaluate(async ([c, k]) => !!(await (await caches.open(c!)).match(k!)), [
      cache,
      key,
    ] as const);

  test('build A installs, works offline, then build B is offered, accepted and shown; the signed-in cache of A is purged and the upload ledger survives', async ({
    page,
    context,
  }) => {
    test.setTimeout(240_000);
    const buildA = buildId(dirA);
    expect(buildId(dirB)).not.toBe(buildA);

    // ---- 1. Build A: installed and controlling the page; a public page creates no signed-in cache.
    await serve(dirA);
    await page.goto(`${ORIGIN}/login`);
    await controlled(page);
    // Reload once under the worker: the page's lazy code is cached the first time it is used under control.
    await page.reload();
    await page.waitForLoadState('networkidle');
    expect(await cacheNames(page)).toEqual([`sr-static-${buildA}`]);
    await expect(page.getByTestId('pwa-host').getByText('A new version is ready')).toHaveCount(0);

    // A member who has seen A's news, and a chat upload waiting in the ledger.
    await page.evaluate(([k, v]) => localStorage.setItem(k!, v!), [SEEN_KEY, versionA]);
    await putLedgerRow(page, 'p26-ledger-row');

    // ---- 2. Offline with A. A public page is answered from its surface's shell in the static cache.
    await context.setOffline(true);
    await page.goto(`${ORIGIN}/login`);
    await expect(page.getByRole('heading', { name: 'Come on in.' })).toBeVisible();
    await context.setOffline(false);

    // A page of the app is stored as the signed-in shell the first time it is visited online, in its own cache.
    // There is no API here, so the app ends on its signed-out or error screen; what matters is that the shell ran.
    await page.goto(`${ORIGIN}/channels`);
    await page.waitForLoadState('networkidle');
    await expect.poll(() => appCaches(page)).toEqual([`sr-app-${buildA}`]);
    expect(await cached(page, `sr-app-${buildA}`, '/app.html')).toBe(true);
    // ... and it never lands in the static cache, which holds public files only.
    const staticKeys = await page.evaluate(
      async (name) => (await (await caches.open(name)).keys()).map((r) => new URL(r.url).pathname),
      `sr-static-${buildA}`,
    );
    expect(staticKeys.filter((p) => p.startsWith('/app'))).toEqual([]);
    expect(staticKeys).toContain('/offline.html');

    await context.setOffline(true);
    await page.goto(`${ORIGIN}/channels`);
    await expect(page.locator('#root > *').first()).toBeAttached();
    await expect(page.getByRole('heading', { name: "You're offline" })).toHaveCount(0);
    await expect(page).toHaveTitle(/SIDE ROOM/);
    await context.setOffline(false);

    // With no signed-in shell cached, an app page offline shows the offline page.
    await page.evaluate(async (name) => {
      await (await caches.open(name)).delete('/app.html');
    }, `sr-app-${buildA}`);
    await context.setOffline(true);
    await page.goto(`${ORIGIN}/channels`);
    await expect(page.getByRole('heading', { name: "You're offline" })).toBeVisible();
    await context.setOffline(false);
    // Online again: the next navigation stores the shell again.
    await page.goto(`${ORIGIN}/channels`);
    await controlled(page);
    await expect.poll(() => cached(page, `sr-app-${buildA}`, '/app.html')).toBe(true);

    // Opening the offline page itself while online must not turn it into a shell (it once was stored as one, and
    // every offline navigation then showed "You're offline" for as long as the worker lived).
    await page.goto(`${ORIGIN}/offline.html`);
    await expect(page.getByRole('heading', { name: "You're offline" })).toBeVisible();
    await context.setOffline(true);
    await page.goto(`${ORIGIN}/login`);
    await expect(page.getByRole('heading', { name: 'Come on in.' })).toBeVisible();
    await context.setOffline(false);

    // ---- 3. Deploy B: the new worker installs and waits; the page offers it.
    await serve(dirB);
    const waiting = page.evaluate(
      () =>
        new Promise<string>((resolve) => {
          void navigator.serviceWorker.getRegistration().then((reg) => {
            if (!reg) return resolve('no registration');
            const done = () => resolve(reg.waiting ? reg.waiting.state : 'none');
            if (reg.waiting) return done();
            reg.addEventListener('updatefound', () => {
              const next = reg.installing;
              next?.addEventListener('statechange', () => {
                if (next.state === 'installed') done();
              });
            });
            // The same check the app runs when the tab becomes visible again.
            void reg.update();
          });
        }),
    );
    expect(await waiting).toBe('installed');
    const banner = page.getByTestId('pwa-host');
    await expect(banner.getByText('A new version is ready')).toBeVisible();
    // The old release keeps running until the member accepts: A's caches are still there, signed-in one included.
    expect(await cacheNames(page)).toEqual(
      expect.arrayContaining([`sr-static-${buildA}`, `sr-app-${buildA}`]),
    );

    // ---- 4. Accept: the page reloads under the new worker, which cleans up the old caches. Drill 1 (SEC-ENT-002):
    // signed in, the update purges sr-app-A, so no signed-in file of the old build outlives its release.
    const reloaded = page.waitForEvent('load');
    await banner.getByRole('button', { name: 'Reload' }).click();
    await reloaded;
    await controlled(page);
    await expect.poll(() => cacheNames(page)).toEqual([`sr-static-${BUILD_B}`]);
    expect(await appCaches(page)).toEqual([]);
    await expect(page.getByTestId('pwa-host').getByText('A new version is ready')).toHaveCount(0);

    // ---- 5. What's new shows the new version and the member has news they have not seen. In the two-build layout
    // this page is behind the sign-in, so it is proven by the signed-in run (e2e/tests/entrance-isolation.spec.ts).
    expect(await page.evaluate((k) => localStorage.getItem(k), SEEN_KEY)).toBe(versionA);
    if (synthesized) {
      await page.goto(`${ORIGIN}/help/whats-new`);
      await expect(
        page.getByRole('heading', { name: new RegExp(VERSION_B) }).first(),
      ).toBeVisible();
      await expect
        .poll(() => page.evaluate((k) => localStorage.getItem(k), SEEN_KEY))
        .toBe(VERSION_B);
    }

    // ---- 6. The upload row survived the update, file and all.
    expect(await readLedgerRow(page, 'p26-ledger-row')).toEqual({
      id: 'p26-ledger-row',
      uploadFilename: 'holiday.png',
      text: 'p26!',
    });

    // ---- 7. Offline with B.
    await context.setOffline(true);
    await page.goto(`${ORIGIN}/login`);
    await expect(page.getByRole('heading', { name: 'Come on in.' })).toBeVisible();
    // The purge took A's signed-in shell. B has its own only if step 5 visited an app page: otherwise an app page
    // offline is the offline page, never a stale app of the old build.
    await page.goto(`${ORIGIN}/channels`);
    const offlineHeading = page.getByRole('heading', { name: "You're offline" });
    if (synthesized) await expect(offlineHeading).toHaveCount(0);
    else await expect(offlineHeading).toBeVisible();
    await context.setOffline(false);
  });

  test('anonymous: an update on a public page is offered, accepted and never creates a signed-in cache', async ({
    page,
  }) => {
    test.setTimeout(180_000);
    const buildA = buildId(dirA);
    await serve(dirA);
    // Only public surfaces: the landing, an invite-style path, the sign-in page.
    for (const p of ['/', '/join/abc', '/login']) {
      await page.goto(`${ORIGIN}${p}`);
      await controlled(page);
      expect(await appCaches(page), p).toEqual([]);
    }
    expect(await cacheNames(page)).toEqual([`sr-static-${buildA}`]);

    await serve(dirB);
    await page.evaluate(() => navigator.serviceWorker.getRegistration().then((r) => r?.update()));
    const banner = page.getByTestId('pwa-host');
    await expect(banner.getByText('A new version is ready')).toBeVisible();
    // Whatever the second worker did while installing, it made no signed-in cache.
    expect(await appCaches(page)).toEqual([]);

    const reloaded = page.waitForEvent('load');
    await banner.getByRole('button', { name: 'Reload' }).click();
    await reloaded;
    await controlled(page);
    await expect.poll(() => cacheNames(page)).toEqual([`sr-static-${BUILD_B}`]);
    expect(await appCaches(page)).toEqual([]);
  });

  test('sign-out: PURGE_APP deletes the signed-in caches and is answered, and the public ones stay', async ({
    page,
  }) => {
    test.setTimeout(120_000);
    const buildA = buildId(dirA);
    await serve(dirA);
    await page.goto(`${ORIGIN}/login`);
    await controlled(page);
    await page.goto(`${ORIGIN}/channels`);
    await expect.poll(() => appCaches(page)).toEqual([`sr-app-${buildA}`]);
    // A cache from an older build, left behind, goes with it.
    await page.evaluate(() =>
      caches.open('sr-app-0ld').then((c) => c.put('/app/x.js', new Response('x'))),
    );

    // The same exchange src/pwa/purge.ts makes: a message with a reply port, then the reply.
    const reply = await page.evaluate(
      () =>
        new Promise<string>((resolve) => {
          const ch = new MessageChannel();
          ch.port1.onmessage = (e) => resolve((e.data as { type: string }).type);
          navigator.serviceWorker.controller!.postMessage({ type: 'PURGE_APP' }, [ch.port2]);
        }),
    );
    expect(reply).toBe('PURGED');
    expect(await cacheNames(page)).toEqual([`sr-static-${buildA}`]);
  });
});
