#!/usr/bin/env node
// The media pipeline smoke (RDY-MED-003, 004, 005): drives a running sr-server, a real sr-transcoder worker
// and a real ffmpeg through the whole Cinema/Live TV path and checks what comes out. CI runs it in
// .github/workflows/media.yml; docs/dev/media-pipeline-test.md says how to run it by hand and how to dry-walk
// it with fakes (MEDIA_SMOKE_DRY=1, no ffmpeg, no server, no Postgres).
//
//   node deploy/scripts/media-smoke.mjs [stage ...]      stages: pipeline retranscode download tv resilience
//                                                        (default: all, in that order)
//
// Stages (each prints PASS/FAIL lines; the exit code is 0 only when every check passed):
//   pipeline     generate a 20 s lavfi source (video, two audio tracks, one subtitle), upload it as the Owner,
//                probe -> review proxy -> planner -> CPU worker; assert the ladder, the audio tracks, the
//                WebVTT, the sprite sheet and its VTT, then fetch the master and media playlists and every
//                segment of one rendition and check them with ffprobe.                         (RDY-MED-003)
//   retranscode  POST /admin/transcode/retranscode for the asset, wait, check the new ladder.      (RDY-MED-004)
//   download     Patron download: remux job, signed file URL, attachment headers, Range, MP4 layout. (RDY-MED-004)
//   tv           sr-cli tv slate, a channel airing the episode with the slate before and after, the
//                numbering rule (D-267, D-285) over the real live segments.                      (RDY-MED-004)
//   resilience   corrupt source fails with logs, two workers claim without overlap, a killed worker's lease
//                expires and the job is retried.                                                  (RDY-MED-005)
//
// Environment (defaults in brackets):
//   MEDIA_SMOKE_BASE [http://127.0.0.1:8092]   MEDIA_SMOKE_ORIGIN [http://localhost:8092]
//   SR_OWNER_PASSWORD (required)               MEDIA_SMOKE_OWNER [owner]
//   MEDIA_SMOKE_DIR [./media-smoke-work]       scratch files, worker logs, the worker token (mode 0600)
//   SR_TRANSCODER_BIN [target/debug/sr-transcoder]   SR_CLI_BIN [target/debug/sr-cli]
//   FFMPEG [ffmpeg]  FFPROBE [ffprobe]  PSQL [psql]  SR_TEST_DATABASE_URL (only for the Patron grant fallback)
//   SR_TEST_S3_ENDPOINT / _BUCKET / _ACCESS_KEY / _SECRET_KEY / _REGION [garage]   (the corrupt-source scenario
//                                                     overwrites the stored original with SigV4)
//   MEDIA_SMOKE_LONG_SECS [120]   length of the source the kill scenario uses
//   MEDIA_SMOKE_DRY=1             dry walk against deploy/scripts/media-fakes.mjs
import { createHash, createHmac, randomBytes } from 'node:crypto';
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  readSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  checkLiveNumbering,
  kindRuns,
  parseMaster,
  parseMedia,
  parseVtt,
  stable,
  topLevelBoxes,
} from './media-lib.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');
const DRY = process.env.MEDIA_SMOKE_DRY === '1';
const env = (k, d) => process.env[k] ?? d;

// ---- process I/O: real, or the fakes of a dry walk -------------------------------------------------------------
const realIo = {
  run(cmd, args, opts = {}) {
    const r = spawnSync(cmd, args, { encoding: 'utf8', maxBuffer: 256 * 1024 * 1024, ...opts });
    return {
      status: r.status ?? (r.error ? 127 : 1),
      stdout: r.stdout ?? '',
      stderr: r.stderr ?? String(r.error ?? ''),
    };
  },
  /** A detached worker in its own process group, so one kill takes ffmpeg with it. */
  spawnWorker({ bin, args, logPath }) {
    const log = openSync(logPath, 'a');
    const child = spawn(bin, args, {
      detached: true,
      stdio: ['ignore', log, log],
      env: process.env,
    });
    let exited = false;
    child.on('exit', () => (exited = true));
    return {
      pid: child.pid,
      kill(sig = 'SIGKILL') {
        try {
          process.kill(-child.pid, sig);
        } catch {
          /* already gone */
        }
      },
      get exited() {
        return exited;
      },
      logPath,
    };
  },
};
let io = realIo;
let fakeServer = null;
if (DRY) {
  const fakes = await import('./media-fakes.mjs');
  fakeServer = await fakes.startFakeServer();
  io = fakes.fakeIo(fakeServer);
  process.env.MEDIA_SMOKE_BASE = fakeServer.base;
  process.env.MEDIA_SMOKE_ORIGIN = fakeServer.base;
  process.env.SR_OWNER_PASSWORD ??= 'dry-walk-not-a-secret';
  process.env.SR_TEST_S3_ENDPOINT = `${fakeServer.base}/fake-s3`;
  process.env.SR_TEST_S3_BUCKET ??= 'dry';
  process.env.SR_TEST_S3_ACCESS_KEY ??= 'dry';
  process.env.SR_TEST_S3_SECRET_KEY ??= 'dry';
  process.env.MEDIA_SMOKE_LONG_SECS ??= '30';
}

const BASE = env('MEDIA_SMOKE_BASE', 'http://127.0.0.1:8092').replace(/\/$/, '');
const API = `${BASE}/api/v1`;
const ORIGIN = env('MEDIA_SMOKE_ORIGIN', 'http://localhost:8092');
const OWNER = env('MEDIA_SMOKE_OWNER', 'owner');
const PASSWORD = process.env.SR_OWNER_PASSWORD;
const DIR = resolve(env('MEDIA_SMOKE_DIR', './media-smoke-work'));
const exe = process.platform === 'win32' ? '.exe' : '';
const TRANSCODER = env(
  'SR_TRANSCODER_BIN',
  resolve(repoRoot, 'target/debug', `sr-transcoder${exe}`),
);
const CLI = env('SR_CLI_BIN', resolve(repoRoot, 'target/debug', `sr-cli${exe}`));
const FFMPEG = env('FFMPEG', 'ffmpeg');
const FFPROBE = env('FFPROBE', 'ffprobe');
const PSQL = env('PSQL', 'psql');
const DB = env('SR_TEST_DATABASE_URL', '');
const LONG_SECS = Number(env('MEDIA_SMOKE_LONG_SECS', '120'));
const POLL_MS = DRY ? 20 : 2000;
const S3 = {
  endpoint: env('SR_TEST_S3_ENDPOINT', '').replace(/\/$/, ''),
  bucket: env('SR_TEST_S3_BUCKET', ''),
  region: env('SR_TEST_S3_REGION', 'garage'),
  ak: env('SR_TEST_S3_ACCESS_KEY', ''),
  sk: env('SR_TEST_S3_SECRET_KEY', ''),
};
if (!PASSWORD) throw new Error('set SR_OWNER_PASSWORD (the Owner of the instance under test)');
mkdirSync(DIR, { recursive: true });

// ---- reporting -------------------------------------------------------------------------------------------------
let failures = 0;
const log = (m) => console.log(`      ${m}`);
function check(name, ok, detail) {
  const shown = ok ? '' : `  -> ${typeof detail === 'string' ? detail : JSON.stringify(detail)}`;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${shown.slice(0, 900)}`);
  if (!ok) failures++;
  return ok;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function until(what, fn, { timeoutMs, everyMs = POLL_MS }) {
  const end = Date.now() + (DRY ? Math.min(timeoutMs, 30000) : timeoutMs);
  let last;
  while (Date.now() < end) {
    last = await fn();
    if (last) return last;
    await sleep(everyMs);
  }
  throw new Error(`timed out waiting for ${what}`);
}

// ---- HTTP ------------------------------------------------------------------------------------------------------
class Client {
  cookie = new Map();
  csrf = '';
  async raw(method, url, { headers = {}, body, redirect = 'follow' } = {}) {
    const h = { Origin: ORIGIN, ...headers };
    if (this.cookie.size) h.Cookie = [...this.cookie].map(([k, v]) => `${k}=${v}`).join('; ');
    if (this.csrf && method !== 'GET' && method !== 'HEAD') h['X-SR-CSRF'] = this.csrf;
    const res = await fetch(new URL(url, `${BASE}/`), { method, headers: h, body, redirect });
    for (const line of res.headers.getSetCookie?.() ?? []) {
      const [pair] = line.split(';');
      const i = pair.indexOf('=');
      const name = pair.slice(0, i).trim();
      const value = pair.slice(i + 1).trim();
      if (value === '' || /;\s*max-age=0/i.test(line)) this.cookie.delete(name);
      else this.cookie.set(name, value);
    }
    return res;
  }
  async call(method, path, body) {
    const res = await this.raw(method, `${API}${path}`, {
      headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await res.text();
    let json = null;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      json = text;
    }
    return { status: res.status, body: json };
  }
  /** The body of a call that must succeed. */
  async must(method, path, body) {
    const r = await this.call(method, path, body);
    if (r.status < 200 || r.status >= 300) {
      throw new Error(`${method} ${path} -> ${r.status}: ${JSON.stringify(r.body).slice(0, 400)}`);
    }
    return r.body;
  }
  async bytes(url) {
    const res = await this.raw('GET', url);
    if (!res.ok) throw new Error(`GET ${stable(String(url))} -> ${res.status}`);
    return Buffer.from(await res.arrayBuffer());
  }
  async text(url) {
    return (await this.bytes(url)).toString('utf8');
  }
}
const owner = new Client();
const urlOf = (u, from) => new URL(u, new URL(from, `${BASE}/`)).toString();

// ---- ffmpeg and ffprobe ----------------------------------------------------------------------------------------
function ffprobe(file) {
  const r = io.run(FFPROBE, ['-v', 'error', '-show_streams', '-show_format', '-of', 'json', file]);
  if (r.status !== 0) throw new Error(`ffprobe ${file}: ${r.stderr.slice(0, 300)}`);
  const j = JSON.parse(r.stdout);
  const streams = j.streams ?? [];
  return {
    streams,
    format: j.format ?? {},
    of: (type) => streams.filter((s) => s.codec_type === type),
    duration: Number(j.format?.duration ?? 0),
  };
}
function ffmpeg(args) {
  const r = io.run(FFMPEG, ['-hide_banner', '-nostdin', '-y', '-loglevel', 'error', ...args]);
  if (r.status !== 0) throw new Error(`ffmpeg ${args.slice(-1)[0]}: ${r.stderr.slice(0, 600)}`);
}
const SRT = `1
00:00:01,000 --> 00:00:03,000
Hello from the CI source

2
00:00:05,000 --> 00:00:08,000
Second cue

3
00:00:10,000 --> 00:00:14,000
Third cue
`;
/** The source the planner pins in crates/sr-transcoder/src/ci_pins.rs: Matroska, 1280x720 at 25 fps, two
 * stereo AAC tracks (eng, spa), one SubRip subtitle (eng). */
function makeSource(path, secs, { subtitle = true, size = '1280x720' } = {}) {
  const srt = join(DIR, 'subs.srt');
  writeFileSync(srt, SRT);
  const args = [
    '-f',
    'lavfi',
    '-i',
    `testsrc2=size=${size}:rate=25:duration=${secs}`,
    '-f',
    'lavfi',
    '-i',
    `sine=frequency=440:sample_rate=48000:duration=${secs}`,
    '-f',
    'lavfi',
    '-i',
    `sine=frequency=660:sample_rate=48000:duration=${secs}`,
  ];
  if (subtitle) args.push('-i', srt);
  args.push('-map', '0:v', '-map', '1:a', '-map', '2:a');
  if (subtitle) args.push('-map', '3:s');
  args.push(
    '-c:v',
    'libx264',
    '-preset',
    'veryfast',
    '-g',
    '50',
    '-pix_fmt',
    'yuv420p',
    '-c:a',
    'aac',
    '-b:a',
    '96k',
    '-metadata:s:a:0',
    'language=eng',
    '-metadata:s:a:0',
    'title=English',
    '-metadata:s:a:1',
    'language=spa',
    '-metadata:s:a:1',
    'title=Spanish',
    '-disposition:a:0',
    'default',
    '-disposition:a:1',
    '0',
  );
  if (subtitle)
    args.push('-c:s', 'srt', '-metadata:s:s:0', 'language=eng', '-metadata:s:s:0', 'title=English');
  args.push('-t', String(secs), path);
  ffmpeg(args);
  const p = ffprobe(path);
  if (
    p.of('video').length !== 1 ||
    p.of('audio').length !== 2 ||
    p.of('subtitle').length !== (subtitle ? 1 : 0)
  ) {
    throw new Error(
      `the generated source has the wrong streams: ${p.streams.map((s) => s.codec_type).join(',')}`,
    );
  }
  return statSync(path).size;
}

// ---- S3 SigV4 query presign (path-style), only to replace a stored original ------------------------------------
const hmac = (k, s) => createHmac('sha256', k).update(s).digest();
const sha = (s) => createHash('sha256').update(s).digest('hex');
const enc = (s) =>
  encodeURIComponent(s).replace(
    /[!'()*]/g,
    (c) => '%' + c.charCodeAt(0).toString(16).toUpperCase(),
  );
function presign(method, key, expires = 120) {
  const u = new URL(S3.endpoint);
  const now = new Date().toISOString().replace(/[:-]|\.\d{3}/g, '');
  const date = now.slice(0, 8);
  const scope = `${date}/${S3.region}/s3/aws4_request`;
  const prefix = u.pathname.replace(/\/$/, '');
  const path = prefix + '/' + [S3.bucket, ...key.split('/')].map(enc).join('/');
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

// ---- workers ---------------------------------------------------------------------------------------------------
const workers = new Map();
let tokenFile = '';
async function ensureToken() {
  if (tokenFile) return;
  const t = await owner.must('POST', '/admin/transcode/tokens', {
    name: `media-smoke-${Date.now() % 100000}`,
  });
  tokenFile = join(DIR, 'worker.token');
  writeFileSync(tokenFile, t.secret, { mode: 0o600 });
}
async function startWorker(name, concurrency = 1) {
  await ensureToken();
  const h = io.spawnWorker({
    bin: TRANSCODER,
    args: [
      '--server',
      BASE,
      '--token-file',
      tokenFile,
      '--concurrency',
      String(concurrency),
      '--work-dir',
      join(DIR, `work-${name}`),
      '--name',
      name,
      '--force-cpu',
    ],
    logPath: join(DIR, `worker-${name}.log`),
    name,
  });
  workers.set(name, h);
  return h;
}
function killWorker(name, sig = 'SIGKILL') {
  workers.get(name)?.kill(sig);
  workers.delete(name);
}
function killAllWorkers() {
  for (const n of [...workers.keys()]) killWorker(n);
}
async function workerViews() {
  return owner.must('GET', '/admin/transcode/workers');
}
async function waitWorkerOnline(name) {
  return until(
    `worker ${name} to register`,
    async () => (await workerViews()).find((w) => w.name === name && w.online),
    {
      timeoutMs: 60000,
    },
  );
}

// ---- assets and jobs -------------------------------------------------------------------------------------------
async function uploadFile(path, { episodeId, filename } = {}) {
  const size = statSync(path).size;
  const slot = await owner.must('POST', '/admin/cinema/assets', {
    filename: filename ?? path.split(/[\\/]/).pop(),
    size_bytes: size,
    ...(episodeId ? { episode_id: episodeId } : {}),
  });
  const fd = openSync(path, 'r');
  const parts = [];
  try {
    for (const p of slot.parts) {
      const offset = (p.part_number - 1) * slot.part_size;
      const len = Math.min(slot.part_size, size - offset);
      const buf = Buffer.alloc(len);
      readSync(fd, buf, 0, len, offset);
      const res = await fetch(urlOf(p.url, BASE), { method: 'PUT', body: buf });
      if (!res.ok) throw new Error(`part ${p.part_number} PUT -> ${res.status}`);
      parts.push({ part_number: p.part_number, etag: res.headers.get('etag') ?? '' });
    }
  } finally {
    closeSync(fd);
  }
  await owner.must('POST', `/admin/cinema/assets/${slot.asset.id}/complete`, {
    upload_id: slot.upload_id,
    parts,
  });
  return {
    id: slot.asset.id,
    size,
    key: `originals/${slot.asset.id}/source.${(filename ?? path).split('.').pop()}`,
  };
}
const assetStatus = (id) => owner.must('GET', `/admin/cinema/assets/${id}`);
const jobDetail = (jobId) => owner.must('GET', `/admin/transcode/jobs/${jobId}`);
const TERMINAL = new Set(['succeeded', 'failed', 'cancelled']);
async function dumpJobs(assetId) {
  try {
    const st = await assetStatus(assetId);
    for (const j of st.jobs) {
      const d = await jobDetail(j.job_id);
      log(`job ${j.job_id} ${j.kind} ${j.state} attempt ${j.attempt}: ${j.error ?? ''}`);
      if (d.log_tail) log(`log tail:\n${d.log_tail.split('\n').slice(-25).join('\n')}`);
    }
  } catch (e) {
    log(`could not read the jobs of asset ${assetId}: ${e.message}`);
  }
}
/** Wait until the ladder of an asset is done: playable, or a job failed (then the jobs are dumped). */
async function waitPlayable(assetId, timeoutMs = 20 * 60000) {
  try {
    return await until(
      `asset ${assetId} to become playable`,
      async () => {
        const st = await assetStatus(assetId);
        if (st.asset.state === 'failed') throw new Error(`asset failed: ${st.asset.error}`);
        const bad = st.jobs.find((j) => j.state === 'failed' || j.state === 'cancelled');
        if (bad)
          throw new Error(`job ${bad.job_id} (${bad.kind}) ${bad.state}: ${bad.error ?? ''}`);
        const allDone = st.jobs.length >= 2 && st.jobs.every((j) => j.state === 'succeeded');
        return st.asset.playable && allDone ? st : null;
      },
      { timeoutMs },
    );
  } catch (e) {
    await dumpJobs(assetId);
    throw e;
  }
}

// ---- shared state between stages -------------------------------------------------------------------------------
const S = {};
const EP_DURATION = 20;

async function signIn() {
  const login = await owner.must('POST', '/sr/auth/login', { login: OWNER, password: PASSWORD });
  if (login?.status !== 'ok') throw new Error(`sign-in did not complete: ${JSON.stringify(login)}`);
  owner.csrf = (await owner.must('GET', '/sr/csrf')).token;
  if (!owner.csrf) throw new Error('no CSRF token');
}

// ==== stage: pipeline ===========================================================================================
/** Segments of one media playlist, every one fetched and probed. */
async function checkRendition(label, playlistUrl, { kind, width, height, expectSecs }) {
  const pl = parseMedia(await owner.text(playlistUrl));
  check(
    `${label}: VOD playlist with an end marker and an init segment`,
    pl.endlist && !!pl.map,
    pl,
  );
  const init = await owner.bytes(urlOf(pl.map, playlistUrl));
  check(
    `${label}: init segment is a non-empty MP4`,
    init.length > 100 && topLevelBoxes(init).includes('moov'),
    init.length,
  );
  const expectedSegs = Math.ceil(expectSecs / 6);
  check(
    `${label}: ${expectedSegs} segments of at most 6 s`,
    Math.abs(pl.segments.length - expectedSegs) <= 1 && pl.segments.every((s) => s.duration <= 6.5),
    pl.segments.map((s) => s.duration),
  );
  let total = 0;
  let allOk = true;
  let firstBad = '';
  const parts = [init];
  for (const [i, seg] of pl.segments.entries()) {
    const body = await owner.bytes(urlOf(seg.uri, playlistUrl));
    parts.push(body);
    const f = join(DIR, `seg-${label.replace(/\W+/g, '-')}-${i}.mp4`);
    writeFileSync(f, Buffer.concat([init, body]));
    try {
      const p = ffprobe(f);
      const s = p.of(kind)[0];
      const ok =
        !!s &&
        (kind !== 'video' || (s.codec_name === 'h264' && s.width === width && s.height === height));
      if (!ok && allOk)
        firstBad = `segment ${i}: ${JSON.stringify(p.streams.map((x) => [x.codec_type, x.codec_name, x.width, x.height]))}`;
      allOk &&= ok;
      total += p.duration;
    } catch (e) {
      if (allOk) firstBad = `segment ${i}: ${e.message}`;
      allOk = false;
    }
  }
  check(
    `${label}: every segment decodes as ${kind}${kind === 'video' ? ` h264 ${width}x${height}` : ''} (ffprobe)`,
    allOk,
    firstBad,
  );
  check(
    `${label}: segment durations add up to the source (${expectSecs} s)`,
    Math.abs(total - expectSecs) < 1.5,
    total,
  );
  const whole = join(DIR, `whole-${label.replace(/\W+/g, '-')}.mp4`);
  writeFileSync(whole, Buffer.concat(parts));
  try {
    check(
      `${label}: the segments joined are one stream of ${expectSecs} s`,
      Math.abs(ffprobe(whole).duration - expectSecs) < 1.5,
      ffprobe(whole).duration,
    );
  } catch (e) {
    check(`${label}: the segments joined are one stream`, false, e.message);
  }
}

async function validateVod(playback) {
  const masterUrl = urlOf(playback.master_url, BASE);
  const master = parseMaster(await owner.text(masterUrl));
  const heights = master.variants.map((v) => v.height);
  check(
    'master: the ladder is 720p and 480p (1080p is never made for a 720p source)',
    JSON.stringify(heights) === '[720,480]',
    heights,
  );
  check(
    'master: every variant names codecs, bandwidth and an audio group',
    master.variants.every((v) => v.attrs.CODECS && v.bandwidth > 0 && v.attrs.AUDIO),
    master.variants.map((v) => v.attrs),
  );
  const audio = master.media.filter((m) => m.TYPE === 'AUDIO');
  check(
    'master: two audio renditions, eng and spa, exactly one default',
    audio.length === 2 &&
      audio
        .map((a) => a.LANGUAGE)
        .sort()
        .join() === 'eng,spa' &&
      audio.filter((a) => a.DEFAULT === 'YES').length === 1,
    audio,
  );
  const subs = master.media.filter((m) => m.TYPE === 'SUBTITLES');
  check(
    'master: one subtitle rendition (eng)',
    subs.length === 1 && subs[0].LANGUAGE === 'eng',
    subs,
  );
  check(
    'playback: two audio tracks and one subtitle track',
    playback.audio_tracks?.length === 2 && playback.subtitles?.length === 1,
    { a: playback.audio_tracks, s: playback.subtitles },
  );

  // WebVTT
  const vttUrl = urlOf(playback.subtitles[0].vtt_url, BASE);
  const vtt = await owner.text(vttUrl);
  let vttCues = [];
  try {
    vttCues = parseVtt(vtt);
  } catch (e) {
    check('subtitles: WebVTT file', false, e.message);
  }
  check(
    'subtitles: WebVTT with the three cues of the source',
    vttCues.length === 3 && vttCues[0].text.includes('Hello from the CI source'),
    vtt.slice(0, 300),
  );
  if (subs[0]) {
    const subPl = parseMedia(await owner.text(urlOf(subs[0].URI, masterUrl)));
    check('subtitles: the subtitle playlist lists the VTT', subPl.segments.length >= 1, subPl);
  }

  // Sprite sheet and its VTT
  check(
    'sprite: the playback info describes a 160x90 sheet, one frame per 10 s',
    playback.sprite?.thumb_width === 160 &&
      playback.sprite?.interval_secs === 10 &&
      playback.sprite?.thumbs_total === 2,
    playback.sprite,
  );
  const spriteVtt = await owner.text(urlOf(playback.sprites_url, BASE));
  const spriteCues = (() => {
    try {
      return parseVtt(spriteVtt);
    } catch {
      return [];
    }
  })();
  check(
    'sprite: the VTT has two cues with #xywh=x,y,160,90 regions',
    spriteCues.length === 2 &&
      spriteCues.every((c) => /#xywh=\d+,\d+,160,90\s*$/.test(c.text.trim())),
    spriteVtt.slice(0, 400),
  );
  if (spriteCues[0]) {
    const sheetUrl = urlOf(
      spriteCues[0].text.trim().split('#')[0],
      urlOf(playback.sprites_url, BASE),
    );
    const sheet = await owner.bytes(sheetUrl);
    check(
      'sprite: the sheet is a JPEG',
      sheet[0] === 0xff && sheet[1] === 0xd8,
      sheet.subarray(0, 4).toString('hex'),
    );
    const f = join(DIR, 'sprite-sheet.jpg');
    writeFileSync(f, sheet);
    const v = ffprobe(f).of('video')[0];
    check(
      'sprite: the sheet is 10 x 10 tiles of 160x90 (ffprobe)',
      v?.width === 1600 && v?.height === 900,
      v && [v.width, v.height],
    );
  }

  // Every segment of the 720p rendition and of the default audio rendition
  const v720 = master.variants.find((v) => v.height === 720);
  if (v720)
    await checkRendition('video 720p', urlOf(v720.uri, masterUrl), {
      kind: 'video',
      width: 1280,
      height: 720,
      expectSecs: EP_DURATION,
    });
  const a0 = audio.find((a) => a.DEFAULT === 'YES') ?? audio[0];
  if (a0)
    await checkRendition('audio default', urlOf(a0.URI, masterUrl), {
      kind: 'audio',
      expectSecs: EP_DURATION,
    });
  return master;
}

async function stagePipeline() {
  console.log('== pipeline: probe, review proxy, planner, CPU worker, playback (RDY-MED-003)');
  const src = join(DIR, 'ci-source.mkv');
  const size = makeSource(src, EP_DURATION);
  log(`source ${src} (${size} bytes)`);

  // The catalog item the video is for.
  const title = await owner.must('POST', '/admin/cinema/titles', {
    kind: 'series',
    title: 'Media Smoke Series',
  });
  const season = await owner.must('POST', `/admin/cinema/titles/${title.id}/seasons`, {
    number: 1,
    name: 'Season 1',
  });
  const episode = await owner.must('POST', `/admin/cinema/seasons/${season.id}/episodes`, {
    number: 1,
    title: 'Pilot',
    runtime_min: 1,
  });
  S.titleId = title.id;
  S.episodeId = episode.id;

  await startWorker('ci-main', 2);
  const w = await waitWorkerOnline('ci-main');
  check(
    'worker: registered, online, CPU path (libx264, no NVENC)',
    w.capabilities.encoders.includes('libx264') && w.capabilities.nvenc === false,
    w.capabilities,
  );
  log(`ffmpeg ${w.capabilities.ffmpeg_version}`);

  const up = await uploadFile(src, { episodeId: episode.id, filename: 'ci-source.mkv' });
  S.assetId = up.id;
  const st = await waitPlayable(up.id);
  check(
    'probe: container, size, languages and the subtitle are read from the file',
    /matroska/.test(st.probe?.container ?? '') &&
      st.probe?.width === 1280 &&
      st.probe?.height === 720 &&
      st.probe?.video_codec === 'h264' &&
      st.probe?.audio_languages?.join() === 'eng,spa' &&
      st.probe?.subtitle_count === 1 &&
      Math.abs((st.probe?.duration_secs ?? 0) - EP_DURATION) < 0.5,
    st.probe,
  );
  check(
    'review proxy: made (asset has_proxy) by a succeeded proxy job',
    st.asset.has_proxy && st.jobs.some((j) => j.kind === 'proxy' && j.state === 'succeeded'),
    st.jobs,
  );
  const ladder = st.jobs.find((j) => j.kind === 'ladder');
  const ld = await jobDetail(ladder.job_id);
  check(
    'ladder: succeeded first time on libx264 with its outputs',
    ladder.state === 'succeeded' &&
      ladder.attempt === 1 &&
      ld.encoder === 'libx264' &&
      ld.output_count > 10,
    ld,
  );
  if (ld.warnings.length) log(`planner warnings: ${ld.warnings.join('; ')}`);
  S.durationMs = st.asset.duration_ms;

  // Link, publish, play.
  await owner.must('PUT', `/admin/cinema/assets/${up.id}/link`, {
    episode_id: episode.id,
    set_playable: true,
  });
  await owner.must('POST', `/admin/cinema/episodes/${episode.id}/publish`);
  await owner.must('POST', `/admin/cinema/titles/${title.id}/publish`);
  const playback = await owner.must('GET', `/cinema/assets/${up.id}/playback`);
  S.playback = playback;
  await validateVod(playback);
}

// ==== stage: retranscode ========================================================================================
async function stageRetranscode() {
  console.log('== retranscode: POST /admin/transcode/retranscode (RDY-MED-004)');
  if (!S.assetId)
    return check('retranscode: needs the pipeline stage', false, 'run the pipeline stage first');
  const before = await assetStatus(S.assetId);
  const knownJobs = new Set(before.jobs.map((j) => j.job_id));
  const out = await owner.must('POST', '/admin/transcode/retranscode', {
    scope: 'asset',
    id: S.assetId,
    reason: 'media smoke',
  });
  check(
    'retranscode: one job enqueued, none skipped or failed',
    out.enqueued.length === 1 && out.skipped.length === 0 && out.failed.length === 0,
    out,
  );
  const jobId = out.enqueued[0];
  await until(
    'the re-transcode job',
    async () => {
      const d = await jobDetail(jobId);
      if (d.state === 'failed' || d.state === 'cancelled')
        throw new Error(`re-transcode ${d.state}: ${d.error}\n${d.log_tail ?? ''}`);
      return d.state === 'succeeded';
    },
    { timeoutMs: 15 * 60000 },
  );
  check('retranscode: the job is new, not the first ladder', jobId && !knownJobs.has(jobId), jobId);
  const playback = await owner.must('GET', `/cinema/assets/${S.assetId}/playback`);
  const after = await assetStatus(S.assetId);
  const ladders = after.jobs.filter((j) => j.kind === 'ladder' && j.state === 'succeeded');
  check(
    'retranscode: the asset has two succeeded ladder jobs and stays playable',
    after.asset.playable && ladders.length === 2,
    { jobs: after.jobs.map((j) => [j.kind, j.state]), playable: after.asset.playable },
  );
  const nd = await jobDetail(jobId);
  check(
    'retranscode: the new ladder ran on libx264 and published its outputs',
    nd.encoder === 'libx264' && nd.output_count > 10,
    nd,
  );
  await validateVod(playback);
}

// ==== stage: download (Patron) ==================================================================================
async function grantPatron() {
  let me = await owner.call('GET', '/users/@me');
  let id = me.body?.id;
  if (!id && DB) {
    id = io
      .run(PSQL, [
        DB,
        '-X',
        '-A',
        '-t',
        '-c',
        `SELECT id FROM accounts WHERE username = '${OWNER}'`,
      ])
      .stdout.trim();
  }
  if (!id) throw new Error('cannot find the Owner account id');
  const r = await owner.call('POST', '/admin/donations/entitlements', {
    kind: 'patron',
    user_id: String(id),
    permanent: true,
    reason: 'media smoke: Patron download check',
  });
  if (r.status >= 200 && r.status < 300) return 'api';
  if (!DB)
    throw new Error(
      `granting Patron through the API failed (${r.status}) and no SR_TEST_DATABASE_URL for the fallback`,
    );
  const sql = `INSERT INTO entitlement_grants (id, kind, user_id, source, permanent, starts_ms, reason)
    VALUES (9000000000000001, 'patron', ${id}, 'admin', true, ${Date.now()}, 'media smoke') ON CONFLICT DO NOTHING`;
  const p = io.run(PSQL, [DB, '-X', '-c', sql]);
  if (p.status !== 0) throw new Error(`Patron grant by SQL failed: ${p.stderr}`);
  return 'sql';
}

async function stageDownload() {
  console.log('== download: Patron download object and headers (RDY-MED-004)');
  if (!S.assetId)
    return check('download: needs the pipeline stage', false, 'run the pipeline stage first');
  const nope = await owner.call(
    'GET',
    `/cinema/titles/${S.titleId}/downloads?episode=${S.episodeId}`,
  );
  check('download: refused (403) before the member is a Patron', nope.status === 403, nope);
  const how = await grantPatron();
  log(`Patron granted by ${how}`);
  const opts = await owner.must(
    'GET',
    `/cinema/titles/${S.titleId}/downloads?episode=${S.episodeId}`,
  );
  const rend = opts.renditions.find((r) => r.id !== 'original') ?? opts.renditions[0];
  check(
    'download: the picker lists the stored renditions with sizes',
    opts.renditions.length >= 1 && rend && rend.size_bytes > 0,
    opts,
  );
  const job = await owner.must('POST', `/cinema/titles/${S.titleId}/downloads`, {
    rendition_id: rend.id,
    episode_id: S.episodeId,
  });
  const ready = await until(
    'the download file (remux job)',
    async () => {
      const j = await owner.must('GET', `/cinema/downloads/${job.id}`);
      if (j.state === 'failed') throw new Error(`download failed: ${j.error}`);
      return j.state === 'ready' ? j : null;
    },
    { timeoutMs: 10 * 60000 },
  );
  check(
    'download: ready with a link and an expiry',
    !!ready.url && ready.expires_at_ms > Date.now(),
    ready,
  );
  const redirect = await owner.raw('GET', `${API}/cinema/downloads/${job.id}/file`, {
    redirect: 'manual',
  });
  const location = redirect.headers.get('location') ?? '';
  check(
    'download: /file redirects (307) to a signed URL with dl=1',
    redirect.status === 307 && /[?&]dl=1(&|$)/.test(location),
    { status: redirect.status, location: stable(location) },
  );
  const file = await owner.raw('GET', location);
  const body = Buffer.from(await file.arrayBuffer());
  const h = (n) => file.headers.get(n) ?? '';
  check(
    'download headers: 200, video/mp4, attachment with a .mp4 name, nosniff',
    file.status === 200 &&
      /^video\/mp4/.test(h('content-type')) &&
      /^attachment;/.test(h('content-disposition')) &&
      /\.mp4/i.test(h('content-disposition')) &&
      h('x-content-type-options') === 'nosniff',
    {
      status: file.status,
      type: h('content-type'),
      disposition: h('content-disposition'),
      nosniff: h('x-content-type-options'),
    },
  );
  check(
    'download headers: length matches, byte ranges offered',
    Number(h('content-length')) === body.length && /bytes/.test(h('accept-ranges')),
    {
      length: h('content-length'),
      got: body.length,
      ranges: h('accept-ranges'),
    },
  );
  const ranged = await owner.raw('GET', location, { headers: { Range: 'bytes=0-99' } });
  check(
    'download headers: Range answers 206 with Content-Range',
    ranged.status === 206 && /^bytes 0-99\//.test(ranged.headers.get('content-range') ?? ''),
    {
      status: ranged.status,
      range: ranged.headers.get('content-range'),
    },
  );
  const boxes = topLevelBoxes(body, 1 << 20);
  check(
    'download file: faststart layout (moov before mdat)',
    boxes.includes('moov') && boxes.indexOf('moov') < boxes.indexOf('mdat'),
    boxes,
  );
  const f = join(DIR, 'download.mp4');
  writeFileSync(f, body);
  const p = ffprobe(f);
  check(
    'download file: H.264 video, two AAC tracks, the length of the episode (ffprobe)',
    p.of('video')[0]?.codec_name === 'h264' &&
      p.of('video')[0]?.height === 720 &&
      p.of('audio').length === 2 &&
      p.of('audio').every((a) => a.codec_name === 'aac') &&
      Math.abs(p.duration - EP_DURATION) < 1.5,
    p.streams.map((s) => [s.codec_type, s.codec_name, s.height]),
  );
  const remux = await until(
    'the remux job to be recorded as done',
    async () => {
      const jobs = await owner.must(
        'GET',
        `/admin/transcode/jobs?kind=remux&asset_id=${S.assetId}`,
      );
      return jobs.some((j) => j.state === 'succeeded') ? jobs : null;
    },
    { timeoutMs: 60000 },
  ).catch(() => null);
  check('download: made by a succeeded remux worker job (stream copy)', !!remux, remux);
  const again = await owner.must(
    'GET',
    `/cinema/titles/${S.titleId}/downloads?episode=${S.episodeId}`,
  );
  check(
    'download: the daily allowance went down',
    again.daily_remaining === null ||
      opts.daily_remaining === null ||
      again.daily_remaining < opts.daily_remaining,
    { before: opts.daily_remaining, after: again.daily_remaining },
  );
}

// ==== stage: tv (slate, channel, numbering) =====================================================================
async function stageTv() {
  console.log(
    '== tv: slate, scheduled channel, numbering across the boundary (RDY-MED-004; D-267, D-285)',
  );
  if (!S.assetId)
    return check('tv: needs the pipeline stage', false, 'run the pipeline stage first');
  if (!workers.has('ci-main')) await startWorker('ci-main', 2);

  const r = io.run(
    CLI,
    ['tv', 'slate', '--seconds', '12', '--ffmpeg', FFMPEG, '--wait-secs', '900'],
    { env: process.env },
  );
  const out = r.stdout.trim();
  const slateId = out.split(/\r?\n/).pop().trim();
  check(
    'slate: sr-cli tv slate made, transcoded and registered it',
    r.status === 0 && /^\d+$/.test(slateId),
    { status: r.status, out: out.slice(-300), err: r.stderr.slice(-400) },
  );
  if (!/^\d+$/.test(slateId)) return;
  const slate = await owner.must('GET', '/admin/tv/slate');
  check(
    'slate: Admin > Live TV reports it configured and playable',
    slate.configured && slate.playable && String(slate.asset_id) === slateId,
    slate,
  );
  const slateAsset = await assetStatus(slateId);
  check(
    'slate: a system-owned asset outside the catalogue with a full ladder',
    slateAsset.asset.playable &&
      !slateAsset.asset.title_id &&
      slateAsset.jobs.some((j) => j.kind === 'ladder' && j.state === 'succeeded'),
    slateAsset.asset,
  );

  const channel = await owner.must('POST', '/admin/tv/channels', {
    number: 901,
    name: 'Media Smoke',
    kind: 'manual',
    filler_policy: 'slate',
  });
  const leadMs = DRY ? 400 : 45000;
  const start = Date.now() + leadMs;
  const block = await owner.must('POST', `/admin/tv/channels/${channel.id}/blocks`, {
    title_id: S.titleId,
    episode_id: S.episodeId,
    start_ms: start,
    end_ms: start + S.durationMs,
  });
  check(
    'channel: the episode is placed on the schedule',
    block.episode_id === S.episodeId && block.end_ms - block.start_ms === S.durationMs,
    block,
  );
  // Live segments are /cdn/hls/<asset>/..., so the asset id says which programme a number belongs to.
  const kindOf = (g) => {
    const u = stable(urlOf(g.uri, BASE));
    return u.includes(`/cdn/hls/${S.assetId}/`)
      ? 'episode'
      : u.includes(`/cdn/hls/${slateId}/`)
        ? 'slate'
        : 'other';
  };

  // Sample the live window every couple of seconds from before the programme until after it.
  const samples = [];
  const states = [];
  const stop = start + S.durationMs + (DRY ? 600 : 30000);
  let video = null;
  let audio = null;
  const segFetched = { episode: false, slate: false };
  while (Date.now() < stop) {
    const tune = await owner.must('GET', `/tv/channels/${channel.id}/tune`);
    states.push(`${tune.state}${tune.slate ? '+slate' : ''}`);
    if (tune.master_url) {
      const masterUrl = urlOf(tune.master_url, BASE);
      const master = parseMaster(await owner.text(masterUrl));
      const v = master.variants.find((x) => x.height === 720) ?? master.variants[0];
      video = urlOf(v.uri, masterUrl);
      const a = master.media.find((m) => m.TYPE === 'AUDIO');
      audio = a ? urlOf(a.URI, masterUrl) : null;
      const media = parseMedia(await owner.text(video));
      samples.push({ at: Date.now(), media });
      const newest = media.segments[media.segments.length - 1];
      const k = newest && kindOf(newest);
      if (k && !segFetched[k]) {
        segFetched[k] = true;
        // The real segment on air: init + segment decode as H.264 720p (or the slate's rung).
        const init = await owner.bytes(urlOf(newest.map, video));
        const seg = await owner.bytes(urlOf(newest.uri, video));
        const f = join(DIR, `live-${k}.mp4`);
        writeFileSync(f, Buffer.concat([init, seg]));
        const p = ffprobe(f).of('video')[0];
        check(
          `live: the ${k} segment on air (#${newest.seq}) decodes as H.264`,
          p?.codec_name === 'h264',
          p,
        );
        if (audio) {
          const am = parseMedia(await owner.text(audio));
          const an = am.segments[am.segments.length - 1];
          check(
            `live: audio and video agree on the number on air during the ${k} (#${newest.seq})`,
            !!an && Math.abs(an.seq - newest.seq) <= 1,
            { video: newest.seq, audio: an?.seq },
          );
        }
      }
    }
    await sleep(DRY ? 40 : 2000);
  }
  const seq = states.filter((s, i) => i === 0 || s !== states[i - 1]);
  check(
    'channel: off air with the slate, then live, then the slate again',
    JSON.stringify(seq) === JSON.stringify(['off_air+slate', 'live', 'off_air+slate']),
    seq,
  );
  const runs = kindRuns(samples, kindOf);
  check(
    'numbering: slate, then the episode, then the slate',
    JSON.stringify(runs) === JSON.stringify(['slate', 'episode', 'slate']),
    runs,
  );
  const problems = checkLiveNumbering(samples, kindOf);
  check(
    'numbering: one chain across both boundaries, no jump, no renumbering, discontinuities marked (D-267, D-285)',
    problems.length === 0,
    problems.slice(0, 5),
  );
  const now = await owner.must('GET', `/admin/tv/channels/${channel.id}/now`);
  check(
    'channel: Admin now-view agrees the programme is over',
    now.current === null || now.current === undefined,
    now.current,
  );
}

// ==== stage: resilience =========================================================================================
async function stageResilience() {
  console.log('== resilience: corrupt source, two workers, killed worker (RDY-MED-005)');
  killAllWorkers();
  await sleep(DRY ? 20 : 3000);

  // 1. A corrupt original. The probe runs in the server, so the file is good until the worker would download
  //    it: upload a real clip, wait for the jobs to queue, replace the stored object by random bytes of the same
  //    size and then start a worker.
  const small = join(DIR, 'corrupt-source.mkv');
  makeSource(small, 4, { size: '640x360' });
  const bad = await uploadFile(small, { filename: 'corrupt-source.mkv' });
  await until('the jobs to be queued', async () => (await assetStatus(bad.id)).jobs.length >= 2, {
    timeoutMs: 120000,
  });
  const put = await fetch(presign('PUT', bad.key), { method: 'PUT', body: randomBytes(bad.size) });
  check(
    'corrupt: the stored original was replaced by random bytes of the same size',
    put.ok,
    put.status,
  );
  await startWorker('ci-corrupt', 1);
  const failed = await until(
    'the corrupt jobs to fail',
    async () => {
      const st = await assetStatus(bad.id);
      return st.jobs.length >= 2 && st.jobs.every((j) => TERMINAL.has(j.state)) ? st : null;
    },
    { timeoutMs: 5 * 60000 },
  );
  const details = await Promise.all(failed.jobs.map((j) => jobDetail(j.job_id)));
  check(
    'corrupt: both jobs failed, once each (a bad file is not retried)',
    details.length === 2 && details.every((d) => d.state === 'failed' && d.attempt === 1),
    details.map((d) => [d.kind, d.state, d.attempt]),
  );
  check(
    'corrupt: the job carries the message and the ffmpeg log (Admin > Transcoding)',
    details.every(
      (d) =>
        /could not be decoded/.test(d.error ?? '') &&
        /\$ ffmpeg /.test(d.log_tail ?? '') &&
        /Invalid data found/.test(d.log_tail ?? ''),
    ),
    details.map((d) => [d.error, (d.log_tail ?? '').slice(0, 200)]),
  );
  check(
    'corrupt: the asset is failed with a reason',
    failed.asset.state === 'failed' && !!failed.asset.error,
    failed.asset,
  );
  const q = await owner.must('GET', '/admin/transcode/queue');
  check('corrupt: the queue summary counts the failures', q.failed_last_day >= 2, q);
  killWorker('ci-corrupt');

  // 2. Two workers, no overlap.
  await sleep(DRY ? 20 : 2000);
  await startWorker('ci-a', 1);
  await startWorker('ci-b', 1);
  const wa = await waitWorkerOnline('ci-a');
  const wb = await waitWorkerOnline('ci-b');
  const clips = [];
  for (let i = 0; i < 4; i++) {
    const f = join(DIR, `clip-${i}.mkv`);
    makeSource(f, 4, { size: '640x360' });
    clips.push(await uploadFile(f, { filename: `clip-${i}.mkv` }));
  }
  const stats = [];
  for (const c of clips) stats.push(await waitPlayable(c.id));
  const all = stats.flatMap((s) => s.jobs);
  check(
    'two workers: all 8 jobs (proxy + ladder per clip) succeeded on the first attempt',
    all.length === 8 && all.every((j) => j.state === 'succeeded' && j.attempt === 1),
    all.map((j) => [j.kind, j.state, j.attempt]),
  );
  const ids = new Set(all.map((j) => j.job_id));
  check(
    'two workers: every job was claimed once and ran on exactly one of the two workers',
    ids.size === 8 && all.every((j) => [wa.id, wb.id].includes(j.worker_id)),
    all.map((j) => j.worker_id),
  );
  const byWorker = new Map();
  for (const j of all) byWorker.set(j.worker_id, [...(byWorker.get(j.worker_id) ?? []), j]);
  let overlap = false;
  for (const list of byWorker.values()) {
    const s = list
      .filter((j) => j.started_at_ms && j.finished_at_ms)
      .sort((x, y) => x.started_at_ms - y.started_at_ms);
    for (let i = 1; i < s.length; i++)
      if (s[i].started_at_ms < s[i - 1].finished_at_ms - 1000) overlap = true;
  }
  check(
    'two workers: a worker with concurrency 1 never ran two jobs at once',
    !overlap,
    [...byWorker].map(([w, l]) => [w, l.length]),
  );
  if (byWorker.size < 2)
    log('note: one worker took every job (fast machine); the claims still did not overlap');
  const views = await workerViews();
  check(
    'two workers: both idle at the end',
    views.filter((w) => ['ci-a', 'ci-b'].includes(w.name)).every((w) => w.active_jobs === 0),
    views.map((w) => [w.name, w.active_jobs]),
  );
  killWorker('ci-a');
  killWorker('ci-b');

  // 3. Kill a worker mid-job: the lease expires, the job is retried.
  await sleep(DRY ? 20 : 2000);
  await startWorker('ci-victim', 1);
  const victim = await waitWorkerOnline('ci-victim');
  const long = join(DIR, 'long-source.mkv');
  makeSource(long, LONG_SECS);
  const up = await uploadFile(long, { filename: 'long-source.mkv' });
  const running = await until(
    'the ladder job to be encoding',
    async () => {
      const st = await assetStatus(up.id);
      const j = st.jobs.find((x) => x.kind === 'ladder');
      if (j?.state === 'succeeded')
        throw new Error(
          'the ladder finished before it could be killed; raise MEDIA_SMOKE_LONG_SECS',
        );
      return j && j.state === 'running' && (j.progress?.percent ?? 0) >= 8 ? j : null;
    },
    { timeoutMs: 10 * 60000 },
  );
  log(
    `ladder job ${running.job_id} is at ${running.progress.percent.toFixed(0)}% on ${victim.name}; killing the worker and its ffmpeg`,
  );
  const killedAt = Date.now();
  killWorker('ci-victim');
  const requeued = await until(
    'the lease to expire and the job to be requeued',
    async () => {
      const d = await jobDetail(running.job_id);
      if (d.state === 'failed')
        throw new Error(`the job failed instead of being retried: ${d.error}`);
      return d.state === 'queued' ? d : null;
    },
    { timeoutMs: 8 * 60000, everyMs: DRY ? 20 : 5000 },
  );
  const waited = (Date.now() - killedAt) / 1000;
  check(
    'kill: the job went back to queued only after its lease ran out (not at once)',
    DRY || waited >= 45,
    `${waited.toFixed(0)} s`,
  );
  check(
    'kill: still attempt 1 and no worker holds it',
    requeued.attempt === 1 && !requeued.worker_id,
    requeued,
  );
  log(
    `requeued ${waited.toFixed(0)} s after the kill; reason: ${requeued.error ?? '(none recorded)'}`,
  );
  await startWorker('ci-rescue', 1);
  const rescue = await waitWorkerOnline('ci-rescue');
  const done = await waitPlayable(up.id);
  const l2 = done.jobs.find((j) => j.kind === 'ladder');
  check(
    'kill: the retry ran on the new worker as attempt 2 and succeeded',
    l2.state === 'succeeded' && l2.attempt === 2 && l2.worker_id === rescue.id,
    l2,
  );
  check('kill: the asset is playable after the retry', done.asset.playable, done.asset);
  killWorker('ci-rescue');
}

// ==== main ======================================================================================================
const STAGES = {
  pipeline: stagePipeline,
  retranscode: stageRetranscode,
  download: stageDownload,
  tv: stageTv,
  resilience: stageResilience,
};
const wanted = process.argv.slice(2).length ? process.argv.slice(2) : Object.keys(STAGES);
for (const s of wanted)
  if (!STAGES[s]) throw new Error(`unknown stage ${s}; use ${Object.keys(STAGES).join(' ')}`);

let crashed = null;
try {
  await signIn();
  console.log(
    `signed in as ${OWNER} at ${BASE}${DRY ? '  (DRY WALK with fakes: nothing here is real)' : ''}`,
  );
  for (const s of wanted) {
    try {
      await STAGES[s]();
    } catch (e) {
      failures++;
      console.log(`FAIL  stage ${s} stopped: ${e.message}`);
      crashed ??= e;
    }
  }
} catch (e) {
  failures++;
  crashed = e;
  console.log(`FAIL  ${e.stack ?? e.message}`);
} finally {
  killAllWorkers();
  if (failures) {
    for (const f of [
      'worker-ci-main.log',
      'worker-ci-victim.log',
      'worker-ci-rescue.log',
      'worker-ci-corrupt.log',
    ]) {
      const p = join(DIR, f);
      if (existsSync(p))
        console.log(
          `---- tail of ${f}\n${readFileSync(p, 'utf8').split('\n').slice(-30).join('\n')}`,
        );
    }
  }
  if (fakeServer) await fakeServer.close();
}
console.log(
  failures ? `\nmedia-smoke: ${failures} failure(s)` : '\nmedia-smoke: all checks passed',
);
process.exit(failures ? 1 : 0);
