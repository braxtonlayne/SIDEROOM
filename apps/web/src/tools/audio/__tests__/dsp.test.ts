import { describe, expect, it } from 'vitest';
import {
  applyFades,
  encodeWav,
  normalizeGain,
  processAudio,
  rmsDb,
  samplePeak,
  sliceChannels,
  truePeak,
  type PcmLike,
} from '../dsp';

const RATE = 8000;

/** A generated sine tone. */
function tone(seconds: number, amp: number, freq = 440, channels = 1): PcmLike {
  const length = Math.round(seconds * RATE);
  const data = Array.from({ length: channels }, () => {
    const a = new Float32Array(length);
    for (let i = 0; i < length; i++) a[i] = amp * Math.sin((2 * Math.PI * freq * i) / RATE);
    return a;
  });
  return {
    sampleRate: RATE,
    length,
    numberOfChannels: channels,
    getChannelData: (c) => data[c]!,
  };
}

const opts = {
  startSeconds: 0,
  endSeconds: 10,
  fadeInSeconds: 0,
  fadeOutSeconds: 0,
  targetDb: null,
  ceilingDb: -1,
};

describe('audio trimmer (UTL-AUD-001)', () => {
  it('trims to the exact length and clamps out-of-range handles', () => {
    const src = tone(2, 0.5);
    expect(sliceChannels(src, 0.5, 1.25)[0]!.length).toBe(0.75 * RATE);
    expect(sliceChannels(src, -3, 99)[0]!.length).toBe(2 * RATE);
    expect(sliceChannels(src, 1.5, 0.5)[0]!.length).toBe(0);
    expect(
      processAudio(tone(2, 0.5, 440, 2), { ...opts, startSeconds: 1, endSeconds: 2 }).channels,
    ).toHaveLength(2);
  });

  it('measures a sine tone: RMS is amplitude minus 3 dB', () => {
    const rms = rmsDb([tone(1, 0.5).getChannelData(0)]);
    expect(rms).toBeCloseTo(20 * Math.log10(0.5 / Math.SQRT2), 1);
    expect(rmsDb([new Float32Array(10)])).toBe(-Infinity);
  });

  it('fades in and out without overlapping', () => {
    const ch = [new Float32Array(100).fill(1)];
    applyFades(ch, 100, 0.2, 0.2);
    expect(ch[0]![0]).toBe(0);
    expect(ch[0]![10]).toBeCloseTo(0.5);
    expect(ch[0]![50]).toBe(1);
    expect(ch[0]![99]).toBe(0);
    const both = [new Float32Array(100).fill(1)];
    applyFades(both, 100, 5, 5);
    expect(Math.max(...both[0]!)).toBeLessThanOrEqual(1);
    expect(both[0]![0]).toBe(0);
  });

  it('applies the gain that reaches the RMS target', () => {
    const src = tone(1, 0.05);
    const out = processAudio(src, { ...opts, targetDb: -20 });
    expect(out.limited).toBe(false);
    expect(rmsDb(out.channels)).toBeCloseTo(-20, 1);
    expect(out.gainDb).toBeCloseTo(-20 - rmsDb([src.getChannelData(0)]), 1);
  });

  it('the peak guard stops the gain below the ceiling', () => {
    // A quiet tone with one loud click: RMS is low, so the target asks for a lot of gain.
    const src = tone(1, 0.01);
    src.getChannelData(0)[1000] = 0.5;
    const out = processAudio(src, { ...opts, targetDb: -6, ceilingDb: -1 });
    expect(out.limited).toBe(true);
    expect(truePeak(out.channels)).toBeLessThanOrEqual(10 ** (-1 / 20) + 1e-4);
    expect(rmsDb(out.channels)).toBeLessThan(-6);
  });

  it('true peak sees inter-sample peaks the samples miss', () => {
    // A tone at a quarter of the rate sampled 45 degrees off its crest: samples peak at 0.707.
    const n = 64;
    const a = new Float32Array(n);
    for (let i = 0; i < n; i++) a[i] = Math.sin((Math.PI / 2) * i + Math.PI / 4);
    expect(samplePeak([a])).toBeCloseTo(Math.SQRT1_2, 3);
    expect(truePeak([a])).toBeGreaterThan(samplePeak([a]) * 1.2); // the cubic estimate is about 0.88 of the true 1.0
  });

  it('leaves silence alone', () => {
    expect(normalizeGain([new Float32Array(50)], -16, -1)).toEqual({ gain: 1, limited: false });
  });

  it('writes a valid 16-bit WAV, stereo or mono', () => {
    const wav = encodeWav(
      [new Float32Array([0, 1, -1, 0.5]), new Float32Array([0, 0, 0, 0])],
      RATE,
    );
    const v = new DataView(wav.buffer);
    expect(String.fromCharCode(...wav.slice(0, 4))).toBe('RIFF');
    expect(String.fromCharCode(...wav.slice(8, 12))).toBe('WAVE');
    expect(v.getUint16(22, true)).toBe(2);
    expect(v.getUint32(24, true)).toBe(RATE);
    expect(v.getUint32(40, true)).toBe(4 * 2 * 2);
    expect(wav.length).toBe(44 + 16);
    expect(v.getInt16(48, true)).toBe(32767);
    expect(v.getInt16(52, true)).toBe(-32768);
    const mono = encodeWav([new Float32Array([1]), new Float32Array([0])], RATE, true);
    expect(new DataView(mono.buffer).getUint16(22, true)).toBe(1);
    expect(new DataView(mono.buffer).getInt16(44, true)).toBe(16384);
  });
});
