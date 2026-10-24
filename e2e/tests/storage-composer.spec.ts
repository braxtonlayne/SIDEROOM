import { execFileSync, execSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import type { BrowserContext, CDPSession, Page } from '@playwright/test';
import { apiPost, expect, loginApi, registerUser, test } from '../support/fixtures';
import { listParts, presignUrl, s3FromEnv, type S3Config } from '../support/s3-sign.mjs';

/**
 * RDY-STO-001 / RDY-STO-002: a chat attachment above the 64 MiB part size, sent through the real composer, against
 * a real S3 (Garage). Three runs of one user:
 *
 *   1. abort   : part 1 reaches S3, the user cancels; the ledger row, the S3 upload and the server slot go.
 *   2. reload  : part 1 reaches S3, the page reloads; the composer reattaches from the IndexedDB ledger and sends
 *                only the rest without a click, then the message goes out.
 *   3. restart : as 2, over a throttled network, and sr-server is restarted while the parts are in flight.
 *
 * Whenever the upload is held still the ledger's confirmed parts must equal S3's ListParts.
 *
 * Needs (the storage.yml `composer` job sets them): SR_E2E_STORAGE_LIVE=1, SR_TEST_S3_* (the Garage the server
 * uses), SR_TEST_DATABASE_URL (psql: the multipart id, and the Patron upload limit), and SR_E2E_RESTART_CMD (a
 * shell command that stops sr-server, starts it again and waits for /readyz). Chromium only (CDP throttling).
 */
const LIVE = process.env.SR_E2E_STORAGE_LIVE === '1';
const MIB = 1024 * 1024;
const PART = 64 * MIB;
const SIZE = 2 * PART + 12 * MIB; // three parts: 64 + 64 + 12 MiB

test.skip(!LIVE, 'Needs Garage and the storage.yml stack; set SR_E2E_STORAGE_LIVE=1.');
test.setTimeout(15 * 60_000);

const psql = (q: string) =>
  execFileSync('psql', [process.env.SR_TEST_DATABASE_URL ?? '', '-X', '-A', '-t', '-c', q], {
    encoding: 'utf8',
  }).trim();

/** The bytes the page makes: byte i is (i * 31 + 7) mod 256. */
function expectedSha(): string {
  const b = Buffer.allocUnsafe(SIZE);
  for (let i = 0; i < SIZE; i++) b[i] = (i * 31 + 7) & 255;
  return createHash('sha256').update(b).digest('hex');
}

/** SHA-256 of the object as S3 holds it. */
async function objectSha(S3: S3Config, key: string): Promise<string> {
  const res = await fetch(presignUrl(S3, 'GET', key));
  expect(res.status).toBe(200);
  return createHash('sha256')
    .update(Buffer.from(await res.arrayBuffer()))
    .digest('hex');
}

interface LedgerRow {
  id: string;
  done: number[];
  partCount: number;
}

/** The chat rows of the browser's own IndexedDB ledger (lib/uploadLedger.ts). */
async function ledger(page: Page): Promise<LedgerRow[]> {
  return page.evaluate(
    () =>
      new Promise<LedgerRow[]>((resolve) => {
        const open = indexedDB.open('sideroom-uploads');
        open.onerror = () => resolve([]);
        open.onsuccess = () => {
          const db = open.result;
          if (!db.objectStoreNames.contains('chat')) return resolve([]);
          const all = db.transaction('chat').objectStore('chat').getAll();
          all.onerror = () => resolve([]);
          all.onsuccess = () =>
            resolve(
              (all.result as LedgerRow[]).map((r) => ({
                id: r.id,
                done: [...r.done],
                partCount: r.partCount,
              })),
            );
        };
      }),
  );
}

/** Hands the composer a generated file through its file input (the same path as the attach button). */
async function attach(page: Page, name: string) {
  await page.evaluate(
    ({ name, size }) => {
      const bytes = new Uint8Array(size);
      for (let i = 0; i < size; i++) bytes[i] = (i * 31 + 7) & 255;
      const file = new File([bytes], name, {
        type: 'application/octet-stream',
        lastModified: 1_700_000_000_000 + name.length,
      });
      const dt = new DataTransfer();
      dt.items.add(file);
      const input = document.querySelector<HTMLInputElement>('[data-testid="file-input"]')!;
      input.files = dt.files;
      input.dispatchEvent(new Event('change', { bubbles: true }));
    },
    { name, size: SIZE },
  );
  await expect(page.locator('.lng-tray-card').filter({ hasText: name })).toBeVisible();
}

test('chat multipart: abort, reload resume and a server restart, ledger equals ListParts', async ({
  browser,
  browserName,
  playwright,
}) => {
  test.skip(browserName !== 'chromium', 'CDP network throttling is Chromium only.');
  const S3 = s3FromEnv();
  const sha = expectedSha();
  const appOrigin = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;

  // The page PUTs the parts straight to the bucket, so the bucket must allow the app origin (D-175; the
  // e2e-stack action runs `sr-cli storage ensure-cors`). Asked from Node: Playwright answers the preflight of a
  // routed request itself, so in the page a missing rule only shows as a part that S3 holds and the composer
  // never counted (the first CI run: "ledger= s3=1").
  const preflight = await fetch(`${S3.endpoint}/${S3.bucket}/cors-probe`, {
    method: 'OPTIONS',
    headers: { Origin: appOrigin, 'Access-Control-Request-Method': 'PUT' },
  });
  expect(
    [appOrigin, '*'],
    `the bucket has a CORS rule for ${appOrigin} (sr-cli storage ensure-cors)`,
  ).toContain(preflight.headers.get('access-control-allow-origin'));

  // --- A user with the 500 MiB Patron upload limit (granted before the first limit lookup), a server, a channel.
  const req = await playwright.request.newContext({
    baseURL: process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173',
  });
  const user = await registerUser(req, 'stor');
  await req.dispose();
  psql(
    `INSERT INTO entitlement_grants (id, kind, user_id, source, permanent, starts_ms, reason) ` +
      `VALUES (floor(random() * 9e15)::bigint, 'patron', ${user.id}, 'admin', true, 0, 'storage-composer')`,
  );
  const context: BrowserContext = await browser.newContext({
    baseURL: process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173',
  });
  await loginApi(context.request, user);
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    context.request,
    '/guilds',
    { name: `Storage ${Date.now().toString(36)}` },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const page = await context.newPage();

  // --- S3 traffic: part PUTs from `hold.from` on are held (never answered); the others may be delayed.
  const hold = { from: Infinity as number, delayMs: 0 };
  const seen: number[] = [];
  await context.route(
    (url) => url.href.startsWith(S3.endpoint),
    async (route) => {
      const r = route.request();
      const n = Number(new URL(r.url()).searchParams.get('partNumber'));
      if (r.method() === 'PUT' && n) {
        seen.push(n);
        if (n >= hold.from) return; // held: the request hangs until the page goes away
        if (hold.delayMs) await new Promise((res) => setTimeout(res, hold.delayMs));
      }
      await route.continue().catch(() => undefined);
    },
  );
  const cdp: CDPSession = await context.newCDPSession(page);

  const open = async () => {
    await page.goto(`/channels/${guild.id}/${channel}`);
    await expect(page.getByRole('heading', { level: 1, name: 'general' })).toBeVisible();
  };
  const send = () => page.getByRole('button', { name: 'Send message' });
  const card = (name: string) => page.locator('.lng-tray-card').filter({ hasText: name });

  /** Waits until the upload is still and its ledger parts equal ListParts; returns what it saw. */
  async function stillAndEqual() {
    let latest = { id: '', key: '', mp: '', ledger: '' };
    await expect
      .poll(
        async () => {
          const r = (await ledger(page))[0];
          if (!r) return 'no ledger row yet';
          const key = psql(`SELECT storage_key FROM attachments WHERE id = ${r.id}`);
          const mp = psql(
            `SELECT multipart_id FROM attachment_multipart WHERE attachment_id = ${r.id}`,
          );
          const s3 = (await listParts(S3, key, mp)) ?? [];
          const l = r.done.join(',');
          const s = s3.map((p) => p.part).join(',');
          latest = { id: r.id, key, mp, ledger: l };
          return l !== '' && l === s ? 'equal' : `ledger=${l} s3=${s}`;
        },
        { timeout: 90_000, intervals: [500, 1000, 2000] },
      )
      .toBe('equal');
    return latest;
  }

  try {
    await open();

    // ===== 1. abort =============================================================================================
    hold.from = 2;
    await attach(page, 'abort.bin');
    await send().click();
    const a = await stillAndEqual();
    expect(a.ledger).toBe('1');
    await page.getByRole('button', { name: 'Cancel upload of abort.bin' }).click();
    await expect.poll(async () => (await ledger(page)).length, { timeout: 20_000 }).toBe(0);
    await expect.poll(async () => await listParts(S3, a.key, a.mp), { timeout: 20_000 }).toBeNull(); // S3 forgot the aborted upload
    await expect
      .poll(
        async () =>
          (
            await context.request.get(`/api/v1/channels/${channel}/attachments/${a.id}/parts`)
          ).status(),
        { timeout: 20_000 },
      )
      .toBe(404);
    await expect(page.locator('.lng-tray-card')).toHaveCount(0);

    // ===== 2. reload and resume from the ledger =================================================================
    hold.from = 2;
    await attach(page, 'resume.bin');
    await send().click();
    const b = await stillAndEqual();
    expect(b.ledger).toBe('1');
    // The reload kills the held requests; the ledger row (with the file inside) survives it.
    hold.from = Infinity;
    seen.length = 0;
    await page.reload();
    await expect(page.getByRole('heading', { level: 1, name: 'general' })).toBeVisible();
    await expect(card('resume.bin')).toBeVisible(); // reattached without choosing the file again
    await expect(card('resume.bin')).toHaveAttribute('data-status', 'done', { timeout: 120_000 });
    expect([...seen].sort()).toEqual([2, 3]); // only the missing parts were sent
    await expect.poll(async () => (await ledger(page)).length).toBe(0);
    expect(await listParts(S3, b.key, b.mp)).toBeNull(); // completed: no open upload is left
    expect(await objectSha(S3, b.key)).toBe(sha);
    await send().click();
    await expect(page.locator('.lng-tray-card')).toHaveCount(0);
    await expect(page.getByText('resume.bin').first()).toBeVisible();

    // ===== 3. a slow network and a server restart in the middle =================================================
    hold.from = 2;
    await attach(page, 'restart.bin');
    await send().click();
    const c = await stillAndEqual();
    expect(c.ledger).toBe('1');
    hold.from = Infinity;
    hold.delayMs = 4000; // every part waits, on top of the bandwidth limit below
    seen.length = 0;
    await cdp.send('Network.enable');
    // Only the upload side is slowed: the 4 s hold per part above plus the bandwidth cap. Added latency also
    // applied to every module the Vite dev server serves on reload, and the page did not come back within the
    // wait (the CI run after the CORS fix).
    await cdp.send('Network.emulateNetworkConditions', {
      offline: false,
      latency: 0,
      downloadThroughput: -1,
      uploadThroughput: 3 * MIB,
    });
    await page.reload();
    await expect(page.getByRole('heading', { level: 1, name: 'general' })).toBeVisible({
      timeout: 60_000,
    });
    await expect(card('restart.bin')).toBeVisible();
    await expect.poll(() => seen.length, { timeout: 60_000 }).toBeGreaterThan(0);
    // sr-server goes away and comes back while the parts still travel to S3, before the upload is completed.
    execSync(process.env.SR_E2E_RESTART_CMD ?? 'false', { stdio: 'inherit', timeout: 120_000 });
    await expect(card('restart.bin')).toHaveAttribute('data-status', 'done', { timeout: 240_000 });
    expect(await listParts(S3, c.key, c.mp)).toBeNull();
    expect(await objectSha(S3, c.key)).toBe(sha);
    await cdp.send('Network.emulateNetworkConditions', {
      offline: false,
      latency: 0,
      downloadThroughput: -1,
      uploadThroughput: -1,
    });
    await send().click();
    await expect(page.getByText('restart.bin').first()).toBeVisible();
  } finally {
    await context.unrouteAll({ behavior: 'ignoreErrors' });
    await context.close();
  }
});
