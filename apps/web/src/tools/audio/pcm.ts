// UTL-AUD-001: decoded audio as the audio tool sees it. The file is decoded with WebAudio at a fixed rate
// (an OfflineAudioContext, so exports are 48 kHz on every machine) and the waveform is drawn from its peaks.

/** What the audio tool needs of a decoded sound; an `AudioBuffer` satisfies it. */
export interface PcmSource {
  sampleRate: number;
  length: number;
  numberOfChannels: number;
  getChannelData(channel: number): Float32Array;
}

export const durationOf = (s: PcmSource) => s.length / s.sampleRate;

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

const readBytes = (file: Blob): Promise<ArrayBuffer> =>
  typeof file.arrayBuffer === 'function'
    ? file.arrayBuffer()
    : new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result as ArrayBuffer);
        reader.onerror = () => reject(new Error('read'));
        reader.readAsArrayBuffer(file);
      });

/** The rate every trimmed sound is decoded and exported at (K-03). */
export const TRIM_SAMPLE_RATE = 48000;

type DecodeCtx = Pick<BaseAudioContext, 'decodeAudioData'> & { close?: () => Promise<void> };

/**
 * Decodes an audio file with WebAudio at `TRIM_SAMPLE_RATE`. An OfflineAudioContext fixes its rate, so the exported
 * WAV is 48 kHz on every machine (a live context would use the sound card's rate). Falls back to the live context
 * where OfflineAudioContext is missing. Rejects when the browser cannot decode the file.
 */
export async function decodeAudioFile(file: Blob): Promise<PcmSource> {
  const w = globalThis as unknown as {
    OfflineAudioContext?: new (channels: number, length: number, rate: number) => DecodeCtx;
    webkitOfflineAudioContext?: new (channels: number, length: number, rate: number) => DecodeCtx;
    AudioContext?: new () => DecodeCtx;
    webkitAudioContext?: new () => DecodeCtx;
  };
  const Offline = w.OfflineAudioContext ?? w.webkitOfflineAudioContext;
  const Live = w.AudioContext ?? w.webkitAudioContext;
  if (!Offline && !Live) throw new Error('no WebAudio');
  const ctx = Offline ? new Offline(2, 1, TRIM_SAMPLE_RATE) : new Live!();
  try {
    return await ctx.decodeAudioData(await readBytes(file));
  } finally {
    void ctx.close?.();
  }
}
