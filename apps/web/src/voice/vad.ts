/**
 * Voice activity detection: a WebAudio energy gate (VOX-CLI-002, LNG-USET-044).
 *
 * Pure and clock-injected so it is unit-testable. Levels are dBFS (0 = full scale, about -100 = silence).
 * Automatic sensitivity tracks the noise floor and opens `AUTO_MARGIN_DB` above it; manual mode opens at
 * the user's threshold. A hang time keeps the gate open between words.
 */
export const SILENCE_DB = -100;
export const THRESHOLD_RANGE = [-100, 0] as const;
export const DEFAULT_THRESHOLD_DB = -50;
export const AUTO_MARGIN_DB = 12;
export const HANG_MS = 250;
/** Sound that stays above the margin this long is background noise, not speech: the floor adapts to it. */
export const NOISE_ADAPT_MS = 10_000;
export const AUTO_FLOOR_MIN_DB = -75;
export const AUTO_FLOOR_MAX_DB = -35;

/** RMS of float samples as dBFS. */
export function rmsDb(samples: ArrayLike<number>): number {
  const n = samples.length;
  if (n === 0) return SILENCE_DB;
  let sum = 0;
  for (let i = 0; i < n; i++) {
    const v = samples[i]!;
    sum += v * v;
  }
  const rms = Math.sqrt(sum / n);
  return rms <= 1e-5 ? SILENCE_DB : Math.max(SILENCE_DB, 20 * Math.log10(rms));
}

/** Map dBFS to 0..1 for meters (anything under -90 reads as empty). */
export function levelFraction(db: number): number {
  return Math.min(1, Math.max(0, (db + 90) / 90));
}

export interface VadOptions {
  auto: boolean;
  thresholdDb: number;
}

const clampFloor = (v: number) => Math.min(AUTO_FLOOR_MAX_DB, Math.max(AUTO_FLOOR_MIN_DB, v));

export class VadGate {
  private open_ = false;
  private lastLoudAt = -Infinity;
  private floor = -60;
  private seeded = false;
  private loudSince = -1;
  private opts: VadOptions;

  constructor(opts: VadOptions) {
    this.opts = opts;
  }

  configure(opts: VadOptions): void {
    this.opts = opts;
  }

  get isOpen(): boolean {
    return this.open_;
  }

  /** The level the gate currently opens at. */
  get openAtDb(): number {
    return this.opts.auto ? this.floor + AUTO_MARGIN_DB : this.opts.thresholdDb;
  }

  /** Feed one level reading; returns whether the gate is open afterwards. */
  process(db: number, nowMs: number): boolean {
    if (this.opts.auto) this.trackFloor(db, nowMs);
    if (db >= this.openAtDb) this.lastLoudAt = nowMs;
    this.open_ = nowMs - this.lastLoudAt <= HANG_MS;
    return this.open_;
  }

  private trackFloor(db: number, nowMs: number): void {
    if (!this.seeded) {
      this.floor = clampFloor(db);
      this.seeded = true;
      return;
    }
    // Speech (well above the floor) leaves the estimate alone, unless it never stops: then it is the room.
    if (db >= this.floor + AUTO_MARGIN_DB) {
      if (this.loudSince < 0) this.loudSince = nowMs;
      if (nowMs - this.loudSince < NOISE_ADAPT_MS) return;
    } else {
      this.loudSince = -1;
    }
    // Follow quickly down and slowly up, so the floor settles on the background level.
    const k = db < this.floor ? 0.2 : 0.01;
    this.floor = clampFloor(this.floor + (db - this.floor) * k);
  }

  reset(): void {
    this.open_ = false;
    this.lastLoudAt = -Infinity;
    this.seeded = false;
    this.loudSince = -1;
  }
}
