/**
 * MUS-LIB-001: the ten EQ bands and the presets. The band centres are the ISO octave centres the player's
 * graph uses (`player/types.ts` `EQ_BANDS`); the first band is a low shelf and the last a high shelf.
 */

export const EQ_FREQS = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000] as const;
export const EQ_BAND_COUNT = EQ_FREQS.length;
/** Largest boost or cut of one band, in dB. */
export const EQ_MAX_DB = 12;

/** The ids of the presets; the label of each is the catalog key `music.eq.preset.<id>`. */
export const EQ_PRESET_IDS = [
  'flat',
  'bassBoost',
  'trebleBoost',
  'vocal',
  'rock',
  'pop',
  'jazz',
  'classical',
  'electronic',
  'loudness',
  'acoustic',
  'smallSpeakers',
] as const;

export type EqPresetId = (typeof EQ_PRESET_IDS)[number];

/** dB per band, 31 Hz to 16 kHz. */
export const EQ_PRESETS: Readonly<Record<EqPresetId, readonly number[]>> = {
  flat: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  bassBoost: [7, 6, 5, 3, 1, 0, 0, 0, 0, 0],
  trebleBoost: [0, 0, 0, 0, 0, 1, 3, 5, 6, 7],
  vocal: [-3, -3, -2, 1, 4, 4, 3, 1, 0, -1],
  rock: [5, 4, 2, -1, -2, -1, 2, 4, 5, 5],
  pop: [-1, 1, 3, 4, 3, 0, -1, -1, 1, 2],
  jazz: [3, 2, 1, 2, -2, -2, 0, 1, 2, 3],
  classical: [4, 3, 2, 1, -1, -1, 0, 2, 3, 4],
  electronic: [6, 5, 1, 0, -2, 2, 1, 2, 5, 6],
  loudness: [6, 4, 0, 0, -1, 0, -1, 0, 4, 2],
  acoustic: [4, 4, 3, 1, 2, 2, 3, 3, 3, 2],
  smallSpeakers: [6, 5, 4, 3, 2, 0, -1, -2, -2, -3],
};

/** One finite number clamped to the band range; anything else is 0 dB. */
export function clampDb(v: unknown): number {
  const n = typeof v === 'number' && Number.isFinite(v) ? v : 0;
  return Math.min(EQ_MAX_DB, Math.max(-EQ_MAX_DB, n));
}

/** Exactly ten clamped gains; missing bands are flat, extras are ignored. */
export function normalizeGains(gains: readonly unknown[] | null | undefined): number[] {
  return EQ_FREQS.map((_, i) => clampDb(gains?.[i]));
}

/** The preset a set of gains equals (within 0.05 dB), or `custom`. */
export function matchPreset(gains: readonly number[]): EqPresetId | 'custom' {
  const g = normalizeGains(gains);
  for (const id of EQ_PRESET_IDS) {
    if (EQ_PRESETS[id].every((v, i) => Math.abs(v - g[i]!) < 0.05)) return id;
  }
  return 'custom';
}

/**
 * Headroom: boosting bands raises the peak level and can clip, so the chain cuts the whole signal by the
 * largest boost (0 dB when nothing is boosted). Returns the preamp gain in dB (zero or negative).
 */
export function preampDb(gains: readonly number[]): number {
  const top = Math.max(0, ...normalizeGains(gains));
  return -top;
}

/** dB to a linear gain factor. */
export const dbToGain = (db: number): number => 10 ** (db / 20);
