// BYOS-CNV-002: sample-rate and channel conversion for the audio path. Pure
// functions on planar float samples, so they are unit-tested without WebCodecs.

/** Downmix to `dst` channels (1 or 2). 5.1 and 7.1 follow the ITU weights, normalised so they cannot clip. */
export function downmix(planes: Float32Array[], dst: 1 | 2): Float32Array[] {
  const n = planes[0]?.length ?? 0;
  const src = planes.length;
  if (src === dst) return planes;
  const mono = (f: (i: number) => number) => {
    const out = new Float32Array(n);
    for (let i = 0; i < n; i++) out[i] = f(i);
    return out;
  };
  let left: Float32Array;
  let right: Float32Array;
  if (src === 1) {
    left = planes[0]!;
    right = planes[0]!;
  } else if (src >= 6) {
    // L R C LFE Ls Rs [Lb Rb]; LFE dropped
    const [l, r, c, , ls, rs] = planes as [
      Float32Array,
      Float32Array,
      Float32Array,
      Float32Array,
      Float32Array,
      Float32Array,
    ];
    const k = 1 / (1 + Math.SQRT1_2 * 2);
    left = mono((i) => (l[i]! + Math.SQRT1_2 * c[i]! + Math.SQRT1_2 * ls[i]!) * k);
    right = mono((i) => (r[i]! + Math.SQRT1_2 * c[i]! + Math.SQRT1_2 * rs[i]!) * k);
  } else {
    left = planes[0]!;
    right = planes[1]!;
  }
  if (dst === 2) return [left, right];
  return [mono((i) => (left[i]! + right[i]!) / 2)];
}

/** Linear-interpolation resampler. Good enough for a playable fallback; not a mastering tool. */
export function resample(plane: Float32Array, srcRate: number, dstRate: number): Float32Array {
  if (srcRate === dstRate) return plane;
  const outLen = Math.max(1, Math.round((plane.length * dstRate) / srcRate));
  const out = new Float32Array(outLen);
  const ratio = srcRate / dstRate;
  const last = plane.length - 1;
  for (let i = 0; i < outLen; i++) {
    const pos = i * ratio;
    const i0 = Math.min(last, Math.floor(pos));
    const i1 = Math.min(last, i0 + 1);
    const frac = pos - i0;
    out[i] = plane[i0]! * (1 - frac) + plane[i1]! * frac;
  }
  return out;
}

/** Converts decoded audio planes to the encoder's rate and channel count. */
export function convertPlanes(
  planes: Float32Array[],
  srcRate: number,
  dstRate: number,
  dstChannels: 1 | 2,
): Float32Array[] {
  return downmix(planes, dstChannels).map((p) => resample(p, srcRate, dstRate));
}

/** Reads an AudioData into planar f32, whatever its own sample format. */
export function readPlanes(data: AudioData): Float32Array[] {
  const planes: Float32Array[] = [];
  for (let c = 0; c < data.numberOfChannels; c++) {
    const p = new Float32Array(data.numberOfFrames);
    data.copyTo(p, { planeIndex: c, format: 'f32-planar' });
    planes.push(p);
  }
  return planes;
}

/** Builds an AudioData from planar f32 (the planes laid end to end, as f32-planar wants). */
export function planesToAudioData(
  AudioDataCtor: typeof AudioData,
  planes: Float32Array[],
  sampleRate: number,
  timestamp: number,
): AudioData {
  const frames = planes[0]!.length;
  const flat = new Float32Array(frames * planes.length);
  planes.forEach((p, i) => flat.set(p, i * frames));
  return new AudioDataCtor({
    format: 'f32-planar',
    sampleRate,
    numberOfFrames: frames,
    numberOfChannels: planes.length,
    timestamp,
    data: flat,
  });
}
