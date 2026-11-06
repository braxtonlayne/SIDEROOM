import { afterEach, describe, expect, it, vi } from 'vitest';
import { TRIM_SAMPLE_RATE, decodeAudioFile, durationOf, peaks, type PcmSource } from '../pcm';
import { encodeWav } from '../dsp';

function sound(seconds: number, rate = 8000): PcmSource {
  const length = Math.round(seconds * rate);
  const d = new Float32Array(length);
  for (let i = 0; i < length; i++) d[i] = i / length;
  return { sampleRate: rate, length, numberOfChannels: 1, getChannelData: () => d };
}

describe('pcm helpers (UTL-AUD-001)', () => {
  it('measures the duration and the peak of each slice', () => {
    const s = sound(2);
    expect(durationOf(s)).toBe(2);
    const p = peaks(s, 4);
    expect(p).toHaveLength(4);
    expect(p[3]!).toBeGreaterThan(p[0]!);
    expect(p[3]!).toBeLessThanOrEqual(1);
    expect(peaks({ ...s, length: 0 }, 4)).toEqual(new Float32Array(4));
  });
});

describe('decodeAudioFile (K-03)', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('decodes in an offline context pinned to 48 kHz, not the live one', async () => {
    const made: unknown[][] = [];
    class Offline {
      constructor(...args: unknown[]) {
        made.push(args);
      }
      decodeAudioData = async () => ({
        sampleRate: TRIM_SAMPLE_RATE,
        length: 1,
        numberOfChannels: 2,
        getChannelData: () => new Float32Array(1),
      });
    }
    const live = vi.fn();
    vi.stubGlobal('OfflineAudioContext', Offline);
    vi.stubGlobal('AudioContext', live);
    const out = await decodeAudioFile(new Blob(['x']));
    expect(out.sampleRate).toBe(48000);
    expect(made).toEqual([[2, 1, 48000]]);
    expect(live).not.toHaveBeenCalled();
    const wav = encodeWav([out.getChannelData(0), out.getChannelData(1)], out.sampleRate);
    expect(new DataView(wav.buffer).getUint32(24, true)).toBe(48000);
  });
});
