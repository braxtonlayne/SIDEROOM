import { describe, expect, it } from 'vitest';
import { createConverter } from '../converter';
import { convertedKey, planRemux, planSubtitles, needsConversion } from '../plan';
import { parseProbeLog, progressSeconds } from '../probe';
import { ConvertError, type ConvertProgress, type FfmpegEngine } from '../types';

const MKV_LOG = `ffmpeg version 5.1 Copyright (c) 2000-2022 the FFmpeg developers
Input #0, matroska,webm, from '/in/input.mkv':
  Metadata:
    title           : Test
  Duration: 00:01:40.50, start: 0.000000, bitrate: 2345 kb/s
  Stream #0:0: Video: h264 (High), yuv420p(progressive), 1920x1080 [SAR 1:1 DAR 16:9], 23.98 fps, 23.98 tbr, 1k tbn (default)
  Stream #0:1(eng): Audio: aac (LC), 48000 Hz, 5.1(side), fltp (default)
    Metadata:
      title           : Surround
  Stream #0:2(jpn): Audio: ac3, 48000 Hz, stereo, fltp, 192 kb/s
  Stream #0:3(eng): Subtitle: subrip (default)
    Metadata:
      title           : English
  Stream #0:4(fre): Subtitle: ass
  Stream #0:5(ger): Subtitle: hdmv_pgs_subtitle
  Stream #0:6(eng): Subtitle: subrip
At least one output file must be specified`.split('\n');

describe('parseProbeLog', () => {
  const p = parseProbeLog(MKV_LOG);
  it('reads the container, duration and bitrate', () => {
    expect(p.container).toBe('matroska,webm');
    expect(p.durationSec).toBeCloseTo(100.5);
    expect(p.bitrateKbps).toBe(2345);
  });
  it('reads every stream', () => {
    expect(p.streams.map((s) => [s.index, s.kind, s.codec])).toEqual([
      [0, 'video', 'h264'],
      [1, 'audio', 'aac'],
      [2, 'audio', 'ac3'],
      [3, 'subtitle', 'subrip'],
      [4, 'subtitle', 'ass'],
      [5, 'subtitle', 'hdmv_pgs_subtitle'],
      [6, 'subtitle', 'subrip'],
    ]);
    expect(p.streams[0]).toMatchObject({ width: 1920, height: 1080, fps: 23.98, profile: 'High' });
    expect(p.streams[1]).toMatchObject({ language: 'eng', channels: 6, sampleRate: 48000 });
    expect(p.streams[2]?.bitrateKbps).toBe(192);
  });
  it('reads stream titles and marks bitmap subtitles', () => {
    expect(p.streams[1]?.title).toBe('Surround');
    expect(p.streams[3]?.title).toBe('English');
    expect(p.streams[3]?.textual).toBe(true);
    expect(p.streams[5]?.textual).toBe(false);
  });
  it('reads the progress clock', () => {
    expect(
      progressSeconds(
        'frame=  100 fps=0.0 q=-1.0 size=  1024kB time=00:00:12.34 bitrate=1.0kbits/s',
      ),
    ).toBeCloseTo(12.34);
    expect(progressSeconds('nothing here')).toBeNull();
  });
  it('reads an MPEG-TS stream id and an unknown duration', () => {
    const t = parseProbeLog([
      "Input #0, mpegts, from 'x.ts':",
      '  Duration: N/A, start: 1.4, bitrate: N/A',
      '  Stream #0:0[0x100]: Video: hevc (Main 10) (HEVC / 0x43564548), yuv420p10le(tv), 3840x2160, 25 fps, 25 tbr, 90k tbn',
      '  Stream #0:1[0x101](und): Audio: eac3 (AC-3 / 0x332D4341), 48000 Hz, 5.1(side), fltp, 640 kb/s',
    ]);
    expect(t.durationSec).toBeNull();
    expect(t.streams[0]).toMatchObject({ codec: 'hevc', width: 3840, profile: 'Main 10' });
    expect(t.streams[1]).toMatchObject({ codec: 'eac3' });
    expect(t.streams[1]?.language).toBeUndefined();
  });
});

describe('planRemux', () => {
  it('copies H.264 and AAC into MP4, drops AC-3 and the bitmap subtitle', () => {
    const plan = planRemux(parseProbeLog(MKV_LOG));
    expect(plan.container).toBe('mp4');
    expect(plan.map).toEqual([0, 1]);
    expect(plan.dropped).toEqual([
      { index: 2, reason: 'incompatible-audio' },
      { index: 5, reason: 'bitmap-subtitle' },
    ]);
  });
  it('refuses a source whose only audio is AC-3 (a transcoder must take it)', () => {
    const p = parseProbeLog([
      'Input #0, avi, from x:',
      '  Stream #0:0: Video: h264 (High), 640x360, 25 fps',
      '  Stream #0:1: Audio: ac3, 48000 Hz, stereo, fltp',
    ]);
    expect(planRemux(p)).toMatchObject({ container: null, reason: 'audio-codec' });
  });
  it('refuses MPEG-4 part 2 and sends VP8 to WebM with WebM audio only', () => {
    const avi = parseProbeLog([
      'Input #0, avi, from x:',
      '  Stream #0:0: Video: mpeg4 (Advanced Simple Profile), 640x360, 25 fps',
    ]);
    expect(planRemux(avi)).toMatchObject({ container: null, reason: 'video-codec' });
    const vp8 = parseProbeLog([
      'Input #0, matroska,webm, from x:',
      '  Stream #0:0: Video: vp8, yuv420p, 640x360, 30 fps',
      '  Stream #0:1: Audio: vorbis, 44100 Hz, stereo, fltp',
      '  Stream #0:2: Audio: aac (LC), 44100 Hz, stereo, fltp',
    ]);
    expect(planRemux(vp8)).toMatchObject({ container: 'webm', map: [0, 1] });
  });
  it('ignores cover art and a source with no video', () => {
    const art = parseProbeLog([
      'Input #0, mp3, from x:',
      '  Stream #0:0: Audio: mp3, 44100 Hz, stereo',
      '  Stream #0:1: Video: mjpeg, 500x500',
    ]);
    expect(planRemux(art)).toMatchObject({ container: null, reason: 'no-video' });
  });
  it('plans a video with no audio', () => {
    const v = parseProbeLog([
      'Input #0, mov,mp4, from x:',
      '  Stream #0:0: Video: hevc (Main), 1280x720, 30 fps',
    ]);
    expect(planRemux(v)).toMatchObject({ container: 'mp4', videoCodec: 'hevc', audioCodecs: [] });
  });
});

describe('names', () => {
  it('puts the output under converted/ and swaps the extension', () => {
    expect(convertedKey('Movies/Film (2019).mkv', 'mp4')).toBe('converted/Movies/Film (2019).mp4');
    expect(convertedKey('a.b/noext', 'mp4')).toBe('converted/a.b/noext.mp4');
  });
  it('names subtitles by language and numbers a repeat', () => {
    const { items, skipped } = planSubtitles(parseProbeLog(MKV_LOG), 'Movies/Film.mkv');
    expect(items.map((i) => i.name)).toEqual(['Film.eng.srt', 'Film.fre.ass', 'Film.eng.2.srt']);
    expect(skipped.map((s) => s.index)).toEqual([5]);
  });
  it('knows which files need conversion', () => {
    expect(needsConversion('a.MKV')).toBe(true);
    expect(needsConversion('a.mp4')).toBe(false);
  });
});

/** A fake ffmpeg worker: answers `-i` with a log, and writes files for `exec` calls with an output name. */
function fakeEngine(log: string[], opts: { code?: number; empty?: boolean } = {}) {
  const calls: string[][] = [];
  const files = new Map<string, Uint8Array>();
  const state = { loaded: 0, terminated: 0, mounted: 0, unmounted: 0 };
  const engine: FfmpegEngine = {
    async load() {
      state.loaded++;
    },
    async mountInput(_f, name) {
      state.mounted++;
      return `/in/${name}`;
    },
    async unmountInput() {
      state.unmounted++;
    },
    async exec(args, o) {
      calls.push(args);
      const hasOutput = args.length > 4;
      if (!hasOutput) {
        log.forEach((l) => o?.onLog?.(l));
        return 1;
      }
      o?.onLog?.('frame= 10 size=1kB time=00:00:50.25 bitrate=1kbits/s');
      for (const a of args.slice(3))
        if (/\.(mp4|webm|srt|ass)$/.test(a))
          files.set(a, opts.empty ? new Uint8Array() : new Uint8Array([1, 2, 3]));
      return opts.code ?? 0;
    },
    async readFile(p) {
      const f = files.get(p);
      if (!f) throw new Error('missing');
      return f;
    },
    async deleteFile(p) {
      files.delete(p);
    },
    terminate() {
      state.terminated++;
    },
  };
  return { engine, calls, state, files };
}

const mkv = (size = 10) =>
  new File([new Uint8Array(size)], 'Film.mkv', { type: 'video/x-matroska' });

describe('Converter', () => {
  it('probes', async () => {
    const f = fakeEngine(MKV_LOG);
    const c = createConverter(() => f.engine);
    const p = await c.probe(mkv());
    expect(p.streams).toHaveLength(7);
    expect(f.calls[0]).toEqual(['-hide_banner', '-i', '/in/input.mkv']);
    expect(f.state.unmounted).toBe(1);
  });

  it('remuxes with -c copy and faststart, reporting progress', async () => {
    const f = fakeEngine(MKV_LOG);
    const c = createConverter(() => f.engine);
    const seen: ConvertProgress[] = [];
    const r = await c.remux(mkv(), { onProgress: (p) => seen.push(p) });
    expect(r.output.name).toBe('Film.mp4');
    expect(r.output.file.type).toBe('video/mp4');
    expect(r.output.file.size).toBe(3);
    const args = f.calls[1]!;
    expect(args).toContain('copy');
    expect(args.join(' ')).toContain('-map 0:0 -map 0:1');
    expect(args).not.toContain('0:2');
    expect(args.join(' ')).toContain('-movflags +faststart');
    expect(args.at(-1)).toBe('output.mp4');
    const last = seen.at(-1)!;
    expect(last.phase).toBe('remuxing');
    expect(last.fraction).toBeCloseTo(50.25 / 100.5);
    expect(seen.map((p) => p.phase)).toContain('loading');
  });

  it('writes a fragmented MP4 on request', async () => {
    const f = fakeEngine(MKV_LOG);
    await createConverter(() => f.engine).remux(mkv(), { fragmented: true });
    expect(f.calls[1]!.join(' ')).toContain('+frag_keyframe+empty_moov+default_base_moof');
  });

  it('extracts every text subtitle as a sidecar and lists the bitmap one as skipped', async () => {
    const f = fakeEngine(MKV_LOG);
    const r = await createConverter(() => f.engine).extractSubtitles(mkv());
    expect(r.subtitles.map((s) => [s.name, s.format, s.language])).toEqual([
      ['Film.eng.srt', 'srt', 'eng'],
      ['Film.fre.ass', 'ass', 'fre'],
      ['Film.eng.2.srt', 'srt', 'eng'],
    ]);
    expect(r.subtitles[0]?.title).toBe('English');
    expect(r.skipped.map((s) => s.index)).toEqual([5]);
    expect(f.calls[1]!.join(' ')).toContain('-map 0:4 -c:s ass Film.fre.ass');
  });

  it('refuses a file over the limit before loading the core', async () => {
    const f = fakeEngine(MKV_LOG);
    const big = new File([new Uint8Array(2 * 1024 * 1024)], 'big.mkv');
    await expect(createConverter(() => f.engine).remux(big, { maxMiB: 1 })).rejects.toMatchObject({
      code: 'TOO_LARGE',
      limitMiB: 1,
    });
    expect(f.state.loaded).toBe(0);
  });

  it('refuses a file over 2048 MiB when no limit is passed (the 2 GiB heap of the shipped core, P29-CNV-1)', async () => {
    const f = fakeEngine(MKV_LOG);
    const big = new File([new Uint8Array(1)], 'big.mkv');
    Object.defineProperty(big, 'size', { value: 2049 * 1024 * 1024 });
    await expect(createConverter(() => f.engine).remux(big)).rejects.toMatchObject({
      code: 'TOO_LARGE',
      limitMiB: 2048,
    });
    expect(f.state.loaded).toBe(0);
  });

  it('turns an ffmpeg failure, an empty output and an unreadable source into typed errors', async () => {
    const bad = fakeEngine(MKV_LOG, { code: 1 });
    await expect(createConverter(() => bad.engine).remux(mkv())).rejects.toMatchObject({
      code: 'ENGINE',
    });
    const empty = fakeEngine(MKV_LOG, { empty: true });
    await expect(createConverter(() => empty.engine).remux(mkv())).rejects.toMatchObject({
      code: 'NO_OUTPUT',
    });
    const junk = fakeEngine(['Invalid data found when processing input']);
    await expect(createConverter(() => junk.engine).probe(mkv())).rejects.toMatchObject({
      code: 'UNSUPPORTED',
    });
  });

  it('says UNSUPPORTED when remux cannot make the source playable', async () => {
    const f = fakeEngine([
      'Input #0, avi, from x:',
      '  Stream #0:0: Video: mpeg4, 640x360, 25 fps',
    ]);
    const e = await createConverter(() => f.engine)
      .remux(mkv())
      .catch((x) => x);
    expect(e).toBeInstanceOf(ConvertError);
    expect(e.code).toBe('UNSUPPORTED');
  });

  it('cancel kills the worker and rejects with CANCELLED', async () => {
    const f = fakeEngine(MKV_LOG);
    const ac = new AbortController();
    const slow = { ...f.engine, exec: () => new Promise<number>(() => undefined) };
    const c = createConverter(() => slow);
    const p = c.remux(mkv(), { signal: ac.signal });
    await new Promise((r) => setTimeout(r, 0));
    ac.abort();
    // The pending exec never settles in this fake, as a killed worker does not: the converter must not hang.
    await expect(p).rejects.toMatchObject({ code: 'CANCELLED' });
    expect(f.state.terminated).toBe(1);
  });
});
