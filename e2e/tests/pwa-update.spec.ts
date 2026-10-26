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

/**
 * RDY-PWA-001: the service worker's update and offline behaviour, proven in a real browser against two production
 * builds served by `vite preview`.
 *
 * Build A is the real `apps/web/dist` (built here when it is missing). Build B is a copy of A with a different embedded
 * version ("Phase 99"), different hashed file names and a different worker build id, which is what a second release
 * is to the browser; one `vite build` instead of two keeps the spec affordable on a runner. The two are served one
 * after the other on ONE origin (the service worker is per origin): A is installed, then A's server is stopped and
 * B's started, the way a deploy replaces a release.
 *
 * It needs no API, database or login: the pages it uses (/, /login, /help/whats-new) are public. The Lounge is
 * reached for the offline shell only (its route is served by the same cached shell). Timing: the spec waits on
 * service worker state events and polls conditions, never sleeps for a fixed time.
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
const PORT = Number(process.env.SR_PWA_PORT ?? 4175);
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
  const assets = join(b, 'assets');
  const renamed = new Map<string, string>();
  for (const f of readdirSync(assets)) {
    const m = /^(.*-)([A-Za-z0-9_]{8})(\.(?:js|css))$/.exec(f);
    if (!m) continue;
    const hash = m[2]!;
    const next = `${m[1]}${hash.startsWith('Z') ? 'Y' : 'Z'}${hash.slice(1)}${m[3]}`;
    renamed.set(f, next);
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
    writeFileSync(join(assets, to), readFileSync(join(assets, from)));
    rmSync(join(assets, from));
  }
  if (versionHits === 0) throw new Error(`the version "${versionA}" is not in build A's scripts`);
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

/** The page has a worker that controls it. */
async function controlled(page: Page): Promise<void> {
  await page.evaluate(() => navigator.serviceWorker.ready.then(() => undefined));
  if (!(await page.evaluate(() => !!navigator.serviceWorker.controller))) await page.reload();
  await page.waitForFunction(() => !!navigator.serviceWorker.controller);
}

test.describe.configure({ mode: 'serial' });

test.describe('the service worker update and the offline shell (RDY-PWA-001)', () => {
  test.skip(
    ({ browserName }) => browserName !== 'chromium',
    'Chromium only: WebKit does not expose the service worker controls under automation (see the file comment)',
  );

  let tmp = '';
  let dirA = '';
  let dirB = '';
  let versionA = '';

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
    dirB = join(tmp, 'b');
    makeSecondBuild(a, dirB, versionA);
  });

  test.afterAll(async () => {
    await stopServer();
    if (tmp) rmSync(tmp, { recursive: true, force: true });
  });

  test('build A installs, works offline, then build B is offered, accepted and shown, and the upload ledger survives', async ({
    page,
    context,
  }) => {
    test.setTimeout(240_000);
    const buildA = buildId(dirA);
    expect(buildId(dirB)).not.toBe(buildA);

    // ---- 1. Build A: installed and controlling the page.
    await serve(dirA);
    await page.goto(`${ORIGIN}/login`);
    await controlled(page);
    expect(await cacheNames(page)).toEqual([`sr-static-${buildA}`]);
    await expect(page.getByTestId('pwa-host').getByText('A new version is ready')).toHaveCount(0);

    // A member who has seen A's news, and a chat upload waiting in the ledger.
    await page.evaluate(([k, v]) => localStorage.setItem(k!, v!), [SEEN_KEY, versionA]);
    await putLedgerRow(page, 'p26-ledger-row');

    // ---- 2. Offline with A: the cached shell serves the Lounge; with no shell cached the offline page shows.
    // Code is cached the first time it is used (sw.js), so the Lounge's chunks are fetched once online. There is no
    // API here, so the app ends on its signed-out or error screen; what matters is that the shell and its code ran.
    await page.goto(`${ORIGIN}/channels`);
    await page.waitForLoadState('networkidle');
    await context.setOffline(true);
    await page.goto(`${ORIGIN}/channels`);
    await expect(page.locator('#root > *').first()).toBeAttached();
    await expect(page.getByRole('heading', { name: "You're offline" })).toHaveCount(0);
    await expect(page).toHaveTitle(/SIDE ROOM/);
    await page.goto(`${ORIGIN}/login`);
    await expect(page.getByRole('heading', { name: 'Come on in.' })).toBeVisible();
    await context.setOffline(false);

    await page.evaluate(async (name) => {
      const c = await caches.open(name);
      await c.delete('/index.html');
    }, `sr-static-${buildA}`);
    await context.setOffline(true);
    await page.goto(`${ORIGIN}/channels`);
    await expect(page.getByRole('heading', { name: "You're offline" })).toBeVisible();
    await context.setOffline(false);
    // Online again: the next navigation stores the shell again.
    await page.goto(`${ORIGIN}/login`);
    await controlled(page);
    await expect
      .poll(() =>
        page.evaluate(
          async (name) => !!(await (await caches.open(name)).match('/index.html')),
          `sr-static-${buildA}`,
        ),
      )
      .toBe(true);

    // Opening the offline page itself while online must not turn it into the shell (it once was stored as one, and
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
    // The old release keeps running until the member accepts: A's cache is still the only active one.
    expect(await cacheNames(page)).toContain(`sr-static-${buildA}`);

    // ---- 4. Accept: the page reloads under the new worker, which cleans up the old cache.
    const reloaded = page.waitForEvent('load');
    await banner.getByRole('button', { name: 'Reload' }).click();
    await reloaded;
    await controlled(page);
    await expect.poll(() => cacheNames(page)).toEqual([`sr-static-${BUILD_B}`]);
    await expect(page.getByTestId('pwa-host').getByText('A new version is ready')).toHaveCount(0);

    // ---- 5. What's new shows the new version and the member has news they have not seen.
    expect(await page.evaluate((k) => localStorage.getItem(k), SEEN_KEY)).toBe(versionA);
    await page.goto(`${ORIGIN}/help/whats-new`);
    await expect(page.getByRole('heading', { name: new RegExp(VERSION_B) }).first()).toBeVisible();
    await expect
      .poll(() => page.evaluate((k) => localStorage.getItem(k), SEEN_KEY))
      .toBe(VERSION_B);

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
    await page.goto(`${ORIGIN}/channels`);
    await expect(page.getByRole('heading', { name: "You're offline" })).toHaveCount(0);
    await context.setOffline(false);
  });
});
