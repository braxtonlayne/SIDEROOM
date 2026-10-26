// DEV-FU-002: the TypeScript Opus encoder (opusscript, WebAssembly) and its cleanup.
import OpusScript from 'opusscript';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  AudioError,
  FRAME_SAMPLES,
  OpusDecoder,
  OpusEncoder,
  audioLevelDbov,
  type Closeable,
} from './audio';
import { VoiceSession } from './voiceTransport';

const tone = (frames: number, channels = 1): Int16Array => {
  const out = new Int16Array(frames * FRAME_SAMPLES * channels);
  for (let i = 0; i < frames * FRAME_SAMPLES; i++) {
    const v = Math.round(Math.sin((2 * Math.PI * 440 * i) / 48000) * 12000);
    for (let c = 0; c < channels; c++) out[i * channels + c] = v;
  }
  return out;
};

/** Normalised correlation at the best lag up to `maxLag` (the codec delays the signal). */
function bestCorrelation(a: Int16Array, b: Int16Array, maxLag: number): number {
  let best = -1;
  for (let lag = 0; lag <= maxLag; lag++) {
    let ab = 0;
    let aa = 0;
    let bb = 0;
    const n = Math.min(a.length, b.length - lag);
    for (let i = 0; i < n; i++) {
      const x = a[i]!;
      const y = b[i + lag]!;
      ab += x * y;
      aa += x * x;
      bb += y * y;
    }
    if (aa > 0 && bb > 0) best = Math.max(best, ab / Math.sqrt(aa * bb));
  }
  return best;
}

afterEach(() => vi.restoreAllMocks());

describe('OpusEncoder and OpusDecoder', () => {
  it('a tone survives the round trip (correlation, not byte equality)', () => {
    const enc = new OpusEncoder(1, 'audio');
    const dec = new OpusDecoder(1);
    const pcm = tone(10);
    const out = new Int16Array(pcm.length);
    for (let k = 0; k < 10; k++) {
      const packet = enc.encodeFrame(pcm.subarray(k * FRAME_SAMPLES, (k + 1) * FRAME_SAMPLES));
      expect(packet.length).toBeGreaterThan(0);
      expect(packet.length).toBeLessThan(400);
      const back = dec.decodeFrame(packet);
      expect(back.length).toBe(FRAME_SAMPLES);
      out.set(back, k * FRAME_SAMPLES);
    }
    const from = 3 * FRAME_SAMPLES;
    expect(bestCorrelation(pcm.subarray(from), out.subarray(from), 480)).toBeGreaterThan(0.9);
    const level = audioLevelDbov(out.subarray(from));
    expect(level).toBeGreaterThanOrEqual(-14);
    expect(level).toBeLessThanOrEqual(-6);
    enc.close();
    dec.close();
  });

  it('refuses a frame that is not exactly 20 ms', () => {
    const enc = new OpusEncoder(1, 'voip');
    expect(() => enc.encodeFrame(new Int16Array(480))).toThrow(AudioError);
    expect(() => enc.encodeFrame(new Int16Array(0))).toThrow(/960 samples \(0 given\)/);
    const stereo = new OpusEncoder(2, 'voip');
    expect(stereo.frameLen).toBe(1920);
    expect(() => stereo.encodeFrame(new Int16Array(960))).toThrow(AudioError);
    expect(stereo.encodeFrame(tone(1, 2)).length).toBeGreaterThan(0);
    enc.close();
    stereo.close();
  });

  it('takes its settings and decodes garbage to silence', () => {
    const enc = new OpusEncoder(1, 'voip');
    enc.setBitrate(24_000);
    enc.setInbandFec(true);
    const dec = new OpusDecoder(1);
    expect(dec.decodeFrame(new Uint8Array([0xff, 0xff, 0xff])).length).toBe(0);
    enc.close();
    dec.close();
  });

  it('computes the level in dBov', () => {
    expect(audioLevelDbov(new Int16Array(0))).toBe(-127);
    expect(audioLevelDbov(new Int16Array(960))).toBe(-127);
    expect(audioLevelDbov(new Int16Array(960).fill(32767))).toBe(0);
    expect(audioLevelDbov(new Int16Array(960).fill(16384))).toBe(-6);
  });
});

describe('cleanup (Emscripten frees nothing by itself)', () => {
  it('close() calls delete() once, however often it is called, and a closed coder refuses work', () => {
    const del = vi.spyOn(OpusScript.prototype, 'delete');
    const enc = new OpusEncoder(1, 'voip');
    const dec = new OpusDecoder(1);
    expect(del).not.toHaveBeenCalled();
    enc.close();
    enc.close();
    dec.close();
    dec.close();
    expect(del).toHaveBeenCalledTimes(2);
    expect(() => enc.encodeFrame(new Int16Array(FRAME_SAMPLES))).toThrow(/closed/);
    expect(() => dec.decodeFrame(new Uint8Array([1]))).toThrow(/closed/);
  });

  it('the session closes what it adopted when it leaves, and a rejoin hands it on', async () => {
    const del = vi.spyOn(OpusScript.prototype, 'delete');
    // A session without a network: leave() only touches these three members and the adopted set.
    const make = () => {
      const s = Object.create(VoiceSession.prototype) as VoiceSession;
      Object.assign(s, {
        heartbeat: null,
        ws: { close: vi.fn() },
        pc: { close: vi.fn(async () => undefined) },
        codecs: new Set<Closeable>(),
      });
      return s;
    };
    const old = make();
    const enc = old.adopt(new OpusEncoder(1, 'voip'));
    const dec = old.adopt(new OpusDecoder(1));
    // The rejoin path: the replacement takes the codecs, the old session's leave() then closes nothing.
    const fresh = make();
    for (const c of old.takeCodecs()) fresh.adopt(c);
    await old.leave();
    expect(del).not.toHaveBeenCalled();
    expect(enc.encodeFrame(new Int16Array(FRAME_SAMPLES)).length).toBeGreaterThan(0);
    await fresh.leave();
    expect(del).toHaveBeenCalledTimes(2);
    expect(() => dec.decodeFrame(new Uint8Array([1]))).toThrow(/closed/);
  });
});
