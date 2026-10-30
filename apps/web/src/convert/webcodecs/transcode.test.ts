import { describe, expect, it } from 'vitest';
import { convertPlanes, downmix, resample } from './audio';
import { transcodeWithWebCodecs } from './transcode';
import type {
  DemuxedSource,
  MuxerHandle,
  MuxerSetup,
  Packet,
  TranscodeOptions,
  WebCodecsApi,
} from './types';
import { WebCodecsError } from './types';

// ---- fakes -----------------------------------------------------------------

interface Log {
  decoded: number[];
  encodedFrames: Array<{ w: number; h: number; key: boolean | undefined }>;
  scaled: number;
  audioEncoded: Array<{ rate: number; ch: number }>;
  videoConfig?: VideoEncoderConfig;
  audioConfig?: AudioEncoderConfig;
  muxSetup?: MuxerSetup;
  muxVideo: Array<{ ts: number; meta?: EncodedVideoChunkMetadata }>;
  muxAudio: Array<{ ts: number; meta?: EncodedAudioChunkMetadata }>;
  finalized: boolean;
  closed: string[];
}

function makeApi(
  opts: { videoDecodeError?: boolean; srcRate?: number; srcChannels?: number } = {},
) {
  const log: Log = {
    decoded: [],
    encodedFrames: [],
    scaled: 0,
    audioEncoded: [],
    muxVideo: [],
    muxAudio: [],
    finalized: false,
    closed: [],
  };
  const frame = (ts: number, w: number, h: number) => ({
    displayWidth: w,
    displayHeight: h,
    timestamp: ts,
    duration: 33333,
    close() {},
  });

  class VideoDecoder {
    state = 'unconfigured';
    decodeQueueSize = 0;
    constructor(private init: { output: (f: unknown) => void; error: (e: Error) => void }) {}
    configure(c: VideoDecoderConfig) {
      this.state = 'configured';
      this.w = c.codedWidth ?? 0;
      this.h = c.codedHeight ?? 0;
    }
    w = 0;
    h = 0;
    decode(chunk: { timestamp: number }) {
      log.decoded.push(chunk.timestamp);
      if (opts.videoDecodeError) {
        queueMicrotask(() => this.init.error(new Error('hevc not decodable')));
        return;
      }
      this.init.output(frame(chunk.timestamp, this.w, this.h));
    }
    async flush() {}
    close() {
      this.state = 'closed';
      log.closed.push('vdec');
    }
  }
  class VideoEncoder {
    state = 'unconfigured';
    encodeQueueSize = 0;
    first = true;
    constructor(private init: { output: (c: unknown, m?: unknown) => void }) {}
    configure(c: VideoEncoderConfig) {
      this.state = 'configured';
      log.videoConfig = c;
    }
    encode(
      f: { displayWidth: number; displayHeight: number; timestamp: number },
      o?: { keyFrame?: boolean },
    ) {
      log.encodedFrames.push({ w: f.displayWidth, h: f.displayHeight, key: o?.keyFrame });
      this.init.output(
        { timestamp: f.timestamp },
        this.first ? { decoderConfig: { codec: 'avc1.640028' } } : undefined,
      );
      this.first = false;
    }
    async flush() {}
    close() {
      this.state = 'closed';
    }
  }
  class AudioDecoder {
    state = 'unconfigured';
    decodeQueueSize = 0;
    constructor(private init: { output: (d: unknown) => void }) {}
    configure() {
      this.state = 'configured';
    }
    decode(c: { timestamp: number }) {
      const n = 1024;
      const ch = opts.srcChannels ?? 2;
      this.init.output({
        sampleRate: opts.srcRate ?? 48000,
        numberOfChannels: ch,
        numberOfFrames: n,
        timestamp: c.timestamp,
        copyTo(dst: Float32Array) {
          dst.fill(0.5);
        },
        close() {},
      });
    }
    async flush() {}
    close() {
      this.state = 'closed';
    }
  }
  class AudioEncoder {
    state = 'unconfigured';
    encodeQueueSize = 0;
    constructor(private init: { output: (c: unknown, m?: unknown) => void }) {}
    configure(c: AudioEncoderConfig) {
      this.state = 'configured';
      log.audioConfig = c;
    }
    encode(d: { sampleRate: number; numberOfChannels: number; timestamp: number }) {
      log.audioEncoded.push({ rate: d.sampleRate, ch: d.numberOfChannels });
      this.init.output({ timestamp: d.timestamp });
    }
    async flush() {}
    close() {
      this.state = 'closed';
    }
  }
  class Chunk {
    timestamp: number;
    constructor(init: { timestamp: number }) {
      this.timestamp = init.timestamp;
    }
  }
  class AudioDataFake {
    sampleRate: number;
    numberOfChannels: number;
    timestamp: number;
    constructor(init: { sampleRate: number; numberOfChannels: number; timestamp: number }) {
      this.sampleRate = init.sampleRate;
      this.numberOfChannels = init.numberOfChannels;
      this.timestamp = init.timestamp;
    }
    close() {}
  }

  const api = {
    VideoDecoder,
    VideoEncoder,
    AudioDecoder,
    AudioEncoder,
    EncodedVideoChunk: Chunk,
    EncodedAudioChunk: Chunk,
    AudioData: AudioDataFake,
    async createMuxer(setup: MuxerSetup): Promise<MuxerHandle> {
      log.muxSetup = setup;
      return {
        addVideo: (c, meta) => log.muxVideo.push({ ts: c.timestamp, meta }),
        addAudio: (c, meta) => log.muxAudio.push({ ts: c.timestamp, meta }),
        finalize() {
          log.finalized = true;
          return new Blob(['x']);
        },
      };
    },
    scaleFrame(f: { timestamp: number }, w: number, h: number) {
      log.scaled++;
      return frame(f.timestamp, w, h);
    },
  } as unknown as WebCodecsApi;
  return { api, log };
}

async function* packets(list: Packet[]): AsyncGenerator<Packet> {
  for (const p of list) yield p;
}

const pk = (kind: 'video' | 'audio', i: number, key = true): Packet => ({
  kind,
  timestampUs: i * 33333,
  durationUs: 33333,
  key,
  data: new Uint8Array([1, 2, 3]),
});

function source(list: Packet[], extra: Partial<DemuxedSource> = {}): DemuxedSource {
  return {
    video: {
      config: { codec: 'hvc1.1.6.L120.90', codedWidth: 1920, codedHeight: 1080 },
      width: 1920,
      height: 1080,
      frameRate: 30,
    },
    audio: {
      config: { codec: 'ac-3', sampleRate: 48000, numberOfChannels: 2 },
      sampleRate: 48000,
      channels: 2,
    },
    packets: packets(list),
    durationSec: 10,
    ...extra,
  };
}

const step = { label: '720p', height: 720, videoBitrate: 3_000_000, avc: 'avc1.64001f' } as const;

const transcodePlan = (
  over: Partial<Extract<TranscodeOptions['plan'], { path: 'webcodecs' }>> = {},
) =>
  ({
    path: 'webcodecs',
    reason: 'test',
    container: 'mp4',
    video: { mode: 'transcode', to: 'avc', step, width: 1280, height: 720, codec: 'avc1.64001f' },
    audio: {
      mode: 'transcode',
      to: 'aac',
      codec: 'mp4a.40.2',
      bitrate: 160000,
      sampleRate: 48000,
      channels: 2,
    },
    ...over,
  }) as TranscodeOptions['plan'];

describe('transcodeWithWebCodecs', () => {
  it('decodes, scales, re-encodes and muxes video and audio', async () => {
    const { api, log } = makeApi();
    const list: Packet[] = [];
    for (let i = 0; i < 70; i++) list.push(pk('video', i, i === 0), pk('audio', i));
    const progress: number[] = [];
    const res = await transcodeWithWebCodecs({
      source: source(list),
      plan: transcodePlan(),
      api,
      onProgress: (p) => progress.push(p.fraction),
    });
    expect(res.container).toBe('mp4');
    expect(res.mime).toBe('video/mp4');
    expect(res.blob).toBeInstanceOf(Blob);
    expect(res.videoFrames).toBe(70);
    expect(res.audioFrames).toBe(70);
    expect(log.scaled).toBe(70);
    expect(log.encodedFrames.every((f) => f.w === 1280 && f.h === 720)).toBe(true);
    // key frame at 0 and every 2 s at 30 fps
    expect(log.encodedFrames.filter((f) => f.key).map((_, i) => i)).toEqual([0, 1]);
    expect(log.encodedFrames[0]!.key).toBe(true);
    expect(log.encodedFrames[60]!.key).toBe(true);
    expect(log.videoConfig).toMatchObject({
      codec: 'avc1.64001f',
      width: 1280,
      height: 720,
      bitrate: 3_000_000,
    });
    expect(log.videoConfig?.avc).toEqual({ format: 'avc' });
    expect(log.audioConfig).toMatchObject({ codec: 'mp4a.40.2', numberOfChannels: 2 });
    expect(log.muxSetup).toMatchObject({
      container: 'mp4',
      video: { codec: 'avc', width: 1280, height: 720 },
      audio: { codec: 'aac', sampleRate: 48000, channels: 2 },
    });
    expect(log.muxVideo[0]!.meta?.decoderConfig).toBeDefined();
    expect(log.finalized).toBe(true);
    expect(progress.at(-1)).toBe(1);
    expect(log.closed).toContain('vdec');
  });

  it('does not scale when the frame already has the target size', async () => {
    const { api, log } = makeApi();
    const res = await transcodeWithWebCodecs({
      source: source([pk('video', 0)], {
        video: {
          config: { codec: 'hvc1', codedWidth: 1280, codedHeight: 720 },
          width: 1280,
          height: 720,
        },
      }),
      plan: transcodePlan({ audio: { mode: 'none' } }),
      api,
    });
    expect(res.videoFrames).toBe(1);
    expect(log.scaled).toBe(0);
  });

  it('converts 5.1 at 44.1 kHz to 48 kHz stereo before encoding', async () => {
    const { api, log } = makeApi({ srcRate: 44100, srcChannels: 6 });
    await transcodeWithWebCodecs({
      source: source([pk('audio', 0), pk('audio', 1)], {
        video: undefined,
        audio: {
          config: { codec: 'ac-3', sampleRate: 44100, numberOfChannels: 6 },
          sampleRate: 44100,
          channels: 6,
        },
      }),
      plan: transcodePlan({ video: null }),
      api,
    });
    expect(log.audioEncoded).toEqual([
      { rate: 48000, ch: 2 },
      { rate: 48000, ch: 2 },
    ]);
  });

  it('copies video and audio chunks, giving the muxer the source config once', async () => {
    const { api, log } = makeApi();
    const res = await transcodeWithWebCodecs({
      source: source([pk('video', 0), pk('video', 1, false), pk('audio', 0), pk('audio', 1)], {
        video: {
          config: { codec: 'avc1.640028', codedWidth: 1280, codedHeight: 720 },
          width: 1280,
          height: 720,
        },
        audio: {
          config: { codec: 'mp4a.40.2', sampleRate: 48000, numberOfChannels: 2 },
          sampleRate: 48000,
          channels: 2,
        },
      }),
      plan: transcodePlan({
        video: { mode: 'copy', codec: 'avc1.640028' },
        audio: { mode: 'copy', codec: 'mp4a.40.2' },
      }),
      api,
    });
    expect(res.videoFrames).toBe(2);
    expect(res.audioFrames).toBe(2);
    expect(log.muxSetup?.video?.codec).toBe('avc');
    expect(log.muxSetup?.audio?.codec).toBe('aac');
    expect(log.muxVideo[0]!.meta?.decoderConfig?.codec).toBe('avc1.640028');
    expect(log.muxVideo[1]!.meta).toBeUndefined();
    expect(log.muxAudio[0]!.meta?.decoderConfig?.codec).toBe('mp4a.40.2');
    expect(log.scaled).toBe(0);
  });

  it('reports a decoder failure as a WebCodecsError with code decode', async () => {
    const { api } = makeApi({ videoDecodeError: true });
    const list = [pk('video', 0), pk('video', 1), pk('video', 2)];
    await expect(
      transcodeWithWebCodecs({
        source: source(list),
        plan: transcodePlan({ audio: { mode: 'none' } }),
        api,
      }),
    ).rejects.toMatchObject({ name: 'WebCodecsError', code: 'decode' });
  });

  it('stops on abort and closes the codecs', async () => {
    const { api, log } = makeApi();
    const ctl = new AbortController();
    async function* slow(): AsyncGenerator<Packet> {
      yield pk('video', 0);
      ctl.abort();
      yield pk('video', 1);
    }
    const err = await transcodeWithWebCodecs({
      source: source([], { packets: slow() }),
      plan: transcodePlan({ audio: { mode: 'none' } }),
      api,
      signal: ctl.signal,
    }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(WebCodecsError);
    expect((err as WebCodecsError).code).toBe('aborted');
    expect(log.closed).toContain('vdec');
    expect(log.finalized).toBe(false);
  });

  it('refuses a plan the source cannot satisfy', async () => {
    const { api } = makeApi();
    await expect(
      transcodeWithWebCodecs({
        source: source([], { video: undefined }),
        plan: transcodePlan(),
        api,
      }),
    ).rejects.toMatchObject({ code: 'unsupported' });
  });
});

describe('audio conversion', () => {
  it('downmixes 5.1 to stereo without clipping and drops the LFE', () => {
    const one = (v: number) => new Float32Array([v, v]);
    const [l, r] = downmix([one(1), one(1), one(1), one(1), one(1), one(1)], 2);
    expect(l![0]).toBeCloseTo(1, 5);
    expect(r![1]).toBeCloseTo(1, 5);
    const [only] = downmix([one(0), one(0), one(0), one(1), one(0), one(0)], 2);
    expect(only![0]).toBe(0);
  });
  it('averages stereo to mono and passes matching layouts through', () => {
    const a = new Float32Array([1, 0]);
    const b = new Float32Array([0, 1]);
    expect(Array.from(downmix([a, b], 1)[0]!)).toEqual([0.5, 0.5]);
    expect(downmix([a, b], 2)).toEqual([a, b]);
  });
  it('resamples to the right length and keeps a constant signal constant', () => {
    const out = resample(new Float32Array(441).fill(0.25), 44100, 48000);
    expect(out.length).toBe(480);
    expect(out.every((s) => Math.abs(s - 0.25) < 1e-6)).toBe(true);
    expect(convertPlanes([new Float32Array(4)], 48000, 48000, 1)[0]!.length).toBe(4);
  });
});
