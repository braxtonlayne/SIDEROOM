import { afterEach, describe, expect, it, vi } from 'vitest';
import { FRAME_SAMPLES, SAMPLE_RATE } from '../constants';
import { FrameResampler } from '../resampler';

function collect(rate: number) {
  const frames: Float32Array[] = [];
  const r = new FrameResampler(rate, SAMPLE_RATE, FRAME_SAMPLES, (f) => frames.push(f));
  return { frames, r };
}

describe('FrameResampler (D-1050)', () => {
  it('cuts 512-sample frames: 48 kHz gives one frame per 1536 input samples', () => {
    const { frames, r } = collect(48_000);
    r.push(new Float32Array(1535).fill(0.25));
    expect(frames).toHaveLength(0);
    r.push(new Float32Array(1).fill(0.25));
    expect(frames).toHaveLength(1);
    expect(frames[0]!.length).toBe(FRAME_SAMPLES);
    expect(frames[0]![0]).toBeCloseTo(0.25, 5);
    expect(frames[0]![511]).toBeCloseTo(0.25, 5);
  });

  it('handles a rate that is not a multiple (44.1 kHz) without drifting', () => {
    const { frames, r } = collect(44_100);
    const block = new Float32Array(128).fill(0.5);
    const seconds = 10;
    for (let i = 0; i < (44_100 * seconds) / 128; i++) r.push(block);
    // 16000 / 512 = 31.25 frames per second.
    expect(frames.length).toBeGreaterThanOrEqual(311);
    expect(frames.length).toBeLessThanOrEqual(313);
    for (const f of frames)
      for (const v of [f[0]!, f[255]!, f[511]!]) expect(v).toBeCloseTo(0.5, 4);
  });

  it('keeps a 500 Hz tone at its amplitude and attenuates a tone above the new Nyquist', () => {
    const rms = (f: Float32Array) => Math.sqrt(f.reduce((a, v) => a + v * v, 0) / f.length);
    const tone = (hz: number) => {
      const { frames, r } = collect(48_000);
      const x = new Float32Array(48_000);
      for (let i = 0; i < x.length; i++) x[i] = Math.sin((2 * Math.PI * hz * i) / 48_000);
      r.push(x);
      return rms(frames[10]!);
    };
    expect(tone(500)).toBeGreaterThan(0.69); // 0.707 for a unit sine
    expect(tone(23_000)).toBeLessThan(0.35); // a box filter lessens aliasing, it does not remove it
  });

  it('hands over a fresh buffer each frame (the old one may be transferred away)', () => {
    const { frames, r } = collect(16_000);
    r.push(new Float32Array(FRAME_SAMPLES * 2).fill(1));
    expect(frames).toHaveLength(2);
    expect(frames[0]!.buffer).not.toBe(frames[1]!.buffer);
  });
});

describe('the frames worklet (D-1050)', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.resetModules();
  });

  it('registers one processor, averages the channels and posts transferable frames to its port', async () => {
    const posted: { data: Float32Array; transfer: unknown[] }[] = [];
    let registered: {
      name: string;
      ctor: new () => { process(i: Float32Array[][]): boolean };
    } | null = null;
    class FakeProcessor {
      port = {
        postMessage: (data: Float32Array, transfer: unknown[]) => posted.push({ data, transfer }),
      };
    }
    vi.stubGlobal('AudioWorkletProcessor', FakeProcessor);
    vi.stubGlobal('sampleRate', 48_000);
    vi.stubGlobal('registerProcessor', (name: string, ctor: never) => {
      registered = { name, ctor };
    });
    await import('../frames.worklet');
    expect(registered!.name).toBe('sr-silero-frames');
    const p = new registered!.ctor();
    const left = new Float32Array(128).fill(0.2);
    const right = new Float32Array(128).fill(0.6);
    for (let i = 0; i < 12; i++) expect(p.process([[left, right]])).toBe(true);
    expect(posted).toHaveLength(1); // 12 * 128 = 1536 samples = one 32 ms frame
    expect(posted[0]!.data[7]).toBeCloseTo(0.4, 5);
    expect(posted[0]!.transfer).toEqual([posted[0]!.data.buffer]);
    expect(p.process([[]])).toBe(true); // no channel yet: keeps running
  });
});
