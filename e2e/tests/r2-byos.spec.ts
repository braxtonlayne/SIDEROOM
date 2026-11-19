import { randomBytes } from 'node:crypto';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
import type { APIRequestContext, Page } from '@playwright/test';
import { apiPost, expect, test } from '../support/fixtures';

/**
 * D-1029: the owner's real-bucket test of "bring your own storage" (BYOS) against a Cloudflare R2 bucket.
 *
 * THIS SPEC HOLDS NO CREDENTIAL AND MUST NEVER BE GIVEN ONE. It has no field, no environment variable and no
 * fixture for a key, a secret or a token. It never opens the "connect a bucket" form, never reads a database,
 * never takes a screenshot (screenshots, video and the trace are switched off below, because a trace would
 * keep the mounts list answer). It logs only: step results, counts, status numbers, and whether a header is
 * present. It never prints a URL (a presigned URL carries the access key id and the account's host).
 *
 * What it needs before it runs (docs/dev/r2-byos-test.md, the runbook, has the steps):
 *   1. A stack of the merged tree on its own port and database, started by the commander, with the e2e Owner
 *      made by Playwright's global setup (one run of any spec with this one's switch off).
 *   2. The OWNER has, by hand, in their own browser: made an R2 API token scoped to one bucket, set the
 *      bucket's CORS for the origin below, and connected the bucket to the e2e Owner's account with the label
 *      `sideroom-test` and the prefix `sideroom-test/` (User settings > Storage), and the mount shows as
 *      connected with CORS passed.
 *   Nothing else. The spec finds that mount by its label; if it is missing, or its prefix is not exactly
 *   `sideroom-test/`, it stops at once with one line and touches nothing.
 *
 * Command (from the e2e folder of the merged tree; the environment is the one a shoot job sets: SR_E2E_WEB=server,
 * SR_E2E_SERVER_PORT, SR_E2E_BASE_URL (the origin the owner allowed in the bucket's CORS), SR_CLI_BIN and the
 * database URLs of the stack):
 *
 *   SR_R2_ON=1 npx playwright test tests/r2-byos.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 *
 * Reading the result: every step prints `R2 step <n> <name>: ok` or `R2 step <n> <name>: FAIL <what>` and the
 * run goes on, so one failure does not hide the next. The LAST TWO `R2 RESULT` lines say how many objects of
 * this run are left in the prefix (0 is the pass) and how many objects the prefix holds in all.
 *
 * What it never does: delete by pattern, delete an object it did not make in this run, touch a path outside the
 * mount's prefix, disconnect or edit the mount. Every object is named `sr-r2-<run id>-...`, is generated in the
 * test (about 65 MB in all, under the 100 MB limit), and is deleted by the app's own delete (the Delete button
 * of the bucket browser, then its confirm).
 */

const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const MOUNT_LABEL = 'sideroom-test';
const MOUNT_PREFIX = 'sideroom-test/';
const API = '/api/v1';
const MB = 1024 * 1024;

test.skip(process.env.SR_R2_ON !== '1', 'needs the owner’s connected bucket (SR_R2_ON=1)');
// A trace or a screenshot would keep the mounts list (it carries the access key id and the endpoint).
test.use({ trace: 'off', screenshot: 'off', video: 'off' });
test.setTimeout(1_200_000);

// ---------------------------------------------------------------------------------------------------------
// Small helpers

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** One short line for the log: URLs and 32-character ids (an account id) are removed, the length is capped. */
function clean(s: string): string {
  return s
    .replace(/https?:\/\/\S+/g, '<url>')
    .replace(/\b[0-9a-f]{32}\b/gi, '<id>')
    .replace(/\s+/g, ' ')
    .slice(0, 220);
}
const msg = (e: unknown) =>
  clean(e instanceof Error ? (e.message.split('\n')[0] ?? '') : String(e));

const failures: string[] = [];
let stepNo = 0;
/** One step: prints `ok` or `FAIL <what>` and goes on. */
async function step(name: string, fn: () => Promise<string | void>) {
  const n = ++stepNo;
  await test.step(`${n} ${name}`, async () => {
    try {
      const note = await fn();
      console.log(`R2 step ${n} ${name}: ok${note ? ` (${note})` : ''}`);
    } catch (e) {
      failures.push(`${n} ${name}`);
      console.log(`R2 step ${n} ${name}: FAIL ${msg(e)}`);
    }
  });
}

function crc32(buf: Buffer): number {
  let c = ~0;
  for (const b of buf) {
    c ^= b;
    for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1;
  }
  return ~c >>> 0;
}
function pngChunk(type: string, data: Buffer): Buffer {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}
/** A valid 256 x 256 RGB PNG of noise (it does not compress): about 197 KB. */
function noisePng(): Buffer {
  const w = 256;
  const h = 256;
  const rows: Buffer[] = [];
  for (let y = 0; y < h; y++) rows.push(Buffer.concat([Buffer.from([0]), randomBytes(w * 3)]));
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 2; // RGB
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk('IHDR', ihdr),
    pngChunk('IDAT', deflateSync(Buffer.concat(rows), { level: 0 })),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}
/** A mono 16-bit PCM WAV of one second of a 440 Hz tone at 8 kHz (a track the indexer takes by its extension). */
function toneWav(): Buffer {
  const rate = 8000;
  const frames = rate;
  const b = Buffer.alloc(44 + frames * 2);
  b.write('RIFF', 0);
  b.writeUInt32LE(36 + frames * 2, 4);
  b.write('WAVEfmt ', 8);
  b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20);
  b.writeUInt16LE(1, 22);
  b.writeUInt32LE(rate, 24);
  b.writeUInt32LE(rate * 2, 28);
  b.writeUInt16LE(2, 32);
  b.writeUInt16LE(16, 34);
  b.write('data', 36);
  b.writeUInt32LE(frames * 2, 40);
  for (let i = 0; i < frames; i++) {
    b.writeInt16LE(Math.round(8000 * Math.sin((2 * Math.PI * 440 * i) / rate)), 44 + i * 2);
  }
  return b;
}

// ---------------------------------------------------------------------------------------------------------
// The app's API as the signed-in user (cookie session; GET needs no token, POST goes through `apiPost`)

async function getJson(
  request: APIRequestContext,
  path: string,
): Promise<{ status: number; body: unknown }> {
  const res = await request.get(`${API}${path}`, { headers: { Origin: ORIGIN } });
  let body: unknown = null;
  try {
    body = await res.json();
  } catch {
    body = null;
  }
  return { status: res.status(), body };
}

const rec = (v: unknown): Record<string, unknown> =>
  typeof v === 'object' && v !== null ? (v as Record<string, unknown>) : {};
const str = (v: unknown): string => (typeof v === 'string' ? v : '');

interface MountLite {
  id: string;
  prefix: string;
  active: boolean;
  corsPassed: boolean;
  responseTypeFailed: boolean;
}

/**
 * The mount the owner connected, found by its label. The list answer also carries the access key id and the
 * endpoint: they are never read here. Only the five fields below are taken out of it, and the answer is dropped.
 */
async function findMount(request: APIRequestContext): Promise<MountLite | string> {
  const { status, body } = await getJson(request, '/users/@me/storage/mounts');
  if (status !== 200 || !Array.isArray(body)) {
    return `the mounts list answered ${status}: is the Owner signed in on the right stack?`;
  }
  const hits = body.filter((m) => str(rec(m).label) === MOUNT_LABEL);
  if (hits.length === 0) return `no mount labelled ${MOUNT_LABEL} on this account`;
  if (hits.length > 1) return `more than one mount labelled ${MOUNT_LABEL}`;
  const m = rec(hits[0]);
  const rows = rec(m.last_test).rows;
  const failed =
    Array.isArray(rows) &&
    rows.some((r) => str(rec(r).id) === 'response_type' && str(rec(r).status) === 'fail');
  return {
    id: str(m.id),
    prefix: str(m.prefix),
    active: str(m.status) === 'active',
    corsPassed: typeof m.cors_ok_at === 'string' && m.cors_ok_at !== '',
    responseTypeFailed: failed,
  };
}

interface Entry {
  key: string;
  size: number;
}
/** Every object under the mount's prefix (mount-relative keys), all pages, no delimiter. */
async function listAll(request: APIRequestContext, mountId: string): Promise<Entry[]> {
  const out: Entry[] = [];
  let cursor = '';
  for (let page = 0; page < 200; page++) {
    const q = new URLSearchParams({ prefix: '', delimiter: '' });
    if (cursor) q.set('cursor', cursor);
    const { status, body } = await getJson(request, `/storage/mounts/${mountId}/objects?${q}`);
    if (status !== 200) throw new Error(`the object list answered ${status}`);
    const b = rec(body);
    const objects = Array.isArray(b.objects) ? b.objects : [];
    for (const o of objects) out.push({ key: str(rec(o).key), size: Number(rec(o).size ?? 0) });
    const next = b.next_cursor;
    if (typeof next !== 'string' || next === '') return out;
    cursor = next;
  }
  throw new Error('the object list did not end after 200 pages');
}

interface Fetched {
  ok: boolean;
  status?: number;
  length?: number;
  contentRange?: string | null;
  acceptRanges?: string | null;
  error?: string;
}
/** A GET from the page itself, so the browser's own CORS rules apply. The URL is never logged. */
async function pageFetch(page: Page, url: string, range?: string): Promise<Fetched> {
  return page.evaluate(
    async (a) => {
      try {
        const r = await fetch(a.url, a.range ? { headers: { Range: a.range } } : {});
        const buf = await r.arrayBuffer();
        return {
          ok: true,
          status: r.status,
          length: buf.byteLength,
          contentRange: r.headers.get('content-range'),
          acceptRanges: r.headers.get('accept-ranges'),
        };
      } catch (e) {
        return { ok: false, error: String(e).slice(0, 80) };
      }
    },
    { url, range },
  );
}

async function presignGet(request: APIRequestContext, mountId: string, key: string) {
  const r = await apiPost<{ url: string }>(request, `/storage/mounts/${mountId}/presign`, {
    key,
    method: 'GET',
  });
  return r.url;
}

/** The app's own delete: the Delete button of the bucket browser's row, then its confirm. */
async function deleteViaApp(page: Page, mountId: string, name: string) {
  await page.goto(`/storage/${mountId}`);
  const button = page.getByRole('button', { name: `Delete ${name}`, exact: true });
  await button.waitFor({ timeout: 30_000 });
  await button.click();
  const dialog = page.getByRole('alertdialog').or(page.getByRole('dialog'));
  await dialog.getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(button).toHaveCount(0, { timeout: 30_000 });
}

// ---------------------------------------------------------------------------------------------------------

test('BYOS against the owner’s real bucket: upload, CORS, shelf, listing, delete', async ({
  owner,
  signedIn,
}, testInfo) => {
  const { context, page } = await signedIn(owner);
  const request = context.request;
  const run = Date.now().toString(36);
  const stamp = `sr-r2-${run}`;

  // -- Find the mount. Anything wrong: one line, and nothing is touched.
  const found = await findMount(request);
  if (typeof found === 'string') {
    console.log(`R2 STOP: ${found}. Nothing was touched.`);
    throw new Error(`R2 STOP: ${found}`);
  }
  const mount = found;
  if (mount.prefix !== MOUNT_PREFIX) {
    console.log(
      `R2 STOP: the mount ${MOUNT_LABEL} has the prefix "${clean(mount.prefix)}", not exactly "${MOUNT_PREFIX}". Nothing was touched.`,
    );
    throw new Error('R2 STOP: the mount prefix is not sideroom-test/');
  }
  if (!mount.active || !mount.corsPassed) {
    console.log(
      `R2 STOP: the mount is ${mount.active ? 'active' : 'not active'} and its CORS check has ${mount.corsPassed ? '' : 'not '}passed (the app refuses uploads until it has). Nothing was touched.`,
    );
    throw new Error('R2 STOP: the mount is not ready');
  }
  console.log(
    `R2 mount found (prefix ok, active, CORS passed). Origin under test: ${ORIGIN}. Run id: ${run}`,
  );
  if (mount.responseTypeFailed) {
    console.log(
      'R2 note: the mount’s stored connection test has a failed response_type row; the app will refuse download links (409 MOUNT_NOT_SAFE).',
    );
  }

  // -- The files (generated here; about 65 MB in all).
  const dir = testInfo.outputPath('r2-files');
  mkdirSync(dir, { recursive: true });
  const files = [
    { name: `${stamp}-1k.txt`, data: Buffer.from('sideroom r2 test\n'.repeat(64).slice(0, 1024)) },
    { name: `${stamp}-img.png`, data: noisePng() },
    { name: `${stamp}-5mb.bin`, data: randomBytes(5 * MB) },
    { name: `${stamp}-60mb.bin`, data: randomBytes(60 * MB) },
    { name: `${stamp}-track.wav`, data: toneWav() },
  ];
  const total = files.reduce((n, f) => n + f.data.length, 0);
  expect(total, 'bytes the run uploads').toBeLessThan(100 * MB);
  const paths = files.map((f) => `${dir}/${f.name}`);
  files.forEach((f, i) => writeFileSync(paths[i] ?? '', f.data));
  const size = (n: string) => files.find((f) => f.name === n)?.data.length ?? 0;
  const [txt, img, bin5, bin60, wav] = files.map((f) => f.name) as [
    string,
    string,
    string,
    string,
    string,
  ];
  console.log(`R2 files generated: ${files.length}, ${(total / MB).toFixed(1)} MB in all`);

  /** Every object this run may have made: the finally deletes what is left of exactly these. */
  const made = new Set<string>();

  // What the browser sends to the bucket (host never logged): method, status, whether CORS headers are present.
  const puts: { method: string; status: number; acao: boolean }[] = [];
  let blocked = 0;
  const isBucket = (url: string) => {
    try {
      return new URL(url).origin !== ORIGIN;
    } catch {
      return false;
    }
  };
  page.on('response', (res) => {
    const req = res.request();
    if (req.method() === 'PUT' && isBucket(req.url())) {
      puts.push({
        method: 'PUT',
        status: res.status(),
        acao: res.headers()['access-control-allow-origin'] !== undefined,
      });
    }
  });
  page.on('requestfailed', (req) => {
    if (req.method() === 'PUT' && isBucket(req.url())) blocked += 1;
  });

  const mountId = mount.id;
  try {
    // ---- 1. Upload through the app (the Storage page's upload): the browser's PUT to the bucket is the CORS check.
    await step('upload through the app (CORS)', async () => {
      for (const f of files) made.add(f.name);
      await page.goto(`/storage/${mountId}`);
      await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
      await page.getByTestId('sto-pick-files').setInputFiles(paths);
      const deadline = Date.now() + 15 * 60_000;
      for (;;) {
        const have = new Set((await listAll(request, mountId)).map((e) => e.key));
        const missing = files.filter((f) => !have.has(f.name));
        if (missing.length === 0) break;
        if (blocked >= 4) break;
        if (Date.now() > deadline)
          throw new Error(`${missing.length} file(s) not in the bucket after 15 min`);
        await sleep(3000);
      }
      const statuses: Record<string, number> = {};
      for (const p of puts) statuses[String(p.status)] = (statuses[String(p.status)] ?? 0) + 1;
      const withAcao = puts.filter((p) => p.acao).length;
      const detail = `PUT requests ${puts.length}, answers ${JSON.stringify(statuses)}, access-control-allow-origin present on ${withAcao} of ${puts.length}, blocked ${blocked}`;
      if (blocked > 0) {
        throw new Error(
          `CORS: the bucket does not allow this origin (${ORIGIN}); ${detail}. Fix the bucket's CORS (runbook) and run once more.`,
        );
      }
      if (puts.length === 0) throw new Error('no browser PUT to the bucket was seen');
      if (puts.some((p) => p.status < 200 || p.status > 299))
        throw new Error(`a PUT was refused: ${detail}`);
      const sizes = (await listAll(request, mountId)).filter((e) => made.has(e.key));
      for (const f of files) {
        const e = sizes.find((x) => x.key === f.name);
        if (!e || e.size !== f.data.length) {
          throw new Error(
            `${f.name} is ${e ? e.size : 'missing'} bytes in the listing, expected ${f.data.length}`,
          );
        }
      }
      return `${detail}; all ${files.length} objects listed with the right sizes`;
    });

    // ---- 2. The message attachment path, and what the mount's links do.
    await step('message attachment (report) and the mount image and download link', async () => {
      // Read in the code: a channel attachment (routes/messages/attachments.rs) goes to the INSTANCE's storage,
      // never to a member's mount, so it cannot be tested with this bucket. The mount's own image preview and
      // download link are what is tested here.
      console.log(
        'R2 note: channel attachments use the instance storage, not the member’s mount; not testable with this bucket (see the runbook).',
      );
      await page.goto(`/storage/${mountId}`);
      await page.getByRole('button', { name: `Preview ${img}`, exact: true }).click();
      const shown = page.getByRole('img', { name: img, exact: true });
      await expect(shown).toBeVisible();
      await expect
        .poll(() => shown.evaluate((el) => (el as HTMLImageElement).naturalWidth), {
          timeout: 30_000,
        })
        .toBeGreaterThan(0);
      const notes: string[] = ['image shows (naturalWidth > 0)'];
      for (const name of [txt, img, bin5]) {
        const r = await pageFetch(page, await presignGet(request, mountId, name));
        if (!r.ok) throw new Error(`the download of ${name} was blocked or failed: ${r.error}`);
        if (r.status !== 200) throw new Error(`the download of ${name} answered ${r.status}`);
        if (r.length !== size(name))
          throw new Error(`${name} came back ${r.length} bytes, expected ${size(name)}`);
        notes.push(`${name.slice(stamp.length + 1)} 200, ${r.length} bytes`);
      }
      return notes.join('; ');
    });

    // ---- 3. A shelf item (a track the indexer takes by its extension): lists, opens, a ranged GET answers 206.
    await step('shelf item (index, list, open, ranged GET)', async () => {
      const queuedAt = Date.now() - 5000;
      await apiPost(request, `/storage/mounts/${mountId}/index`, {});
      const deadline = Date.now() + 120_000;
      for (;;) {
        const { body } = await getJson(request, `/storage/mounts/${mountId}/index`);
        const s = rec(body);
        const fin = str(s.finished_at);
        if ((s.state === 'done' || s.state === 'failed') && fin && Date.parse(fin) >= queuedAt) {
          if (s.state === 'failed') throw new Error(`the index run failed: ${clean(str(s.error))}`);
          break;
        }
        if (Date.now() > deadline)
          throw new Error('the index run did not finish in 120 s (is the job runner on?)');
        await sleep(3000);
      }
      let item: Record<string, unknown> | null = null;
      for (let offset = 0; offset < 1000 && !item; offset += 50) {
        const { status, body } = await getJson(
          request,
          `/shelves/music?mount=${mountId}&limit=50&offset=${offset}`,
        );
        if (status !== 200) throw new Error(`the music shelf answered ${status}`);
        const b = rec(body);
        const items = Array.isArray(b.items) ? b.items : [];
        item = items.map(rec).find((i) => str(i.object_key) === wav) ?? null;
        if (items.length === 0 || offset + 50 >= Number(b.total ?? 0)) break;
      }
      if (!item) throw new Error('the track is not on the music shelf after the index run');
      const opened = await getJson(request, `/shelves/music/${str(item.id)}`);
      if (opened.status !== 200)
        throw new Error(`opening the shelf item answered ${opened.status}`);
      const url = await presignGet(request, mountId, wav);
      const r = await pageFetch(page, url, 'bytes=0-1023');
      if (!r.ok) throw new Error(`the ranged GET was blocked or failed: ${r.error}`);
      if (r.status !== 206) throw new Error(`the ranged GET answered ${r.status}, not 206`);
      if (r.length !== 1024) throw new Error(`the ranged GET gave ${r.length} bytes, not 1024`);
      // The player needs to read these (the bucket's CORS must expose them).
      const big = await pageFetch(
        page,
        await presignGet(request, mountId, bin60),
        'bytes=1000000-1001023',
      );
      if (!big.ok || big.status !== 206 || big.length !== 1024) {
        throw new Error(
          `the ranged GET inside the 60 MB object answered ${big.status ?? big.error}`,
        );
      }
      return `listed, opened, ranged GET 206 with 1024 bytes; content-range readable by the page: ${r.contentRange ? 'yes' : 'NO'}; accept-ranges readable: ${r.acceptRanges ? 'yes' : 'NO'}; 60 MB object ranged 206`;
    });
    // A Study file reads from a shelf the same way (a PDF or an EPUB through the same presign); none is made here.

    // ---- 4. Listing: the objects under the prefix, and nothing of the spec's outside it.
    await step('listing under the prefix', async () => {
      const all = await listAll(request, mountId);
      for (const f of files) {
        const n = all.filter((e) => e.key === f.name).length;
        if (n !== 1) throw new Error(`${f.name} is listed ${n} times, expected once`);
      }
      const odd = all.filter(
        (e) => e.key.startsWith('/') || e.key.includes('..') || e.key.startsWith(MOUNT_PREFIX),
      );
      if (odd.length > 0) throw new Error(`${odd.length} listed key(s) are not mount-relative`);
      const escape = await getJson(
        request,
        `/storage/mounts/${mountId}/objects?prefix=${encodeURIComponent('../')}`,
      );
      if (escape.status < 400 || escape.status >= 500) {
        throw new Error(`a listing of ../ answered ${escape.status}, expected a refusal (4xx)`);
      }
      const others = all.filter((e) => !made.has(e.key)).length;
      return `this run's ${files.length} objects listed once each; ${others} other object(s) under the prefix; ../ is refused (${escape.status})`;
    });

    // ---- 5. Delete each through the app; the list no longer shows it; a GET of its URL no longer answers 200.
    await step('delete through the app', async () => {
      const problems: string[] = [];
      for (const f of files) {
        try {
          const url = await presignGet(request, mountId, f.name);
          await deleteViaApp(page, mountId, f.name);
          made.delete(f.name);
          const still = (await listAll(request, mountId)).some((e) => e.key === f.name);
          if (still) problems.push(`${f.name} is still listed`);
          // From the test's own HTTP client (no CORS): the signed URL of the deleted object.
          const after = await request.get(url, { maxRedirects: 0, failOnStatusCode: false });
          if (after.status() === 200) problems.push(`${f.name} still answers 200`);
        } catch (e) {
          problems.push(`${f.name}: ${msg(e)}`);
        }
      }
      if (problems.length > 0) throw new Error(problems.join('; '));
      // The shelf row of the track goes with it (the app's delete hides the row).
      const { body } = await getJson(request, `/shelves/music?mount=${mountId}&limit=50`);
      const items = Array.isArray(rec(body).items) ? (rec(body).items as unknown[]) : [];
      const row = items.map(rec).some((i) => str(i.object_key) === wav);
      return `all ${files.length} deleted, none listed, none answers 200; shelf row ${row ? 'STILL THERE' : 'gone'}`;
    });
  } finally {
    // ---- 6. Delete whatever of this run is left, then count.
    try {
      const left = (await listAll(request, mountId)).filter((e) => made.has(e.key));
      for (const e of left) {
        try {
          await deleteViaApp(page, mountId, e.key);
          made.delete(e.key);
        } catch (err) {
          console.log(
            `R2 cleanup: could not delete ${e.key} through the app: ${msg(err)}. The owner deletes it in the bucket's dashboard.`,
          );
        }
      }
    } catch (err) {
      console.log(`R2 cleanup: listing failed: ${msg(err)}`);
    }
    for (const p of paths) rmSync(p, { force: true });
    let runLeft = 'unknown';
    let inAll = 'unknown';
    try {
      const all = await listAll(request, mountId);
      runLeft = String(all.filter((e) => e.key.startsWith(stamp)).length);
      inAll = String(all.length);
    } catch (err) {
      console.log(`R2 cleanup: the final listing failed: ${msg(err)}`);
    }
    console.log(
      `R2 RESULT objects of this run (${stamp}-*) left under the prefix: ${runLeft} (0 is the pass)`,
    );
    console.log(
      `R2 RESULT objects under the prefix in all: ${inAll} (check against the Cloudflare dashboard)`,
    );
  }
  expect(failures, 'steps that printed FAIL').toEqual([]);
});
