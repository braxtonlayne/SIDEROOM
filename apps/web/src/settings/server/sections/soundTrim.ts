// Soundboard trimmer (J-06, D-201): the server never transcodes, so a sound longer than five seconds is cut in the
// browser. The file is decoded with WebAudio, the person picks a five second window on a waveform, and the window is
// re-encoded as 16-bit PCM WAV at the decoded sample rate (the WebAudio context's rate, the same as the source's
// where the browser keeps it). Stereo first; a window that is over the size limit is mixed down to mono before the
// upload is refused with its size.

/** What the trimmer needs of a decoded sound; an `AudioBuffer` satisfies it. */
export interface PcmSource {
  sampleRate: number;
  length: number;
  numberOfChannels: number;
  getChannelData(channel: number): Float32Array;
}

export const durationOf = (s: PcmSource) => s.length / s.sampleRate;

/** Largest start (seconds) that still leaves a full window; 0 when the sound is not longer than the window. */
export const maxStart = (s: PcmSource, windowSeconds: number) =>
  Math.max(0, durationOf(s) - windowSeconds);

/** Peak amplitude (0 to 1) of each of `bins` equal slices of the sound, over all channels. */
export function peaks(s: PcmSource, bins: number): Float32Array {
  const out = new Float32Array(bins);
  if (s.length === 0 || bins <= 0) return out;
  const per = s.length / bins;
  for (let c = 0; c < s.numberOfChannels; c++) {
    const data = s.getChannelData(c);
    for (let b = 0; b < bins; b++) {
      const from = Math.floor(b * per);
      const to = Math.min(s.length, Math.max(from + 1, Math.floor((b + 1) * per)));
      let peak = out[b]!;
      for (let i = from; i < to; i++) {
        const v = Math.abs(data[i]!);
        if (v > peak) peak = v;
      }
      out[b] = Math.min(1, peak);
    }
  }
  return out;
}

const WAV_HEADER = 44;

/** Size in bytes of a PCM WAV of `frames` frames. */
export const wavBytes = (frames: number, channels: number) => WAV_HEADER + frames * channels * 2;

/**
 * 16-bit PCM WAV of the window `[startSeconds, startSeconds + windowSeconds)`, clipped to the sound. With `mono`
 * the channels are averaged. The sample rate is the source's.
 */
export function encodeWav(
  s: PcmSource,
  startSeconds: number,
  windowSeconds: number,
  mono: boolean,
): Uint8Array {
  const first = Math.min(s.length, Math.max(0, Math.round(startSeconds * s.sampleRate)));
  const frames = Math.min(s.length - first, Math.floor(windowSeconds * s.sampleRate));
  const channels = mono ? 1 : s.numberOfChannels;
  const out = new Uint8Array(wavBytes(frames, channels));
  const v = new DataView(out.buffer);
  const text = (at: number, str: string) => {
    for (let i = 0; i < str.length; i++) v.setUint8(at + i, str.charCodeAt(i));
  };
  const dataBytes = frames * channels * 2;
  text(0, 'RIFF');
  v.setUint32(4, 36 + dataBytes, true);
  text(8, 'WAVE');
  text(12, 'fmt ');
  v.setUint32(16, 16, true);
  v.setUint16(20, 1, true); // PCM
  v.setUint16(22, channels, true);
  v.setUint32(24, s.sampleRate, true);
  v.setUint32(28, s.sampleRate * channels * 2, true);
  v.setUint16(32, channels * 2, true);
  v.setUint16(34, 16, true);
  text(36, 'data');
  v.setUint32(40, dataBytes, true);
  const data = Array.from({ length: s.numberOfChannels }, (_, c) => s.getChannelData(c));
  const sample = (x: number) => {
    const c = Math.max(-1, Math.min(1, x));
    return Math.round(c < 0 ? c * 0x8000 : c * 0x7fff);
  };
  let at = WAV_HEADER;
  for (let i = 0; i < frames; i++) {
    if (mono) {
      let sum = 0;
      for (const ch of data) sum += ch[first + i]!;
      v.setInt16(at, sample(sum / data.length), true);
      at += 2;
    } else {
      for (const ch of data) {
        v.setInt16(at, sample(ch[first + i]!), true);
        at += 2;
      }
    }
  }
  return out;
}

export type TrimResult =
  { ok: true; bytes: Uint8Array; mono: boolean } | { ok: false; bytes: number };

/**
 * Cuts the window and encodes it so it fits `maxBytes`: with the source's channels if that fits, else mixed to mono.
 * When even mono is too big, `bytes` is the size of the mono file.
 */
export function trimToWav(
  s: PcmSource,
  startSeconds: number,
  windowSeconds: number,
  maxBytes: number,
): TrimResult {
  const frames = Math.min(
    s.length - Math.min(s.length, Math.max(0, Math.round(startSeconds * s.sampleRate))),
    Math.floor(windowSeconds * s.sampleRate),
  );
  if (wavBytes(frames, s.numberOfChannels) <= maxBytes) {
    return { ok: true, bytes: encodeWav(s, startSeconds, windowSeconds, false), mono: false };
  }
  if (s.numberOfChannels > 1 && wavBytes(frames, 1) <= maxBytes) {
    return { ok: true, bytes: encodeWav(s, startSeconds, windowSeconds, true), mono: true };
  }
  return { ok: false, bytes: wavBytes(frames, 1) };
}

const readBytes = (file: Blob): Promise<ArrayBuffer> =>
  typeof file.arrayBuffer === 'function'
    ? file.arrayBuffer()
    : new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result as ArrayBuffer);
        reader.onerror = () => reject(new Error('read'));
        reader.readAsArrayBuffer(file);
      });

/** Decodes an audio file with WebAudio. Rejects when the browser cannot decode it. */
export async function decodeAudioFile(file: Blob): Promise<PcmSource> {
  const w = globalThis as unknown as {
    AudioContext?: new () => AudioContext;
    webkitAudioContext?: new () => AudioContext;
  };
  const Ctx = w.AudioContext ?? w.webkitAudioContext;
  if (!Ctx) throw new Error('no WebAudio');
  const ctx = new Ctx();
  try {
    return await ctx.decodeAudioData(await readBytes(file));
  } finally {
    void ctx.close?.();
  }
}
