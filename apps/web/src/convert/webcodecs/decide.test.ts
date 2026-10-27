import { describe, expect, it } from 'vitest';
import {
  codecFamily,
  decide,
  describeDecision,
  estimateWasmMinutes,
  ladderSize,
  pickLadderStep,
} from '../decide';
import type { Capabilities, SourceInfo } from './types';

interface Fake {
  play?: string[]; // substrings of mime strings that play
  vdec?: string[];
  adec?: string[];
  venc?: string[];
  aenc?: string[];
}

/** A fake browser: each list holds codec prefixes it supports. */
function caps(f: Fake, engine: Capabilities['engine'] = 'chromium'): Capabilities {
  const has = (list: string[] | undefined, codec: string) =>
    !!list?.some((p) => codec.startsWith(p));
  return {
    engine,
    canPlay: (m) => !!f.play?.some((s) => m.includes(s)),
    videoDecode: async (c) => has(f.vdec, c.codec),
    audioDecode: async (c) => has(f.adec, c.codec),
    videoEncode: async (c) => has(f.venc, c.codec),
    audioEncode: async (c) => has(f.aenc, c.codec),
  };
}

const hevcMkv: SourceInfo = {
  container: 'mkv',
  video: { codec: 'hvc1.1.6.L120.90', width: 1920, height: 1080, frameRate: 24 },
  audio: { codec: 'mp4a.40.2', sampleRate: 48000, channels: 2 },
  durationSec: 600,
  sizeBytes: 1e9,
};

describe('decide', () => {
  it('plays a native mp4 as is', async () => {
    const src: SourceInfo = {
      ...hevcMkv,
      container: 'mp4',
      video: { codec: 'avc1.640028', width: 1280, height: 720 },
    };
    const d = await decide(src, caps({ play: ['avc1.640028', 'mp4a.40.2'] }));
    expect(d.path).toBe('native');
  });

  it('remuxes mkv with playable codecs into mp4', async () => {
    const src: SourceInfo = {
      ...hevcMkv,
      video: { codec: 'avc1.640028', width: 1280, height: 720 },
    };
    const d = await decide(src, caps({ play: ['avc1.640028', 'mp4a.40.2'] }));
    expect(d).toMatchObject({ path: 'remux', container: 'mp4' });
  });

  it('remuxes vp9 and opus mkv into webm when mp4 will not play it', async () => {
    const src: SourceInfo = {
      ...hevcMkv,
      video: { codec: 'vp09.00.10.08', width: 1280, height: 720 },
      audio: { codec: 'opus', sampleRate: 48000, channels: 2 },
    };
    const d = await decide(
      src,
      caps({ play: ['video/webm; codecs="vp09.00.10.08"', 'audio/webm; codecs="opus"'] }),
    );
    expect(d).toMatchObject({ path: 'remux', container: 'webm' });
  });

  it('transcodes H.265 to H.264 mp4 on a browser without HEVC playback (Chromium)', async () => {
    const d = await decide(
      hevcMkv,
      caps({ play: ['mp4a.40.2'], vdec: ['hvc1'], venc: ['avc1'], aenc: ['mp4a'] }),
    );
    expect(d).toMatchObject({
      path: 'webcodecs',
      container: 'mp4',
      video: { mode: 'transcode', to: 'avc', step: { label: '1080p' } },
      audio: { mode: 'copy' },
    });
  });

  it('copies video and transcodes AC-3 audio to AAC', async () => {
    const src: SourceInfo = {
      ...hevcMkv,
      video: { codec: 'avc1.640028', width: 1920, height: 1080 },
      audio: { codec: 'ac-3', sampleRate: 48000, channels: 6 },
    };
    const d = await decide(src, caps({ play: ['avc1.640028'], adec: ['ac-3'], aenc: ['mp4a'] }));
    expect(d).toMatchObject({
      path: 'webcodecs',
      container: 'mp4',
      video: { mode: 'copy' },
      audio: { mode: 'transcode', to: 'aac', channels: 2 },
    });
  });

  it('falls back to opus in webm when AAC cannot be encoded (Firefox shape)', async () => {
    const d = await decide(
      { ...hevcMkv, audio: { codec: 'ac-3', sampleRate: 48000, channels: 2 } },
      caps({ vdec: ['hvc1'], adec: ['ac-3'], venc: ['vp09'], aenc: ['opus'] }, 'gecko'),
    );
    expect(d).toMatchObject({
      path: 'webcodecs',
      container: 'webm',
      video: { mode: 'transcode', to: 'vp9' },
      audio: { mode: 'transcode', to: 'opus' },
    });
  });

  it('goes to the worker when the source cannot be decoded here', async () => {
    const d = await decide(
      hevcMkv,
      caps({ play: ['mp4a.40.2'], venc: ['avc1'], aenc: ['mp4a'] }, 'webkit'),
    );
    expect(d.path).toBe('worker');
    if (d.path === 'worker') {
      expect(d.estimateMinutes).toBeGreaterThan(10);
      expect(d.reason).toContain('hvc1');
    }
  });

  it('goes to the worker with no WebCodecs at all', async () => {
    const d = await decide(hevcMkv, caps({}));
    expect(d.path).toBe('worker');
  });

  it('handles audio-only sources', async () => {
    const src: SourceInfo = {
      container: 'mkv',
      audio: { codec: 'ac-3', sampleRate: 48000, channels: 2 },
      durationSec: 200,
      sizeBytes: 1e7,
    };
    const d = await decide(src, caps({ adec: ['ac-3'], aenc: ['mp4a'] }));
    expect(d).toMatchObject({ path: 'webcodecs', video: null, audio: { to: 'aac' } });
  });

  it('describes the path per engine', async () => {
    const d = await decide(hevcMkv, caps({}, 'gecko'));
    expect(describeDecision('gecko', d)).toMatch(/^gecko:worker - /);
  });
});

describe('ladder and estimate', () => {
  it('picks the step from the source height and never upscales', () => {
    expect(pickLadderStep(360).label).toBe('480p');
    expect(pickLadderStep(720).label).toBe('720p');
    expect(pickLadderStep(1080).label).toBe('1080p');
    expect(pickLadderStep(2160).label).toBe('1080p');
    expect(ladderSize({ width: 640, height: 360 }, pickLadderStep(360))).toEqual({
      width: 640,
      height: 360,
    });
    expect(ladderSize({ width: 3840, height: 2160 }, pickLadderStep(2160))).toEqual({
      width: 1920,
      height: 1080,
    });
    expect(ladderSize({ width: 1000, height: 700 }, pickLadderStep(700))).toEqual({
      width: 1000,
      height: 700,
    });
  });
  it('estimates wasm minutes from duration and size', () => {
    const video = { codec: 'x', width: 1280, height: 720 };
    expect(estimateWasmMinutes({ ...hevcMkv, video, durationSec: 600 })).toBe(50);
    expect(estimateWasmMinutes({ ...hevcMkv, video: undefined, durationSec: 30 })).toBe(1);
  });
  it('names codec families', () => {
    expect(codecFamily('hev1.1.6.L93.B0')).toBe('hevc');
    expect(codecFamily('mp4a.40.5')).toBe('aac');
    expect(codecFamily('ec-3')).toBe('eac3');
  });
});

describe('decide: audio copy is limited to what the muxers can write', () => {
  it('re-encodes playable MP3 when video needs transcoding', async () => {
    const d = await decide(
      { ...hevcMkv, audio: { codec: 'mp3', sampleRate: 44100, channels: 2 } },
      caps({
        play: ['audio/mp4; codecs="mp3"'],
        vdec: ['hvc1'],
        venc: ['avc1'],
        adec: ['mp3'],
        aenc: ['mp4a'],
      }),
    );
    expect(d).toMatchObject({ path: 'webcodecs', audio: { mode: 'transcode', to: 'aac' } });
  });
});
