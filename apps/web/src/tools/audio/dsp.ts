// UTL-AUD-001: the audio trimmer and normalizer. Pure sample math (no WebAudio), so it is testable with a
// generated tone. Loudness is a plain RMS gain with a true-peak guard; it is NOT EBU R128 (the UI says so).

export type Channels = Float32Array[];

/** What the tool needs of a decoded sound; an `AudioBuffer` satisfies it. */
export interface PcmLike {
  sampleRate: number;
  length: number;
  numberOfChannels: number;
  getChannelData(channel: number): Float32Array;
}

export interface ProcessOptions {
  /** Start and end of the kept part, in seconds. */
  startSeconds: number;
  endSeconds: number;
  fadeInSeconds: number;
  fadeOutSeconds: number;
  /** RMS target in dBFS, or null for no normalization. */
  targetDb: number | null;
  /** The (approximate) true-peak ceiling in dBFS. */
  ceilingDb: number;
}

export interface Processed {
  channels: Channels;
  sampleRate: number;
  /** The gain that was applied, in dB (0 without normalization). */
  gainDb: number;
  /** True when the peak guard lowered the gain below what the target asked for. */
  limited: boolean;
}

export const DEFAULT_TARGET_DB = -16;
export const DEFAULT_CEILING_DB = -1;

const toLinear = (db: number) => 10 ** (db / 20);
const toDb = (lin: number) => (lin > 0 ? 20 * Math.log10(lin) : -Infinity);

/** Copies `[start, end)` seconds of every channel, clamped to the sound. */
export function sliceChannels(src: PcmLike, startSeconds: number, endSeconds: number): Channels {
  const from = Math.min(src.length, Math.max(0, Math.round(startSeconds * src.sampleRate)));
  const to = Math.min(src.length, Math.max(from, Math.round(endSeconds * src.sampleRate)));
  return Array.from({ length: src.numberOfChannels }, (_, c) =>
    src.getChannelData(c).slice(from, to),
  );
}

/** Linear fade in and fade out, in place. The fades never overlap: together they are cut to the length. */
export function applyFades(
  channels: Channels,
  sampleRate: number,
  fadeInSeconds: number,
  fadeOutSeconds: number,
): void {
  const len = channels[0]?.length ?? 0;
  let inN = Math.min(len, Math.max(0, Math.round(fadeInSeconds * sampleRate)));
  let outN = Math.min(len, Math.max(0, Math.round(fadeOutSeconds * sampleRate)));
  if (inN + outN > len) {
    const scale = len / (inN + outN);
    inN = Math.floor(inN * scale);
    outN = Math.floor(outN * scale);
  }
  for (const ch of channels) {
    for (let i = 0; i < inN; i++) ch[i]! *= i / inN;
    for (let i = 0; i < outN; i++) ch[len - 1 - i]! *= i / outN;
  }
}

/** RMS over all channels, in dBFS (-Infinity for silence). */
export function rmsDb(channels: Channels): number {
  let sum = 0;
  let n = 0;
  for (const ch of channels) {
    for (let i = 0; i < ch.length; i++) sum += ch[i]! * ch[i]!;
    n += ch.length;
  }
  return n === 0 ? -Infinity : toDb(Math.sqrt(sum / n));
}

/** Highest absolute sample, linear. */
export function samplePeak(channels: Channels): number {
  let peak = 0;
  for (const ch of channels)
    for (let i = 0; i < ch.length; i++) peak = Math.max(peak, Math.abs(ch[i]!));
  return peak;
}

/**
 * Approximate true peak (linear): the sample peak plus the inter-sample peaks found by 4x Catmull-Rom
 * interpolation. A guard, not a certified meter.
 */
export function truePeak(channels: Channels): number {
  let peak = 0;
  for (const ch of channels) {
    const n = ch.length;
    for (let i = 0; i < n; i++) {
      peak = Math.max(peak, Math.abs(ch[i]!));
      if (i + 1 >= n) continue;
      const p0 = ch[Math.max(0, i - 1)]!;
      const p1 = ch[i]!;
      const p2 = ch[i + 1]!;
      const p3 = ch[Math.min(n - 1, i + 2)]!;
      for (let k = 1; k < 4; k++) {
        const t = k / 4;
        const v =
          0.5 *
          (2 * p1 +
            (-p0 + p2) * t +
            (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t +
            (-p0 + 3 * p1 - 3 * p2 + p3) * t * t * t);
        peak = Math.max(peak, Math.abs(v));
      }
    }
  }
  return peak;
}

/**
 * The linear gain that brings the RMS to `targetDb`, lowered when it would push the true peak over `ceilingDb`.
 * Silence gets gain 1.
 */
export function normalizeGain(
  channels: Channels,
  targetDb: number,
  ceilingDb: number,
): { gain: number; limited: boolean } {
  const rms = rmsDb(channels);
  const peak = truePeak(channels);
  if (!Number.isFinite(rms) || peak === 0) return { gain: 1, limited: false };
  const wanted = toLinear(targetDb - rms);
  const maxGain = toLinear(ceilingDb) / peak;
  return wanted > maxGain ? { gain: maxGain, limited: true } : { gain: wanted, limited: false };
}

/** Trim, normalize (gain measured on the kept part), then fade. */
export function processAudio(src: PcmLike, o: ProcessOptions): Processed {
  const channels = sliceChannels(src, o.startSeconds, o.endSeconds);
  let gain = 1;
  let limited = false;
  if (o.targetDb !== null) {
    const g = normalizeGain(channels, o.targetDb, o.ceilingDb);
    gain = g.gain;
    limited = g.limited;
    for (const ch of channels) for (let i = 0; i < ch.length; i++) ch[i]! *= gain;
  }
  applyFades(channels, src.sampleRate, o.fadeInSeconds, o.fadeOutSeconds);
  return { channels, sampleRate: src.sampleRate, gainDb: toDb(gain), limited };
}

/** 16-bit PCM WAV of the channels (our own writer). `mono` averages the channels. */
export function encodeWav(channels: Channels, sampleRate: number, mono = false): Uint8Array {
  const frames = channels[0]?.length ?? 0;
  const n = mono ? 1 : channels.length;
  const dataBytes = frames * n * 2;
  const out = new Uint8Array(44 + dataBytes);
  const v = new DataView(out.buffer);
  const text = (at: number, s: string) => {
    for (let i = 0; i < s.length; i++) v.setUint8(at + i, s.charCodeAt(i));
  };
  text(0, 'RIFF');
  v.setUint32(4, 36 + dataBytes, true);
  text(8, 'WAVE');
  text(12, 'fmt ');
  v.setUint32(16, 16, true);
  v.setUint16(20, 1, true);
  v.setUint16(22, n, true);
  v.setUint32(24, sampleRate, true);
  v.setUint32(28, sampleRate * n * 2, true);
  v.setUint16(32, n * 2, true);
  v.setUint16(34, 16, true);
  text(36, 'data');
  v.setUint32(40, dataBytes, true);
  const q = (x: number) => {
    const c = Math.max(-1, Math.min(1, x));
    return Math.round(c < 0 ? c * 0x8000 : c * 0x7fff);
  };
  let at = 44;
  for (let i = 0; i < frames; i++) {
    if (mono) {
      let s = 0;
      for (const ch of channels) s += ch[i]!;
      v.setInt16(at, q(s / channels.length), true);
      at += 2;
    } else {
      for (const ch of channels) {
        v.setInt16(at, q(ch[i]!), true);
        at += 2;
      }
    }
  }
  return out;
}

/** The Opus container this browser can record, preferring Ogg (what voice messages need), or null. */
export function opusMimeType(preferOgg = true): string | null {
  if (typeof MediaRecorder === 'undefined' || typeof MediaRecorder.isTypeSupported !== 'function')
    return null;
  const ogg = ['audio/ogg;codecs=opus', 'audio/ogg'];
  const webm = ['audio/webm;codecs=opus', 'audio/webm'];
  for (const t of preferOgg ? [...ogg, ...webm] : [...webm, ...ogg]) {
    if (MediaRecorder.isTypeSupported(t)) return t;
  }
  return null;
}

/**
 * Opus through MediaRecorder: the processed audio is played into a MediaStreamDestination and recorded, so it
 * takes as long as the audio lasts. Rejects when the browser cannot do it.
 */
export async function encodeOpus(
  p: Processed,
  mimeType: string,
  onProgress?: (fraction: number) => void,
): Promise<Blob> {
  const w = globalThis as unknown as {
    AudioContext?: new () => AudioContext;
    webkitAudioContext?: new () => AudioContext;
  };
  const Ctx = w.AudioContext ?? w.webkitAudioContext;
  if (!Ctx) throw new Error('no WebAudio');
  const len = p.channels[0]?.length ?? 0;
  const ctx = new Ctx();
  try {
    const buf = ctx.createBuffer(Math.max(1, p.channels.length), Math.max(1, len), p.sampleRate);
    p.channels.forEach((ch, c) => buf.copyToChannel(ch, c));
    const dest = ctx.createMediaStreamDestination();
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.connect(dest);
    const rec = new MediaRecorder(dest.stream, { mimeType, audioBitsPerSecond: 128_000 });
    const chunks: Blob[] = [];
    rec.ondataavailable = (e) => {
      if (e.data.size > 0) chunks.push(e.data);
    };
    const seconds = len / p.sampleRate;
    const done = new Promise<Blob>((resolve) => {
      rec.onstop = () => resolve(new Blob(chunks, { type: mimeType.split(';')[0] }));
    });
    const tick = setInterval(
      () => onProgress?.(Math.min(1, ctx.currentTime / Math.max(0.001, seconds))),
      200,
    );
    src.onended = () => {
      clearInterval(tick);
      if (rec.state !== 'inactive') rec.stop();
    };
    rec.start();
    src.start();
    return await done;
  } finally {
    void ctx.close?.();
  }
}
