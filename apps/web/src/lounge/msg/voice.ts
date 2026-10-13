/**
 * Voice message helpers (LNG-CMP-012). The server accepts exactly one `audio/ogg` file of at most two minutes
 * with a waveform of up to 256 amplitude bytes (rich_create.rs). Browsers differ in what MediaRecorder can
 * produce: Firefox records Ogg Opus, Chrome records WebM and Safari records MP4. Only a browser that can record
 * Ogg gets the recorder; the others do not see the button rather than uploading a file the server refuses.
 */

export const MAX_VOICE_SECS = 120;
export const WAVEFORM_BINS = 64;
/** Message flag for voice messages (1 << 13). */
export const VOICE_FLAG = 8192;

/** The Ogg type this browser can record, or null when it cannot record Ogg. */
export function voiceMimeType(): string | null {
  if (typeof MediaRecorder === 'undefined' || typeof MediaRecorder.isTypeSupported !== 'function')
    return null;
  for (const t of ['audio/ogg;codecs=opus', 'audio/ogg']) {
    if (MediaRecorder.isTypeSupported(t)) return t;
  }
  return null;
}

/** Peak amplitude per bin of mono samples, scaled to bytes (0 to 255, at least 1 where there is sound). */
export function computeWaveform(samples: Float32Array, bins = WAVEFORM_BINS): Uint8Array {
  const out = new Uint8Array(bins);
  if (samples.length === 0) return out;
  const per = samples.length / bins;
  let max = 0;
  const peaks = new Float32Array(bins);
  for (let b = 0; b < bins; b++) {
    const from = Math.floor(b * per);
    const to = Math.max(from + 1, Math.floor((b + 1) * per));
    let peak = 0;
    for (let i = from; i < to && i < samples.length; i++)
      peak = Math.max(peak, Math.abs(samples[i]!));
    peaks[b] = peak;
    max = Math.max(max, peak);
  }
  // Normalised to the loudest bin so quiet recordings still show a shape.
  for (let b = 0; b < bins; b++) {
    out[b] = max > 0 ? Math.max(peaks[b]! > 0 ? 1 : 0, Math.round((peaks[b]! / max) * 255)) : 0;
  }
  return out;
}

export function toBase64(bytes: Uint8Array): string {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

/** Decodes a recording and returns its base64 waveform; empty string when decoding is not possible. */
export async function waveformOf(blob: Blob): Promise<string> {
  const Ctx =
    typeof AudioContext !== 'undefined'
      ? AudioContext
      : (globalThis as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!Ctx) return '';
  const ctx = new Ctx();
  try {
    const audio = await ctx.decodeAudioData(await blob.arrayBuffer());
    return toBase64(computeWaveform(audio.getChannelData(0)));
  } catch {
    return '';
  } finally {
    void ctx.close();
  }
}

/** A flat waveform for when decoding fails: the server requires one. */
export const FLAT_WAVEFORM = toBase64(new Uint8Array(WAVEFORM_BINS).fill(96));
