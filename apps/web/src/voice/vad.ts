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

// ---- D-1048, D-1050, D-1051: the second engine (the Silero detector), seen from outside ----------------------------
// Everything below is small and holds no detector code: the detector arrives through `load()` (a dynamic import), and
// only after a member chose it. With engine 'energy' none of it ever runs.

export type VadDetectorState = 'idle' | 'loading' | 'ready' | 'fallback';

export interface VadDetector {
  readonly gate: {
    readonly isOpen: boolean;
    process(prob: number, nowMs: number): boolean;
    reset(): void;
  };
  readonly state: VadDetectorState;
  readonly reason: string | null;
  readonly frames: number;
  readonly speechFrames: number;
  readonly meanMs: number;
  readonly maxMs: number;
  start(ctx: AudioContext, source: AudioNode): void;
  stop(): void;
  reset(): void;
  onProb(cb: (prob: number) => void): () => void;
  onState(cb: (s: VadDetectorState, reason: string | null) => void): () => void;
}

/** Who holds the microphone: the call wins over the settings test. */
export type VadTapRole = 'call' | 'test';

export interface VadEngineStats {
  engine: 'energy' | 'silero';
  state: VadDetectorState;
  frames: number;
  speechFrames: number;
  meanMs: number;
  maxMs: number;
  fallbackReason: string | null;
  /** Duration of one frame, milliseconds. */
  frameMs: number;
}

interface VadTap {
  owner: object;
  role: VadTapRole;
  ctx: AudioContext;
  source: AudioNode;
}

export interface VadEngineHostOptions {
  /** Whether the member chose the Silero engine and voice activity is the input mode. */
  wanted(): boolean;
  /** Loads the detector code (a dynamic import); rejects when the chunk cannot be fetched. */
  load(): Promise<VadDetector>;
  now(): number;
  /** Which engine the setting names (for the test hook's figures). */
  engine(): 'energy' | 'silero';
}

/**
 * Starts and stops the detector on the microphone's tap and says whether it drives the gate. While it is loading,
 * after a fallback, or when the engine is Standard, `drives` is false and the energy gate decides, so the
 * microphone is never dead. A fallback holds for the rest of the call (the tap's owner), and the next call tries again.
 */
export class VadEngineHost {
  private tap: VadTap | null = null;
  private det: VadDetector | null = null;
  private running: VadTap | null = null;
  private offs: (() => void)[] = [];
  private seq = 0;
  private blocked: object | null = null;
  private fell: string | null = null;
  private state_: VadDetectorState = 'idle';
  private open_ = false;
  private listeners = new Set<() => void>();

  constructor(private readonly o: VadEngineHostOptions) {}

  get state(): VadDetectorState {
    return this.state_;
  }
  get reason(): string | null {
    return this.state_ === 'fallback' ? this.fell : null;
  }
  /** The role of the microphone the detector listens to now. */
  get role(): VadTapRole | null {
    return this.running?.role ?? this.tap?.role ?? null;
  }
  /** True when the detector, not the energy gate, decides whether the member is speaking. */
  get drives(): boolean {
    return this.state_ === 'ready' && this.running !== null;
  }
  get open(): boolean {
    return this.open_;
  }

  subscribe(cb: () => void): () => void {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  }

  snapshot(): VadEngineStats {
    const d = this.det;
    return {
      engine: this.o.engine(),
      state: this.state_,
      frames: d?.frames ?? 0,
      speechFrames: d?.speechFrames ?? 0,
      meanMs: d?.meanMs ?? 0,
      maxMs: d?.maxMs ?? 0,
      fallbackReason: this.reason,
      frameMs: 32,
    };
  }

  /** A microphone graph was built: its context and the node that carries the unprocessed microphone. */
  setTap(owner: object, role: VadTapRole, ctx: AudioContext, source: AudioNode): void {
    if (this.tap?.role === 'call' && role !== 'call' && this.tap.owner !== owner) return;
    this.tap = { owner, role, ctx, source };
    this.refresh();
  }

  /** That microphone graph is gone. */
  clearTap(owner: object): void {
    if (this.tap?.owner !== owner) return;
    this.tap = null;
    this.refresh();
  }

  /** A new call (or a rejoin) tries the detector again after a fallback. */
  resetBlock(): void {
    this.blocked = null;
    this.refresh();
  }

  /** Start, stop or keep the detector to match the setting and the tap. Cheap when nothing changed. */
  refresh(): void {
    const tap = this.tap;
    if (!this.o.wanted()) {
      this.blocked = null;
      this.halt();
      this.det = null;
      return;
    }
    if (!tap) {
      // A fallback stays on show for the rest of the call, even while its microphone graph is rebuilt.
      if (!this.blocked) this.halt();
      return;
    }
    if (this.blocked === tap.owner) return;
    if (this.running && this.running.ctx === tap.ctx && this.running.source === tap.source) return;
    this.begin(tap);
  }

  private begin(tap: VadTap): void {
    this.halt(false);
    const my = ++this.seq;
    this.running = tap;
    this.det = null;
    this.fell = null;
    this.set('loading');
    this.o.load().then(
      (det) => {
        if (my !== this.seq) return;
        this.det = det;
        this.offs.push(
          det.onProb((p) => {
            const open = det.gate.process(p, this.o.now());
            if (open !== this.open_) {
              this.open_ = open;
              this.emit();
            }
          }),
          det.onState((s, reason) => {
            if (my !== this.seq) return;
            if (s === 'fallback') this.fall(tap, reason ?? 'unknown');
            else this.set(s);
          }),
        );
        det.start(tap.ctx, tap.source);
      },
      () => {
        if (my === this.seq) this.fall(tap, 'chunk');
      },
    );
  }

  private fall(tap: VadTap, reason: string): void {
    this.blocked = tap.owner;
    this.fell = reason;
    this.stopDetector();
    this.running = null;
    this.open_ = false;
    this.state_ = 'fallback';
    this.emit();
  }

  private halt(emit = true): void {
    this.seq++;
    this.stopDetector();
    this.running = null;
    this.open_ = false;
    this.fell = null;
    if (emit) this.set('idle');
    else this.state_ = 'idle';
  }

  private stopDetector(): void {
    for (const off of this.offs) off();
    this.offs = [];
    try {
      this.det?.stop();
    } catch {
      /* already stopped */
    }
  }

  private set(state: VadDetectorState): void {
    if (state === this.state_) return;
    this.state_ = state;
    this.emit();
  }

  private emit(): void {
    for (const cb of [...this.listeners]) cb();
  }
}
