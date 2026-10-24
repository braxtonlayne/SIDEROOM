// Live end-to-end check of object storage through sr-server against a REAL
// S3-compatible server (Garage / MinIO). NOT part of the unit-test path.
// Full instructions: docs/dev/storage-live-test.md
//
// Steps (the server must already be running with the storage env below):
//   1. createdb sr_s3 ; start the S3 server and create the bucket.
//   2. Start sr-server with
//        SR_DATABASE__URL=postgres://postgres:<pw>@localhost:5432/sr_s3
//        SR_SERVER__BIND=127.0.0.1:8092  SR_SERVER__METRICS_BIND=127.0.0.1:9192
//        SR_SERVER__PUBLIC_URL=http://localhost:8092
//        SR_SERVER__SECRET_KEY=<64 hex>  SR_SEARCH__DIR=<scratch dir>
//        SR_STORAGE__ENDPOINT=$SR_TEST_S3_ENDPOINT  SR_STORAGE__BUCKET=$SR_TEST_S3_BUCKET
//        SR_STORAGE__REGION=$SR_TEST_S3_REGION      SR_STORAGE__ACCESS_KEY=...  SR_STORAGE__SECRET_KEY=...
//   3. node e2e/support/storage-live.mjs
//
// Required env:
//   SR_TEST_S3_ENDPOINT, SR_TEST_S3_BUCKET, SR_TEST_S3_ACCESS_KEY, SR_TEST_S3_SECRET_KEY
//     (same values the server was started with; the driver signs its own S3 requests
//      to inspect objects directly)
//   SR_TEST_S3_REGION          default "garage"
//   SR_TEST_DATABASE_URL       postgres URL of the server's database (used for sr-cli,
//                              and psql to age an abandoned upload / force the sweep)
// Optional env:
//   SR_TEST_BASE               default http://127.0.0.1:8092   (API at /api/v1)
//   SR_TEST_ORIGIN             default http://localhost:8092
//   SR_CLI_BIN                 default <repo>/../SIDEROOM_wt/s3-target/debug/sr-cli(.exe) else cargo run
//   PSQL                       psql executable (default "psql")
//   SR_TEST_SWEEP_WAIT_MS      default 15000 (how long to wait for the job runner)
//
// Exit code 0 only if every check passes. A check that fails because the
// server does not implement something (e.g. a /cdn/* route) is a real finding.

import { createHmac, createHash, randomBytes } from 'node:crypto';
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { listParts } from './s3-sign.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');
const env = (k, d) =>
  process.env[k] ??
  d ??
  (() => {
    throw new Error(`${k} must be set`);
  })();

const S3 = {
  endpoint: env('SR_TEST_S3_ENDPOINT').replace(/\/$/, ''),
  bucket: env('SR_TEST_S3_BUCKET'),
  region: env('SR_TEST_S3_REGION', 'garage'),
  ak: env('SR_TEST_S3_ACCESS_KEY'),
  sk: env('SR_TEST_S3_SECRET_KEY'),
};
const DB = env('SR_TEST_DATABASE_URL');
const BASE = env('SR_TEST_BASE', 'http://127.0.0.1:8092');
const API = BASE + '/api/v1';
const ORIGIN = env('SR_TEST_ORIGIN', 'http://localhost:8092');
const PSQL = env('PSQL', 'psql');
const MIB = 1024 * 1024;

let failures = 0;
const check = (name, ok, detail) => {
  console.log(
    `${ok ? 'PASS' : 'FAIL'}  ${name}${ok ? '' : '  -> ' + (typeof detail === 'string' ? detail : JSON.stringify(detail))?.slice(0, 500)}`,
  );
  if (!ok) failures++;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---- S3 SigV4 query presign (path-style), used only to inspect objects ----
const hmac = (k, s) => createHmac('sha256', k).update(s).digest();
const sha = (s) => createHash('sha256').update(s).digest('hex');
const enc = (s) =>
  encodeURIComponent(s).replace(
    /[!'()*]/g,
    (c) => '%' + c.charCodeAt(0).toString(16).toUpperCase(),
  );
function presign(method, key, expires = 60) {
  const u = new URL(S3.endpoint);
  const now = new Date().toISOString().replace(/[:-]|\.\d{3}/g, '');
  const date = now.slice(0, 8);
  const scope = `${date}/${S3.region}/s3/aws4_request`;
  const path = '/' + [S3.bucket, ...key.split('/')].map(enc).join('/');
  const q = {
    'X-Amz-Algorithm': 'AWS4-HMAC-SHA256',
    'X-Amz-Credential': `${S3.ak}/${scope}`,
    'X-Amz-Date': now,
    'X-Amz-Expires': String(expires),
    'X-Amz-SignedHeaders': 'host',
  };
  const qs = Object.keys(q)
    .sort()
    .map((k) => `${enc(k)}=${enc(q[k])}`)
    .join('&');
  const canon = [method, path, qs, `host:${u.host}\n`, 'host', 'UNSIGNED-PAYLOAD'].join('\n');
  const sts = ['AWS4-HMAC-SHA256', now, scope, sha(canon)].join('\n');
  let k = hmac('AWS4' + S3.sk, date);
  for (const p of [S3.region, 's3', 'aws4_request']) k = hmac(k, p);
  const sig = createHmac('sha256', k).update(sts).digest('hex');
  return `${u.origin}${path}?${qs}&X-Amz-Signature=${sig}`;
}
/** Object state straight from S3: {exists, body}. */
async function s3Get(key) {
  const r = await fetch(presign('GET', key));
  return r.status === 200
    ? { exists: true, body: Buffer.from(await r.arrayBuffer()) }
    : { exists: false, status: r.status };
}

// ---- helpers ----
function runCli(args, extra = {}) {
  const exe = process.platform === 'win32' ? 'sr-cli.exe' : 'sr-cli';
  const guess = [
    process.env.SR_CLI_BIN,
    resolve(repoRoot, 'target/debug', exe),
    resolve(repoRoot, '..', 's3-target/debug', exe),
  ]
    .filter(Boolean)
    .find((p) => existsSync(p));
  const [cmd, a] = guess ? [guess, args] : ['cargo', ['run', '-q', '-p', 'sr-cli', '--', ...args]];
  const r = spawnSync(cmd, a, {
    cwd: repoRoot,
    encoding: 'utf8',
    env: { ...process.env, SR_DATABASE__URL: DB, SR_SERVER__PUBLIC_URL: ORIGIN, ...extra },
  });
  return r;
}
function newInvite() {
  const r = runCli(['invite', 'create', '--note', 'storage-live']);
  const code = (r.stdout || '')
    .split(/\r?\n/)
    .map((l) => l.trim())
    .find((l) => /^[0-9A-HJKMNP-TV-Z]{5}-[0-9A-HJKMNP-TV-Z]{5}$/.test(l));
  if (!code)
    throw new Error(
      `no invite code from sr-cli (is an Owner bootstrapped? run: sr-cli bootstrap ...):\n${r.stdout}\n${r.stderr}`,
    );
  return code;
}
function sql(q) {
  return execFileSync(PSQL, [DB, '-X', '-A', '-t', '-c', q], { encoding: 'utf8' }).trim();
}

class Client {
  cookie = '';
  csrf = '';
  id = '';
  async raw(method, url, { headers = {}, body } = {}) {
    const h = { Origin: ORIGIN, ...headers };
    if (this.cookie) h.Cookie = this.cookie;
    if (this.csrf && method !== 'GET') h['X-SR-CSRF'] = this.csrf;
    const res = await fetch(url, { method, headers: h, body });
    for (const sc of res.headers.getSetCookie?.() ?? []) {
      const [kv] = sc.split(';');
      const k = kv.split('=')[0];
      this.cookie = this.cookie
        .split('; ')
        .filter((c) => c && !c.startsWith(k + '='))
        .concat(kv)
        .join('; ');
    }
    return res;
  }
  async call(method, path, body) {
    const res = await this.raw(method, API + path, {
      headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await res.text();
    let json;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      json = text;
    }
    return { status: res.status, body: json };
  }
  async register(code, name) {
    const r = await this.call('POST', '/sr/register', {
      code,
      email: `${name}@example.test`,
      username: name,
      password: 'Smoke-Test-Passw0rd-xyz!',
      accepted_house_notes: true,
      confirmed_min_age: 13,
    });
    this.id = r.body?.id;
    this.csrf = (await this.call('GET', '/sr/csrf')).body?.token ?? '';
    return r;
  }
}

// Minimal PNG (1x1) for the emoji upload.
const PNG_1X1 =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';

/** slot -> presigned PUT. Returns {slot, putStatus, put}. */
async function uploadFile(c, channel, { filename, type, bytes, declaredSize }) {
  const r = await c.call('POST', `/channels/${channel}/attachments`, {
    files: [{ id: '1', filename, file_size: declaredSize ?? bytes.length, content_type: type }],
  });
  if (r.status !== 200) return { slotRes: r };
  const slot = r.body.attachments[0];
  const put = await fetch(slot.upload_url, {
    method: 'PUT',
    headers: { 'Content-Type': type ?? 'application/octet-stream' },
    body: bytes,
  });
  return { slotRes: r, slot, putStatus: put.status };
}

// ================= run =================
const sfx = Math.floor(Math.random() * 1e6);

// 1. readiness
{
  const r = await fetch(BASE + '/readyz').catch((e) => ({
    status: 0,
    text: async () => String(e),
  }));
  const t = await r.text();
  check(
    '/readyz reports storage ok',
    r.status === 200 && /storage/.test(t) && !/fail|error/i.test(t),
    `${r.status} ${t}`,
  );
}

// 2. accounts, guild, channel
const A = new Client();
let r = await A.register(newInvite(), 'sto' + sfx);
check('register user', r.status < 300, r);
r = await A.call('POST', '/guilds', { name: 'Storage Guild' });
check('create guild', r.status < 300, r);
const guild = r.body?.id;
const chans = (await A.call('GET', `/guilds/${guild}/channels`)).body;
const channel = chans?.find((c) => c.type === 0)?.id;
check('have a text channel', !!channel, chans);

// 3. happy path: text attachment
const textBytes = Buffer.from('hello from the live storage test\n'.repeat(2000)); // ~66 KB, UTF-8
{
  const u = await uploadFile(A, channel, {
    filename: 'notes.txt',
    type: 'text/plain',
    bytes: textBytes,
  });
  check(
    'upload slot issued with presigned URL',
    u.slotRes.status === 200 && /^https?:\/\//.test(u.slot?.upload_url ?? ''),
    u.slotRes,
  );
  check('presigned PUT accepted by S3', u.putStatus === 200, u.putStatus);
  const key = sql(`SELECT storage_key FROM attachments WHERE id = ${u.slot.upload_id}`);
  const obj = await s3Get(key);
  check(
    'object present in S3 under the slot key',
    obj.exists && Buffer.compare(obj.body, textBytes) === 0,
    key,
  );
  const m = await A.call('POST', `/channels/${channel}/messages`, {
    content: 'with file',
    attachments: [{ id: u.slot.id, uploaded_filename: u.slot.upload_filename }],
  });
  check(
    'message referencing the upload is created',
    m.status < 300 && m.body?.attachments?.length === 1,
    m,
  );
  const got = await A.call('GET', `/channels/${channel}/messages/${m.body?.id}`);
  const att = (got.body?.attachments ?? [])[0];
  check(
    'GET message returns attachment with sniffed text/plain',
    att?.content_type === 'text/plain' && Number(att?.size) === textBytes.length,
    att,
  );
  const url = att?.url ?? '';
  check('attachment URL is signed (?ex=&sig=)', /[?&]ex=\d+&sig=/.test(url), url);
  const abs = url.startsWith('http') ? url : BASE + url;
  const full = await fetch(abs).catch((e) => ({ status: 0, err: String(e) }));
  check(
    'signed CDN URL serves the full body (200)',
    full.status === 200 && Buffer.compare(Buffer.from(await full.arrayBuffer()), textBytes) === 0,
    `status ${full.status} (a 404 means sr-server has no /cdn/* route yet)`,
  );
  const rng = await fetch(abs, { headers: { Range: 'bytes=5-14' } }).catch(() => ({ status: 0 }));
  const rb = rng.status ? Buffer.from(await rng.arrayBuffer()) : Buffer.alloc(0);
  check(
    'Range request returns 206 with the slice',
    rng.status === 206 &&
      rb.equals(textBytes.subarray(5, 15)) &&
      rng.headers.get('content-range') === `bytes 5-14/${textBytes.length}`,
    `status ${rng.status}`,
  );
  const tampered = await fetch(
    abs.replace(/sig=(.)/, (_, c) => 'sig=' + (c === 'A' ? 'B' : 'A')),
  ).catch(() => ({ status: 0 }));
  check('tampered CDN signature rejected', [401, 403].includes(tampered.status), tampered.status);
  const noSig = await fetch(abs.split('?')[0]).catch(() => ({ status: 0 }));
  check('unsigned CDN URL rejected', [401, 403].includes(noSig.status), noSig.status);
}

// 4. EXIF strip on a real JPEG with orientation 6 (4x2 -> 2x4 upright, GPS marker gone)
{
  const jpg = readFileSync(resolve(here, 'fixtures/exif-orient6.jpg'));
  check(
    'fixture carries EXIF and a GPS marker',
    jpg.includes('Exif') && jpg.includes('GPS-SECRET'),
  );
  const u = await uploadFile(A, channel, { filename: 'photo.jpg', type: 'image/jpeg', bytes: jpg });
  check('jpeg PUT accepted', u.putStatus === 200, u);
  const c = await A.call('POST', `/channels/${channel}/attachments/${u.slot.upload_id}/complete`);
  check(
    'complete: image/jpeg verified',
    c.status === 200 && c.body?.content_type === 'image/jpeg',
    c,
  );
  const key = sql(`SELECT storage_key FROM attachments WHERE id = ${u.slot.upload_id}`);
  const obj = await s3Get(key);
  check(
    'stored JPEG has no Exif / GPS marker',
    obj.exists && !obj.body.includes('Exif') && !obj.body.includes('GPS-SECRET'),
    obj.exists ? obj.body.length : obj,
  );
  // JPEG SOF0/SOF2 dimensions: height then width after the marker.
  const b = obj.body ?? Buffer.alloc(0);
  let dims = null;
  for (let i = 2; i < b.length - 9; i++)
    if (b[i] === 0xff && (b[i + 1] === 0xc0 || b[i + 1] === 0xc2)) {
      dims = [b.readUInt16BE(i + 7), b.readUInt16BE(i + 5)];
      break;
    }
  check('orientation applied to pixels (now 2x4)', dims && dims[0] === 2 && dims[1] === 4, dims);
  check(
    'size in DB reflects stripped object',
    Number(sql(`SELECT size_bytes FROM attachments WHERE id = ${u.slot.upload_id}`)) === b.length,
  );
}

// 5. mismatched declared content type is rejected and the object removed
{
  const u = await uploadFile(A, channel, {
    filename: 'fake.png',
    type: 'image/png',
    bytes: Buffer.from('this is definitely not a png '.repeat(10)),
  });
  check('mismatch: PUT accepted by S3', u.putStatus === 200, u);
  const key = sql(`SELECT storage_key FROM attachments WHERE id = ${u.slot.upload_id}`);
  const c = await A.call('POST', `/channels/${channel}/attachments/${u.slot.upload_id}/complete`);
  check('mismatched content-type rejected (400)', c.status === 400, c);
  check('rejected object deleted from S3', !(await s3Get(key)).exists);
  // wrong size
  const w = await uploadFile(A, channel, {
    filename: 'short.bin',
    type: 'application/octet-stream',
    bytes: Buffer.alloc(20, 1),
    declaredSize: 10,
  });
  const c2 = await A.call('POST', `/channels/${channel}/attachments/${w.slot.upload_id}/complete`);
  check('size different from the declared size rejected (400)', c2.status === 400, c2);
}

// 6. size limits
{
  const ok = await A.call('POST', `/channels/${channel}/attachments`, {
    files: [{ filename: 'edge.bin', file_size: 30 * MIB }],
  });
  check('exactly 30 MiB accepted for a slot', ok.status === 200, ok);
  const over = await A.call('POST', `/channels/${channel}/attachments`, {
    files: [{ filename: 'big.bin', file_size: 30 * MIB + 1 }],
  });
  check('30 MiB + 1 byte rejected (413)', over.status === 413, over);
  const zero = await A.call('POST', `/channels/${channel}/attachments`, {
    files: [{ filename: 'z.bin', file_size: 0 }],
  });
  check('empty file rejected (400)', zero.status === 400, zero);
  // Real upload of a ~29 MiB file to prove large single PUT works end to end.
  const big = randomBytes(29 * MIB);
  const u = await uploadFile(A, channel, {
    filename: 'large.bin',
    type: 'application/octet-stream',
    bytes: big,
  });
  check('29 MiB single presigned PUT accepted', u.putStatus === 200, u.putStatus);
  const c = await A.call('POST', `/channels/${channel}/attachments/${u.slot.upload_id}/complete`);
  check('29 MiB upload completes', c.status === 200, c);
}

// 7. emoji data-URI upload
{
  const before = new Set(
    (await A.call('GET', `/guilds/${guild}/emojis`)).body?.map?.((e) => e.id) ?? [],
  );
  const e = await A.call('POST', `/guilds/${guild}/emojis`, {
    name: 'live_' + sfx,
    image: `data:image/png;base64,${PNG_1X1}`,
  });
  check(
    'emoji created from a data URI',
    e.status < 300 && !!e.body?.id && !before.has(e.body.id),
    e,
  );
  const key = sql(`SELECT media_key FROM emojis WHERE id = ${e.body?.id}`);
  const obj = await s3Get(key);
  check(
    // Uploads are re-encoded (metadata stripped, fitted to 128 px), so compare the decoded
    // header rather than the bytes: a PNG whose IHDR still says 1x1.
    'emoji image object exists in S3 as a 1x1 PNG',
    obj.exists &&
      obj.body
        .subarray(0, 8)
        .equals(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) &&
      obj.body.readUInt32BE(16) === 1 &&
      obj.body.readUInt32BE(20) === 1,
    key,
  );
  const bad = await A.call('POST', `/guilds/${guild}/emojis`, {
    name: 'notimg_' + sfx,
    image: `data:image/png;base64,${Buffer.from('not an image').toString('base64')}`,
  });
  check('non-image emoji payload rejected', bad.status === 400, bad);
  const d = await A.call('DELETE', `/guilds/${guild}/emojis/${e.body?.id}`);
  check('emoji delete succeeds', d.status < 300, d);
  check('emoji object removed from S3 on delete', !(await s3Get(key)).exists);
}

// 8. abandoned upload sweep
{
  const u = await uploadFile(A, channel, {
    filename: 'abandoned.txt',
    type: 'text/plain',
    bytes: Buffer.from('never attached\n'),
  });
  const key = sql(`SELECT storage_key FROM attachments WHERE id = ${u.slot.upload_id}`);
  check('abandoned: object present before sweep', (await s3Get(key)).exists);
  sql(
    `UPDATE attachments SET created_at = now() - interval '2 days' WHERE id = ${u.slot.upload_id}`,
  );
  sql(
    `UPDATE jobs SET run_at = now() - interval '1 minute', state = 'queued', locked_at = NULL, locked_by = NULL WHERE kind = 'attachments.gc_unreferenced'`,
  );
  const wait = Number(env('SR_TEST_SWEEP_WAIT_MS', '15000'));
  let gone = false;
  for (let t = 0; t < wait && !gone; t += 1000) {
    await sleep(1000);
    gone = sql(`SELECT count(*) FROM attachments WHERE id = ${u.slot.upload_id}`) === '0';
  }
  check(
    'sweep deleted the abandoned attachment row',
    gone,
    'row still present; is the job runner running and the job due?',
  );
  check('sweep deleted the abandoned object from S3', !(await s3Get(key)).exists);
}

// 9. avatar / guild icon upload and the resizing /cdn proxy
{
  const { deflateSync } = await import('node:zlib');
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (b) => {
    let c = 0xffffffff;
    for (const x of b) c = crcTable[(c ^ x) & 255] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (t, d) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(d.length);
    const td = Buffer.concat([Buffer.from(t), d]);
    const c = Buffer.alloc(4);
    c.writeUInt32BE(crc(td));
    return Buffer.concat([len, td, c]);
  };
  const png = (w, h, seed) => {
    const ihdr = Buffer.alloc(13);
    ihdr.writeUInt32BE(w, 0);
    ihdr.writeUInt32BE(h, 4);
    ihdr[8] = 8;
    ihdr[9] = 2;
    const raw = Buffer.alloc((w * 3 + 1) * h);
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++) {
        const o = y * (w * 3 + 1) + 1 + x * 3;
        raw[o] = (x * 3 + seed) & 255;
        raw[o + 1] = (y * 5) & 255;
        raw[o + 2] = seed;
      }
    return Buffer.concat([
      Buffer.from('\x89PNG\r\n\x1a\n', 'latin1'),
      chunk('IHDR', ihdr),
      chunk('IDAT', deflateSync(raw)),
      chunk('IEND', Buffer.alloc(0)),
    ]);
  };
  const uri = (b) => `data:image/png;base64,${b.toString('base64')}`;
  const dims = (b) => [b.readUInt32BE(16), b.readUInt32BE(20)];
  const cdn = async (path, headers = {}) => {
    const r = await fetch(BASE + path, { headers });
    return { r, body: Buffer.from(await r.arrayBuffer()) };
  };

  // user avatar: 96x64 source is cropped to a 64x64 square
  const up = await A.call('PATCH', '/users/@me', { avatar: uri(png(96, 64, 7)) });
  const hash = up.body?.user?.avatar;
  check(
    'avatar upload returns a hash',
    up.status === 200 && /^(a_)?[0-9a-f]{32}$/.test(hash ?? ''),
    up,
  );
  const key = `avatars/${A.id}/${hash}`;
  const stored = await s3Get(key);
  check(
    'avatar original stored in S3 as a square',
    stored.exists && dims(stored.body).join('x') === '64x64',
    key,
  );
  const me = await A.call('GET', '/users/@me');
  check('GET /users/@me carries the avatar hash', me.body?.avatar === hash, me);

  const orig = await cdn(`/cdn/avatars/${A.id}/${hash}.png`);
  check(
    'unsigned avatar fetch serves the original',
    orig.r.status === 200 &&
      orig.r.headers.get('content-type') === 'image/png' &&
      orig.body.equals(stored.body),
    orig.r.status,
  );
  const small = await cdn(`/cdn/avatars/${A.id}/${hash}.png?size=16`);
  check(
    '?size=16 resizes to 16x16 PNG',
    small.r.status === 200 &&
      small.r.headers.get('content-type') === 'image/png' &&
      dims(small.body).join('x') === '16x16',
    small.r.status,
  );
  const cached = await s3Get(`${key}@16.png`);
  check(
    'resized variant cached in S3 under <key>@16.png',
    cached.exists && cached.body.equals(small.body),
  );
  const big = await cdn(`/cdn/avatars/${A.id}/${hash}.png?size=1024`);
  check(
    'size above the original is not upscaled',
    big.r.status === 200 && dims(big.body).join('x') === '64x64',
  );
  const webp = await cdn(`/cdn/avatars/${A.id}/${hash}.webp?size=32`);
  check(
    '.webp extension gives WebP',
    webp.r.status === 200 &&
      webp.r.headers.get('content-type') === 'image/webp' &&
      webp.body.subarray(8, 12).toString() === 'WEBP',
  );
  const badSize = await cdn(`/cdn/avatars/${A.id}/${hash}.png?size=100`);
  check('non power-of-two size is 400', badSize.r.status === 400, badSize.r.status);
  const badSig = await cdn(`/cdn/avatars/${A.id}/${hash}.png?ex=1&sig=bad`);
  check('a present but invalid signature is 403', badSig.r.status === 403, badSig.r.status);

  // replace: old object and its variants disappear
  const up2 = await A.call('PATCH', '/users/@me', { avatar: uri(png(32, 32, 99)) });
  const hash2 = up2.body?.user?.avatar;
  check('avatar replace gives a new hash', up2.status === 200 && hash2 && hash2 !== hash, up2);
  check(
    'replaced avatar object and variants removed from S3',
    !(await s3Get(key)).exists && !(await s3Get(`${key}@16.png`)).exists,
  );
  const notImg = await A.call('PATCH', '/users/@me', {
    avatar: `data:image/png;base64,${Buffer.from('<html>').toString('base64')}`,
  });
  check('non-image avatar rejected', notImg.status === 400, notImg);
  const rm = await A.call('PATCH', '/users/@me', { avatar: null });
  check('avatar removal clears the field', rm.status === 200 && rm.body?.user?.avatar === null, rm);
  check('removed avatar object deleted from S3', !(await s3Get(`avatars/${A.id}/${hash2}`)).exists);

  // guild icon
  const gi = await A.call('PATCH', `/guilds/${guild}`, {
    icon: uri(png(80, 80, 3)),
    banner: uri(png(160, 90, 4)),
  });
  check(
    'guild icon and banner upload',
    gi.status === 200 && !!gi.body?.icon && !!gi.body?.banner,
    gi,
  );
  const icon = await cdn(`/cdn/icons/${guild}/${gi.body?.icon}.png?size=16`);
  check(
    'guild icon resized via /cdn/icons',
    icon.r.status === 200 && dims(icon.body).join('x') === '16x16',
    icon.r.status,
  );
  const clr = await A.call('PATCH', `/guilds/${guild}`, { icon: null, banner: null });
  check(
    'guild icon and banner removed',
    clr.status === 200 && clr.body?.icon === null && clr.body?.banner === null,
    clr,
  );
  check(
    'guild icon object deleted from S3',
    !(await s3Get(`icons/${guild}/${gi.body?.icon}`)).exists,
  );

  // signed emoji URL endpoint
  const em = await A.call('POST', `/guilds/${guild}/emojis`, {
    name: 'url_' + sfx,
    image: uri(png(48, 48, 5)),
  });
  const eu = await A.call('GET', `/guilds/${guild}/emojis/${em.body?.id}/url?size=16&format=png`);
  check(
    'emoji url endpoint returns a signed variant URL',
    eu.status === 200 && /size=16&format=png&ex=\d+&sig=/.test(eu.body?.url ?? ''),
    eu,
  );
  const ef = await cdn(eu.body?.url ?? '/cdn/none');
  check(
    'signed emoji variant is served resized',
    ef.r.status === 200 && dims(ef.body).join('x') === '16x16',
    ef.r.status,
  );
  await A.call('DELETE', `/guilds/${guild}/emojis/${em.body?.id}`);
}

// 13. RDY-STO-001: a chat attachment above the part size, as the composer sends it (slot with a multipart plan,
// parts straight to S3, GET .../parts, complete), with an abort and a resume. After every step what the server
// says it holds is compared with S3's own ListParts. The browser half (composer, IndexedDB ledger, reload) is
// e2e/tests/storage-composer.spec.ts.
{
  const B = new Client();
  const rb = await B.register(newInvite(), 'mp' + sfx);
  check('multipart: register user', rb.status < 300, rb);
  // A multipart file needs the 500 MiB Patron limit: grant it before the first limit lookup is cached.
  sql(
    `INSERT INTO entitlement_grants (id, kind, user_id, source, permanent, starts_ms, reason) ` +
      `VALUES (floor(random() * 9e15)::bigint, 'patron', ${B.id}, 'admin', true, 0, 'storage-live')`,
  );
  const gb = await B.call('POST', '/guilds', { name: 'Multipart Guild' });
  const chB = (await B.call('GET', `/guilds/${gb.body?.id}/channels`)).body?.find(
    (c) => c.type === 0,
  )?.id;
  check('multipart: have a text channel', !!chB, gb);

  const PART = 64 * MIB;
  const big = randomBytes(2 * PART + 12 * MIB); // 3 parts: 64 + 64 + 12 MiB
  const slotReq = () =>
    B.call('POST', `/channels/${chB}/attachments`, {
      files: [
        {
          id: '1',
          filename: 'big.bin',
          file_size: big.length,
          content_type: 'application/octet-stream',
        },
      ],
    });
  const putPart = async (p) => {
    const start = (p.part_number - 1) * PART;
    const res = await fetch(p.url, {
      method: 'PUT',
      body: big.subarray(start, Math.min(start + PART, big.length)),
    });
    return res.status;
  };
  const store = async (uploadId) => ({
    key: sql(`SELECT storage_key FROM attachments WHERE id = ${uploadId}`),
    mp: sql(`SELECT multipart_id FROM attachment_multipart WHERE attachment_id = ${uploadId}`),
  });
  /** What the server reports (GET .../parts) next to what S3 holds (ListParts). */
  const compare = async (label, uploadId) => {
    const { key, mp } = await store(uploadId);
    const api = await B.call('GET', `/channels/${chB}/attachments/${uploadId}/parts`);
    const s3 = await listParts(S3, key, mp);
    const apiParts = (api.body?.received ?? []).map((x) => `${x.part_number}:${x.size}`).join(',');
    const s3Parts = (s3 ?? []).map((x) => `${x.part}:${x.size}`).join(',');
    check(
      `multipart: ${label}: server parts match S3 ListParts`,
      api.status === 200 && apiParts === s3Parts,
      {
        api: apiParts,
        s3: s3Parts,
      },
    );
    return { api: api.body, key, mp };
  };

  // First slot: send part 1 only, compare, abort.
  let r1 = await slotReq();
  let slot = r1.body?.attachments?.[0];
  check(
    'multipart: slot above the part size carries a 3-part plan',
    r1.status === 200 && slot?.multipart?.part_count === 3 && slot.multipart.part_size === PART,
    r1,
  );
  check('multipart: part 1 accepted by S3', (await putPart(slot.multipart.parts[0])) === 200);
  const first = await compare('after part 1', slot.upload_id);
  check(
    'multipart: one part received, two missing',
    first.api?.received?.length === 1 && first.api?.missing?.length === 2,
    first.api,
  );
  const gone = await B.call('DELETE', `/channels/${chB}/attachments/${slot.upload_id}`);
  check('multipart: abort answers 2xx', gone.status >= 200 && gone.status < 300, gone);
  check(
    'multipart: S3 no longer knows the aborted upload',
    (await listParts(S3, first.key, first.mp)) === null,
  );
  check(
    'multipart: aborted slot is gone from the server',
    (await B.call('GET', `/channels/${chB}/attachments/${slot.upload_id}/parts`)).status === 404,
  );

  // Second slot: parts 1 and 3 out of order, then resume with the missing one, then complete.
  r1 = await slotReq();
  slot = r1.body?.attachments?.[0];
  check('multipart: a fresh slot after the abort', r1.status === 200 && !!slot?.multipart, r1);
  check('multipart: part 3 accepted', (await putPart(slot.multipart.parts[2])) === 200);
  check('multipart: part 1 accepted', (await putPart(slot.multipart.parts[0])) === 200);
  const mid = await compare('before the resume', slot.upload_id);
  const missing = mid.api?.missing ?? [];
  check(
    'multipart: only part 2 is missing',
    missing.length === 1 && missing[0].part_number === 2,
    mid.api,
  );
  check('multipart: the missing part is accepted', (await putPart(missing[0])) === 200);
  const full = await compare('after the resume', slot.upload_id);
  check(
    'multipart: all three parts held',
    full.api?.received?.length === 3 && full.api?.missing?.length === 0,
    full.api,
  );
  const done = await B.call('POST', `/channels/${chB}/attachments/${slot.upload_id}/complete`, {});
  check('multipart: complete answers 2xx', done.status >= 200 && done.status < 300, done);
  const obj = await s3Get(full.key);
  check(
    'multipart: assembled object has the original bytes',
    obj.exists && obj.body.length === big.length && sha(obj.body) === sha(big),
    obj.exists ? obj.body.length : obj,
  );
  check(
    'multipart: the multipart record is cleared',
    sql(`SELECT count(*) FROM attachment_multipart WHERE attachment_id = ${slot.upload_id}`) ===
      '0',
  );
}

console.log(failures ? `\n${failures} check(s) FAILED` : '\nall checks passed');
process.exit(failures ? 1 : 0);
