// Fakes for the dry walk of deploy/scripts/media-smoke.mjs (MEDIA_SMOKE_DRY=1): an in-memory stand-in for the
// parts of sr-server the smoke touches, and stand-ins for ffmpeg, ffprobe, psql, sr-cli and the worker process.
// It proves the driver's own logic (stage order, polling, parsing, the numbering check, the kill and retry
// bookkeeping) on a machine with no ffmpeg, no Postgres and no Rust build. It proves NOTHING about the real
// pipeline: the answers below are what a correct server and ffmpeg are expected to give, nothing more.
import { createServer } from 'node:http';
import { writeFileSync } from 'node:fs';
import { basename } from 'node:path';

const box = (name, size, fill = 0) => {
  const b = Buffer.alloc(size, fill);
  b.writeUInt32BE(size, 0);
  b.write(name, 4, 'latin1');
  return b;
};
const MP4 = Buffer.concat([box('ftyp', 24), box('moov', 400), box('mdat', 4000, 7)]);
const INIT = Buffer.concat([box('ftyp', 24), box('moov', 400)]);
const SEG_LEN = 250; // ms per virtual live segment in the dry walk
const LIVE_WINDOW = 4;

export async function startFakeServer() {
  const st = {
    nextId: 1000,
    assets: new Map(),
    jobs: [],
    workers: [],
    entitled: false,
    downloads: new Map(),
    slateId: null,
    channel: null,
    block: null,
    corrupt: new Set(),
  };
  const id = () => String(st.nextId++);

  function assetOfKey(key) {
    const m = /originals\/(\d+)\//.exec(key);
    return m ? st.assets.get(m[1]) : undefined;
  }
  const summary = (j) => ({
    job_id: j.id,
    asset_id: j.asset,
    kind: j.kind,
    state: j.state,
    attempt: j.attempt,
    worker_id: j.worker ?? undefined,
    progress: j.state === 'running' ? { percent: j.percent } : undefined,
    error: j.error,
    started_at_ms: j.startedAt,
    finished_at_ms: j.finishedAt,
  });
  const detail = (j) => ({
    ...summary(j),
    log_tail: j.log,
    encoder: j.state === 'succeeded' ? 'libx264' : undefined,
    output_count: j.state === 'succeeded' ? 40 : 0,
    warnings: [],
  });
  const assetView = (a) => ({
    id: a.id,
    state: a.state,
    error: a.error,
    playable: a.playable,
    has_proxy: a.proxy,
    duration_ms: 1200,
    updated_at_ms: a.updated,
    title_id: a.title ?? undefined,
    episode_id: a.episode ?? undefined,
  });
  const alive = () => st.workers.filter((w) => w.online);
  const active = (w) => st.jobs.filter((j) => j.state === 'running' && j.worker === w.id).length;

  function addJobs(a, kinds) {
    for (const kind of kinds) {
      st.jobs.push({
        id: id(),
        asset: a.id,
        kind,
        state: 'queued',
        attempt: 0,
        ticks: 0,
        percent: 0,
      });
    }
  }

  function tick() {
    const now = Date.now();
    for (const j of st.jobs.filter((x) => x.state === 'queued')) {
      const w = alive().find((x) => active(x) < x.concurrency);
      if (!w) break;
      Object.assign(j, {
        state: 'running',
        worker: w.id,
        attempt: j.attempt + 1,
        startedAt: now,
        ticks: 0,
        percent: 0,
        error: undefined,
      });
    }
    for (const j of st.jobs.filter((x) => x.state === 'running')) {
      const w = st.workers.find((x) => x.id === j.worker);
      const a = st.assets.get(j.asset);
      if (!w?.online) {
        j.deadAt ??= now + 120;
        if (now >= j.deadAt)
          Object.assign(j, {
            state: 'queued',
            worker: null,
            error: 'worker lease expired',
            deadAt: undefined,
          });
        continue;
      }
      j.ticks += 1;
      j.percent = Math.min(95, j.ticks * 6);
      if (a.corrupt && j.ticks >= 2) {
        Object.assign(j, {
          state: 'failed',
          finishedAt: now,
          error: `${j.kind} step failed: the file could not be decoded`,
          log: '$ ffmpeg -hide_banner -nostdin -i source.mkv\nexit code 1\n[matroska,webm @ 0x1] Invalid data found when processing input\n',
        });
        continue;
      }
      const limit = a.long && j.kind === 'ladder' && j.attempt === 1 ? 100000 : 8;
      if (j.ticks >= limit) {
        Object.assign(j, { state: 'succeeded', finishedAt: now, percent: 100 });
        if (j.kind === 'proxy') a.proxy = true;
      }
    }
    for (const a of st.assets.values()) {
      const mine = st.jobs.filter((j) => j.asset === a.id);
      if (
        a.corrupt &&
        mine.length &&
        mine.every((j) => ['failed', 'succeeded'].includes(j.state)) &&
        mine.some((j) => j.state === 'failed')
      ) {
        a.state = 'failed';
        a.error = 'the file could not be decoded';
      }
      const ladders = mine.filter((j) => j.kind === 'ladder');
      if (!a.corrupt && ladders.length && ladders.every((j) => j.state === 'succeeded')) {
        if (!a.playable) a.updated = now;
        a.playable = true;
        a.state = 'published';
        if (a.retranscodeAt && !a.bumped) {
          a.bumped = true;
          a.updated = now + 5;
        }
      }
    }
  }
  const timer = setInterval(tick, 15);

  // ---- Live TV ------------------------------------------------------------------------------------------------
  function liveState() {
    const b = st.block;
    const now = Date.now();
    return b && now >= b.start_ms && now < b.end_ms ? 'live' : 'off_air';
  }
  function kindAt(seqStartMs) {
    const b = st.block;
    return b && seqStartMs >= b.start_ms && seqStartMs < b.end_ms ? 'episode' : 'slate';
  }
  const assetFor = (kind) => (kind === 'episode' ? st.block.asset : st.slateId);
  function liveMedia() {
    const newest = Math.floor(Date.now() / SEG_LEN);
    const first = newest - LIVE_WINDOW + 1;
    const discAt = (s) => {
      // one discontinuity at each change of kind since the block began
      const b = st.block;
      const t = s * SEG_LEN;
      return (t >= b.start_ms ? 1 : 0) + (t >= b.end_ms ? 1 : 0);
    };
    let text = `#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:${first}\n#EXT-X-DISCONTINUITY-SEQUENCE:${discAt(first)}\n`;
    for (let s = first; s <= newest; s++) {
      if (s > first && discAt(s) !== discAt(s - 1)) text += '#EXT-X-DISCONTINUITY\n';
      const a = assetFor(kindAt(s * SEG_LEN));
      text += `#EXT-X-MAP:URI="/cdn/hls/${a}/v/720p/init.mp4?sig=${s}"\n#EXTINF:0.25,\n/cdn/hls/${a}/v/720p/seg_${String(s).padStart(5, '0')}.m4s?sig=${s}\n`;
    }
    return text;
  }

  // ---- the HTTP surface ---------------------------------------------------------------------------------------
  async function handle(req, res) {
    const url = new URL(req.url, 'http://x');
    const path = url.pathname;
    const body = await new Promise((r) => {
      const c = [];
      req.on('data', (d) => c.push(d));
      req.on('end', () => r(Buffer.concat(c)));
    });
    const json = () => (body.length ? JSON.parse(body.toString('utf8')) : {});
    const send = (status, obj, headers = {}) => {
      const data =
        typeof obj === 'string' || Buffer.isBuffer(obj) ? obj : JSON.stringify(obj ?? null);
      res.writeHead(status, {
        'content-type':
          typeof obj === 'string' || Buffer.isBuffer(obj)
            ? 'application/octet-stream'
            : 'application/json',
        ...headers,
      });
      res.end(data);
    };
    const m = (re) => re.exec(api ?? path);
    const api = path.startsWith('/api/v1') ? path.slice(7) : null;
    let r;

    // storage and CDN
    if (path.startsWith('/fake-s3/') && req.method === 'PUT') {
      // /fake-s3/put/<key> is an upload part; /fake-s3/<bucket>/<key> is the driver replacing a stored object.
      if (!path.startsWith('/fake-s3/put/')) {
        const key = decodeURIComponent(path.replace(/^\/fake-s3\/[^/]+\//, ''));
        const a = assetOfKey(key);
        if (a) a.corrupt = true;
      }
      return send(200, '', { etag: '"fake-etag"' });
    }
    if ((r = m(/^\/cdn\/hls\/(\d+)\/(.+)$/))) {
      const rel = r[2];
      const idn = r[1];
      if (rel === 'master.m3u8') {
        return send(
          200,
          `#EXTM3U
#EXT-X-VERSION:7
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="English",LANGUAGE="en",DEFAULT=YES,URI="a/00-eng-stereo/index.m3u8?sig=1"
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="Spanish",LANGUAGE="es",DEFAULT=NO,URI="a/01-spa-stereo/index.m3u8?sig=1"
#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English",LANGUAGE="en",URI="subs/00-eng.m3u8?sig=1"
#EXT-X-STREAM-INF:BANDWIDTH=1800000,RESOLUTION=854x480,CODECS="avc1.64001e,mp4a.40.2",AUDIO="aud",SUBTITLES="subs"
v/480p/index.m3u8?sig=1
#EXT-X-STREAM-INF:BANDWIDTH=3500000,RESOLUTION=1280x720,CODECS="avc1.64001f,mp4a.40.2",AUDIO="aud",SUBTITLES="subs"
v/720p/index.m3u8?sig=1
`,
        );
      }
      if (rel.endsWith('index.m3u8')) {
        let t =
          '#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:6\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-MAP:URI="init.mp4?sig=1"\n';
        for (const [i, d] of [6, 6, 6, 2].entries())
          t += `#EXTINF:${d}.000000,\nseg_${String(i).padStart(5, '0')}.m4s?sig=1\n`;
        return send(200, t + '#EXT-X-ENDLIST\n');
      }
      if (rel.endsWith('.m3u8'))
        return send(
          200,
          '#EXTM3U\n#EXT-X-TARGETDURATION:20\n#EXTINF:20,\n00-eng.vtt?sig=1\n#EXT-X-ENDLIST\n',
        );
      if (rel.endsWith('init.mp4')) return send(200, INIT);
      if (rel.endsWith('.m4s')) return send(200, Buffer.alloc(300, 1));
      if (rel === 'subs/00-eng.vtt') {
        return send(
          200,
          'WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nHello from the CI source\n\n00:00:05.000 --> 00:00:08.000\nSecond cue\n\n00:00:10.000 --> 00:00:14.000\nThird cue\n',
        );
      }
      if (rel === 'sprites/sprites.vtt') {
        return send(
          200,
          `WEBVTT\n\n00:00:00.000 --> 00:00:10.000\nsheet-001.jpg?sig=1#xywh=0,0,160,90\n\n00:00:10.000 --> 00:00:20.000\nsheet-001.jpg?sig=1#xywh=160,0,160,90\n`,
        );
      }
      if (rel.startsWith('sprites/sheet-'))
        return send(200, Buffer.from([0xff, 0xd8, 0xff, 0xe0, 0, 0, 0, 0]));
      void idn;
      return send(404, 'no');
    }
    if ((r = m(/^\/cdn\/dl\/(\d+)\.mp4$/))) {
      const range = req.headers.range;
      const common = {
        'content-type': 'video/mp4',
        'content-disposition': 'attachment; filename="media-smoke-720p.mp4"',
        'x-content-type-options': 'nosniff',
        'accept-ranges': 'bytes',
      };
      if (range)
        return send(206, MP4.subarray(0, 100), {
          ...common,
          'content-range': `bytes 0-99/${MP4.length}`,
          'content-length': '100',
        });
      return send(200, MP4, { ...common, 'content-length': String(MP4.length) });
    }
    if (path === '/healthz') return send(200, 'ok');
    if (api === null) return send(404, 'no');

    // API
    if (api === '/sr/auth/login')
      return send(200, { status: 'ok' }, { 'set-cookie': 'sid=fake; Path=/' });
    if (api === '/sr/csrf') return send(200, { token: 'csrf-fake' });
    if (api === '/users/@me') return send(200, { id: '1' });
    if (api === '/admin/transcode/tokens')
      return send(201, { token: { id: '1' }, secret: 'fake-secret' });
    if (api === '/admin/transcode/workers') {
      return send(
        200,
        st.workers.map((w) => ({
          id: w.id,
          name: w.name,
          online: w.online,
          active_jobs: active(w),
          capabilities: {
            encoders: ['libx264', 'aac'],
            nvenc: false,
            ffmpeg_version: '6.1.1-fake',
          },
        })),
      );
    }
    if (api === '/admin/transcode/queue') {
      return send(200, {
        queued: 0,
        running: 0,
        failed_last_day: st.jobs.filter((j) => j.state === 'failed').length,
        succeeded_last_day: 1,
      });
    }
    if ((r = m(/^\/admin\/transcode\/jobs\/(\d+)$/)) && api)
      return send(200, detail(st.jobs.find((j) => j.id === r[1])));
    if (api === '/admin/transcode/jobs') {
      const kind = url.searchParams.get('kind');
      return send(200, st.jobs.filter((j) => !kind || j.kind === kind).map(summary));
    }
    if (api === '/admin/transcode/retranscode') {
      const { id: aid } = json();
      const a = st.assets.get(aid);
      a.retranscodeAt = Date.now();
      a.bumped = false;
      a.playable = true;
      addJobs(a, ['ladder']);
      return send(200, {
        enqueued: [st.jobs[st.jobs.length - 1].id],
        skipped: [],
        failed: [],
        truncated: false,
        estimated_gpu_secs: 0,
      });
    }
    if (api === '/admin/cinema/titles') return send(201, { id: id() });
    if ((r = m(/^\/admin\/cinema\/titles\/(\d+)\/(seasons|publish)$/)) && api)
      return send(r[2] === 'seasons' ? 201 : 200, { id: id() });
    if ((r = m(/^\/admin\/cinema\/seasons\/(\d+)\/episodes$/)) && api)
      return send(201, { id: id() });
    if ((r = m(/^\/admin\/cinema\/episodes\/(\d+)\/publish$/)) && api)
      return send(200, { id: r[1] });
    if (api === '/admin/cinema/assets' && req.method === 'POST') {
      const b = json();
      const aid = id();
      const a = {
        id: aid,
        state: 'awaiting_upload',
        playable: false,
        proxy: false,
        updated: Date.now(),
        episode: b.episode_id,
        filename: b.filename,
        size: b.size_bytes,
        long: /long/.test(b.filename),
      };
      st.assets.set(aid, a);
      return send(201, {
        asset: assetView(a),
        upload_id: 'u1',
        part_size: 128 * 1024 * 1024,
        content_type: 'video/x-matroska',
        parts: [{ part_number: 1, url: `${fakeBase()}/fake-s3/put/originals/${aid}/source.mkv` }],
        expires_in_secs: 3600,
      });
    }
    if ((r = m(/^\/admin\/cinema\/assets\/(\d+)\/complete$/)) && api) {
      const a = st.assets.get(r[1]);
      a.state = 'transcoding';
      addJobs(a, ['proxy', 'ladder']);
      // The queued jobs wait for a worker; in the corrupt scenario the overwrite happens before one exists.
      return send(202, assetView(a));
    }
    if ((r = m(/^\/admin\/cinema\/assets\/(\d+)\/link$/)) && api) {
      const a = st.assets.get(r[1]);
      a.episode = json().episode_id;
      return send(200, assetView(a));
    }
    if ((r = m(/^\/admin\/cinema\/assets\/(\d+)$/)) && api) {
      const a = st.assets.get(r[1]);
      if (!a) return send(404, { code: 'not_found' });
      return send(200, {
        asset: assetView(a),
        jobs: st.jobs.filter((j) => j.asset === a.id).map(summary),
        probe: {
          container: 'matroska,webm',
          duration_secs: 20,
          width: 1280,
          height: 720,
          video_codec: 'h264',
          audio_languages: ['eng', 'spa'],
          subtitle_count: 1,
        },
      });
    }
    if ((r = m(/^\/cinema\/assets\/(\d+)\/playback$/)) && api) {
      const a = r[1];
      return send(200, {
        master_url: `/cdn/hls/${a}/master.m3u8?sig=1`,
        expires_at: 9e9,
        refresh_at: 9e9,
        duration_secs: 20,
        subtitles: [
          {
            id: 's0',
            language: 'eng',
            label: 'English',
            vtt_url: `/cdn/hls/${a}/subs/00-eng.vtt?sig=1`,
          },
        ],
        audio_tracks: [
          { id: '0', language: 'eng' },
          { id: '1', language: 'spa' },
        ],
        sprites_url: `/cdn/hls/${a}/sprites/sprites.vtt?sig=1`,
        sprite: {
          interval_secs: 10,
          thumb_width: 160,
          thumb_height: 90,
          columns: 10,
          rows: 10,
          thumbs_total: 2,
        },
        markers: [],
      });
    }
    if ((r = m(/^\/cinema\/titles\/(\d+)\/downloads$/)) && api) {
      if (!st.entitled) return send(403, { code: 'forbidden' });
      if (req.method === 'GET') {
        return send(200, {
          renditions: [
            {
              id: '720p',
              label: '720p',
              height: 720,
              size_bytes: 9000,
              kind: 'rendition',
              cached: false,
            },
          ],
          daily_remaining: 20 - st.downloads.size,
        });
      }
      const jid = id();
      const a = [...st.assets.values()].find((x) => x.episode === json().episode_id);
      st.downloads.set(jid, { polls: 0 });
      addJobs(a, ['remux']);
      return send(202, { id: jid, state: 'queued', progress: 0 });
    }
    if ((r = m(/^\/cinema\/downloads\/(\d+)\/file$/)) && api) {
      return send(307, '', { location: `${fakeBase()}/cdn/dl/${r[1]}.mp4?dl=1&sig=x` });
    }
    if ((r = m(/^\/cinema\/downloads\/(\d+)$/)) && api) {
      const d = st.downloads.get(r[1]);
      d.polls += 1;
      return send(
        200,
        d.polls < 3
          ? { id: r[1], state: 'preparing', progress: 0.5 }
          : {
              id: r[1],
              state: 'ready',
              progress: 1,
              url: `/api/v1/cinema/downloads/${r[1]}/file`,
              expires_at_ms: Date.now() + 86400000,
            },
      );
    }
    if (api === '/admin/donations/entitlements') {
      st.entitled = true;
      return send(200, { id: id() });
    }
    if (api === '/admin/tv/slate')
      return send(200, { configured: !!st.slateId, asset_id: st.slateId, playable: true });
    if (api === '/admin/tv/channels') {
      st.channel = id();
      return send(201, { id: st.channel });
    }
    if ((r = m(/^\/admin\/tv\/channels\/(\d+)\/blocks$/)) && api) {
      const b = json();
      const ep = [...st.assets.values()].find((x) => x.episode === b.episode_id);
      st.block = { ...b, asset: ep.id };
      return send(201, {
        id: id(),
        episode_id: b.episode_id,
        start_ms: b.start_ms,
        end_ms: b.end_ms,
      });
    }
    if ((r = m(/^\/admin\/tv\/channels\/(\d+)\/now$/)) && api)
      return send(200, { current: null, next: null });
    if ((r = m(/^\/tv\/channels\/(\d+)\/tune$/)) && api) {
      const live = liveState() === 'live';
      return send(200, {
        channel_id: r[1],
        state: live ? 'live' : 'off_air',
        slate: !live,
        master_url: `/api/v1/tv/channels/${r[1]}/live/master.m3u8`,
      });
    }
    if ((r = m(/^\/tv\/channels\/(\d+)\/live\/master\.m3u8$/)) && api) {
      return send(
        200,
        `#EXTM3U
#EXT-X-VERSION:7
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="Audio",LANGUAGE="eng",DEFAULT=YES,URI="audio.m3u8"
#EXT-X-STREAM-INF:BANDWIDTH=3500000,RESOLUTION=1280x720,AUDIO="aud"
v/720.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=1800000,RESOLUTION=854x480,AUDIO="aud"
v/480.m3u8
`,
      );
    }
    if ((r = m(/^\/tv\/channels\/(\d+)\/live\/(v\/\d+|audio)\.m3u8$/)) && api)
      return send(200, liveMedia());
    return send(404, { code: 'unknown route', path });
  }

  const server = createServer((req, res) => {
    handle(req, res).catch((e) => {
      res.writeHead(500, { 'content-type': 'text/plain' });
      res.end(String(e.stack ?? e));
    });
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${server.address().port}`;
  const fakeBase = () => base;
  return {
    base,
    state: st,
    async close() {
      clearInterval(timer);
      await new Promise((r) => server.close(r));
    },
  };
}

/** Stand-ins for the programs the driver starts. */
export function fakeIo(server) {
  const st = server.state;
  return {
    run(cmd, args) {
      const name = basename(cmd).replace(/\.exe$/, '');
      if (name === 'ffmpeg') {
        const out = args[args.length - 1];
        writeFileSync(out, Buffer.alloc(4096, 3));
        return { status: 0, stdout: '', stderr: '' };
      }
      if (name === 'ffprobe') {
        const file = basename(args[args.length - 1]);
        const v = (w, h, codec = 'h264') => ({
          codec_type: 'video',
          codec_name: codec,
          width: w,
          height: h,
        });
        const a = { codec_type: 'audio', codec_name: 'aac' };
        const s = { codec_type: 'subtitle', codec_name: 'subrip' };
        let streams;
        let duration = 5;
        let startTime;
        // As the real ffprobe reports init + fragment i of a fragmented MP4: the container duration is the
        // fragment's end time, not its length (the first media.yml run summed 56 s for a 20 s rendition).
        const seg = /^seg-.*-(\d+)\.mp4$/.exec(file);
        if (seg) {
          startTime = Number(seg[1]) * 5;
          duration = startTime + 5;
        }
        if (/^seg-audio/.test(file)) streams = [a];
        else if (/^seg-video/.test(file)) streams = [v(1280, 720)];
        else if (/^whole-/.test(file)) {
          streams = /audio/.test(file) ? [a] : [v(1280, 720)];
          duration = 20;
        } else if (file === 'sprite-sheet.jpg') streams = [v(1600, 900, 'mjpeg')];
        else if (file === 'download.mp4') {
          streams = [v(1280, 720), a, a];
          duration = 20;
        } else if (/^live-/.test(file)) streams = [v(1280, 720)];
        else {
          streams = [v(1280, 720), a, a, s];
          duration = 20;
        }
        return {
          status: 0,
          stdout: JSON.stringify({
            streams,
            format: {
              duration: String(duration),
              ...(startTime === undefined ? {} : { start_time: String(startTime) }),
            },
          }),
          stderr: '',
        };
      }
      if (name === 'sr-cli') {
        const aid = String(st.nextId++);
        st.assets.set(aid, {
          id: aid,
          state: 'published',
          playable: true,
          proxy: true,
          updated: Date.now(),
        });
        st.jobs.push({
          id: String(st.nextId++),
          asset: aid,
          kind: 'ladder',
          state: 'succeeded',
          attempt: 1,
          ticks: 0,
          percent: 100,
        });
        st.slateId = aid;
        return { status: 0, stdout: `slate asset ${aid} stored\n${aid}\n`, stderr: '' };
      }
      if (name === 'psql') return { status: 0, stdout: '1\n', stderr: '' };
      return { status: 127, stdout: '', stderr: `fake io: ${cmd} is not faked` };
    },
    spawnWorker({ args, name }) {
      const w = {
        id: `w-${name}`,
        name: args[args.indexOf('--name') + 1] ?? name,
        online: true,
        concurrency: Number(args[args.indexOf('--concurrency') + 1] ?? 1),
      };
      st.workers = st.workers.filter((x) => x.name !== w.name);
      st.workers.push(w);
      return {
        pid: 0,
        kill() {
          w.online = false;
        },
        exited: false,
        logPath: '',
      };
    },
  };
}
