import { describe, expect, it } from 'vitest';
import {
  durationOf,
  encodeWav,
  maxStart,
  peaks,
  trimToWav,
  wavBytes,
  type PcmSource,
} from '../sections/soundTrim';

/** A fixture sound: `seconds` long at `rate` Hz, a ramp on the left channel and its negative on the right. */
function sound(seconds: number, rate = 8000, channels = 2): PcmSource {
  const length = Math.round(seconds * rate);
  const data = Array.from({ length: channels }, (_, c) => {
    const d = new Float32Array(length);
    for (let i = 0; i < length; i++) d[i] = (c === 0 ? 1 : -1) * (i / length);
    return d;
  });
  return {
    sampleRate: rate,
    length,
    numberOfChannels: channels,
    getChannelData: (c) => data[c]!,
  };
}

const view = (b: Uint8Array) => new DataView(b.buffer, b.byteOffset, b.byteLength);
const tag = (b: Uint8Array, at: number) => String.fromCharCode(...b.slice(at, at + 4));

describe('soundboard trimmer (J-06, D-201)', () => {
  it('measures the sound and the last start that leaves a whole window', () => {
    const s = sound(12);
    expect(durationOf(s)).toBe(12);
    expect(maxStart(s, 5)).toBe(7);
    expect(maxStart(sound(3), 5)).toBe(0);
  });

  it('finds the peak of each slice over all channels', () => {
    const s = sound(1, 100);
    const p = peaks(s, 4);
    expect(p).toHaveLength(4);
    // The ramp rises, so each slice peaks higher than the one before.
    expect(p[0]!).toBeLessThan(p[1]!);
    expect(p[3]!).toBeGreaterThan(0.9);
    expect(peaks(sound(0, 100), 3)).toEqual(new Float32Array(3));
  });

  it('writes a 16-bit PCM WAV at the source rate, exactly the window long', () => {
    const s = sound(12, 8000, 2);
    const wav = encodeWav(s, 2, 5, false);
    const v = view(wav);
    expect(tag(wav, 0)).toBe('RIFF');
    expect(tag(wav, 8)).toBe('WAVE');
    expect(tag(wav, 12)).toBe('fmt ');
    expect(v.getUint16(20, true)).toBe(1); // PCM
    expect(v.getUint16(22, true)).toBe(2); // channels
    expect(v.getUint32(24, true)).toBe(8000); // the source rate
    expect(v.getUint32(28, true)).toBe(8000 * 2 * 2); // byte rate
    expect(v.getUint16(34, true)).toBe(16);
    expect(tag(wav, 36)).toBe('data');
    expect(v.getUint32(40, true)).toBe(5 * 8000 * 2 * 2);
    expect(wav.byteLength).toBe(wavBytes(5 * 8000, 2));
    expect(v.getUint32(4, true)).toBe(wav.byteLength - 8);
    // The first frame is the sample at 2 s: left = +2/12, right = -2/12.
    expect(v.getInt16(44, true)).toBeCloseTo((2 / 12) * 0x7fff, -1);
    expect(v.getInt16(46, true)).toBeCloseTo((-2 / 12) * 0x8000, -1);
  });

  it('clips a window that runs past the end and clamps loud samples', () => {
    const s = sound(6, 1000, 1);
    s.getChannelData(0)[5999] = 3; // over full scale
    const wav = encodeWav(s, 4, 5, false);
    expect(view(wav).getUint32(40, true)).toBe(2 * 1000 * 2);
    expect(view(wav).getInt16(wav.byteLength - 2, true)).toBe(0x7fff);
  });

  it('keeps stereo when it fits, mixes to mono when only that fits, refuses with the size otherwise', () => {
    // 5 s at 22.05 kHz stereo is 441 KB: fits in 512 KiB.
    const small = sound(8, 22050, 2);
    const stereo = trimToWav(small, 0, 5, 512 * 1024);
    expect(stereo.ok && stereo.mono).toBe(false);
    // 5 s at 44.1 kHz stereo is 882 KB: too big, but mono (441 KB) fits.
    const wide = sound(8, 44100, 2);
    const mono = trimToWav(wide, 1, 5, 512 * 1024);
    expect(mono.ok).toBe(true);
    if (mono.ok) {
      expect(mono.mono).toBe(true);
      expect(view(mono.bytes).getUint16(22, true)).toBe(1);
      expect(mono.bytes.byteLength).toBeLessThanOrEqual(512 * 1024);
    }
    // 5 s at 96 kHz mono is 960 KB: not even mono fits; the size comes back.
    const huge = sound(6, 96000, 1);
    const refused = trimToWav(huge, 0, 5, 512 * 1024);
    expect(refused).toEqual({ ok: false, bytes: wavBytes(5 * 96000, 1) });
  });

  it('averages the channels when mixing down', () => {
    const s = sound(1, 100, 2); // the right channel is the negative of the left
    const wav = encodeWav(s, 0, 1, true);
    const v = view(wav);
    for (let i = 0; i < 100; i++) expect(v.getInt16(44 + i * 2, true)).toBe(0);
  });
});
