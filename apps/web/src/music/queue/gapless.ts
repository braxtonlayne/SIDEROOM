/**
 * MUS-LIB-001: gapless playback and crossfade on two `<audio>` elements ("decks").
 *
 * One deck plays; the next track is loaded into the other (`preload = auto`) while the first still plays. A
 * short timer watches how much of the playing track is left, measured on the Web Audio clock
 * (`AudioContext.currentTime`), and a little before the end it starts the second deck. The hand-over itself is
 * not a timer's work: the gain of the outgoing deck falls and the gain of the incoming deck rises at times
 * scheduled on the audio clock (`setValueAtTime` for a plain gapless cut, an equal-power `setValueCurveAtTime`
 * for a crossfade), so the cut is sample-accurate even if the timer ran late.
 *
 * Starting an `<audio>` element is not instant. The engine measures how long `play()` takes to become audible
 * (the `playing` event) and starts the next deck that much earlier, so the first sample of the next track
 * lands on the boundary.
 *
 * The module owns no DOM and no `AudioContext`: it is given the clock, the two decks and their gain nodes (each
 * deck's `MediaElementSource` goes through its gain into the EQ input), which is also what the tests fake.
 * Wiring it into the player's graph is the integrator's step (see the handoff).
 */

/** The part of `HTMLAudioElement` the engine uses. */
export interface DeckAudio {
  src: string;
  currentTime: number;
  readonly duration: number;
  readonly paused: boolean;
  preload: string;
  load(): void;
  play(): Promise<void> | void;
  pause(): void;
  addEventListener(type: 'ended' | 'playing', fn: () => void): void;
  removeEventListener(type: 'ended' | 'playing', fn: () => void): void;
}

/** The part of `AudioParam` the engine uses. */
export interface DeckParam {
  cancelScheduledValues(startTime: number): unknown;
  setValueAtTime(value: number, startTime: number): unknown;
  setValueCurveAtTime(values: Float32Array, startTime: number, duration: number): unknown;
}

/** One deck: an element and the gain node after its media source. */
export interface Deck {
  audio: DeckAudio;
  gain: { gain: DeckParam };
}

/** What plays. `durationSec` is a hint used until the element reports the real length. */
export interface GaplessTrack {
  id: string;
  src: string;
  durationSec?: number;
}

export interface GaplessOptions {
  /** The audio clock: `AudioContext` or a fake with `currentTime` in seconds. */
  clock: { readonly currentTime: number };
  decks: readonly [Deck, Deck];
  /** Seconds of crossfade, read at each decision (0 = gapless cut). Capped at 12. */
  crossfadeSec?: () => number;
  /** First guess of how long `play()` takes to become audible. */
  playLatencySec?: number;
  /** How often the timer looks at the clock. */
  tickMs?: number;
  timers?: {
    setInterval(fn: () => void, ms: number): unknown;
    clearInterval(handle: unknown): void;
  };
  /** The next track is now the current one (the hand-over happened). Queue the one after it with `setNext`. */
  onAdvance(track: GaplessTrack): void;
  /** The playing track ended and there was nothing next. */
  onEnded?(): void;
}

export const MAX_CROSSFADE_SEC = 12;
const CURVE_POINTS = 64;

/** Equal-power fade curve: `in` rises 0 to 1, out falls 1 to 0, and in^2 + out^2 = 1 at every point. */
export function fadeCurve(direction: 'in' | 'out'): Float32Array {
  const c = new Float32Array(CURVE_POINTS);
  for (let i = 0; i < CURVE_POINTS; i++) {
    const x = (i / (CURVE_POINTS - 1)) * (Math.PI / 2);
    c[i] = direction === 'in' ? Math.sin(x) : Math.cos(x);
  }
  return c;
}

interface Transition {
  from: 0 | 1;
  to: 0 | 1;
  /** Clock time at which the outgoing track ends. */
  boundary: number;
  /** Clock time at which `play()` was called on the incoming deck. */
  playedAt: number;
}

/** A `play()` call whose `playing` event has not come yet (the latency measurement). */
interface PlayStart {
  deck: 0 | 1;
  at: number;
}

export class GaplessDecks {
  private readonly o: GaplessOptions;
  private readonly tracks: [GaplessTrack | null, GaplessTrack | null] = [null, null];
  private active: 0 | 1 = 0;
  private queued: GaplessTrack | null = null;
  /** Set by `setNext` during a hand-over: it becomes the queued track once the hand-over is done. */
  private after: GaplessTrack | null | undefined = undefined;
  private prefetchedId: string | null = null;
  private transition: Transition | null = null;
  private playStart: PlayStart | null = null;
  private running = false;
  private latency: number;
  private timer: unknown = null;
  private readonly endedFns: [() => void, () => void];
  private readonly playingFns: [() => void, () => void];

  constructor(options: GaplessOptions) {
    this.o = options;
    this.latency = options.playLatencySec ?? 0.05;
    this.endedFns = [() => this.handleEnded(0), () => this.handleEnded(1)];
    this.playingFns = [() => this.handlePlaying(0), () => this.handlePlaying(1)];
    options.decks.forEach((d, i) => {
      d.audio.addEventListener('ended', this.endedFns[i as 0 | 1]);
      d.audio.addEventListener('playing', this.playingFns[i as 0 | 1]);
    });
    const timers = options.timers ?? {
      setInterval: (fn: () => void, ms: number) => setInterval(fn, ms),
      clearInterval: (h: unknown) => clearInterval(h as ReturnType<typeof setInterval>),
    };
    this.timer = timers.setInterval(() => this.tick(), options.tickMs ?? 25);
    this.stopTimer = () => timers.clearInterval(this.timer);
  }

  private stopTimer: () => void;

  private deck(i: 0 | 1): Deck {
    return this.o.decks[i];
  }
  private other(i: 0 | 1): 0 | 1 {
    return i === 0 ? 1 : 0;
  }
  private now(): number {
    return this.o.clock.currentTime;
  }

  /** The current latency estimate (seconds), for tests and diagnostics. */
  get playLatency(): number {
    return this.latency;
  }

  /** The track audible now, or null. */
  get current(): GaplessTrack | null {
    return this.tracks[this.active];
  }

  /** The deck index that is playing the current track. */
  get activeDeck(): 0 | 1 {
    return this.active;
  }

  /** Whether a hand-over is under way (both decks sounding, or about to). */
  get transitioning(): boolean {
    return this.transition !== null;
  }

  /** Play `track` now on the active deck, replacing whatever played. */
  start(track: GaplessTrack, at = 0): void {
    this.abort();
    const d = this.deck(this.active);
    const now = this.now();
    this.silence(this.other(this.active));
    d.gain.gain.cancelScheduledValues(now);
    d.gain.gain.setValueAtTime(1, now);
    d.audio.src = track.src;
    d.audio.currentTime = at;
    this.tracks[this.active] = track;
    this.running = true;
    void d.audio.play();
    // The owner names what follows with setNext once it knows (the old queued track is not this one's next).
    this.queued = null;
    this.after = undefined;
    this.playStart = null;
    this.prefetchedId = null;
  }

  /** Say what follows the current track (null for nothing). It is loaded into the idle deck at once. */
  setNext(track: GaplessTrack | null): void {
    if (this.transition) {
      // Too late for this boundary; it is queued as soon as the hand-over is done.
      this.after = track;
      return;
    }
    if (this.queued?.id !== track?.id) this.prefetchedId = null;
    this.queued = track;
    this.prefetch();
  }

  pause(): void {
    if (this.transition) this.complete(true);
    this.deck(this.active).audio.pause();
    this.running = false;
  }

  resume(): void {
    if (this.tracks[this.active] === null) return;
    this.running = true;
    void this.deck(this.active).audio.play();
  }

  seek(seconds: number): void {
    if (this.transition) this.complete(true);
    this.deck(this.active).audio.currentTime = Math.max(0, seconds);
  }

  /** Stop everything and forget the queued track. */
  stop(): void {
    this.abort();
    for (const i of [0, 1] as const) {
      this.deck(i).audio.pause();
      this.silence(i);
      this.tracks[i] = null;
    }
    this.queued = null;
    this.prefetchedId = null;
    this.running = false;
  }

  destroy(): void {
    this.stopTimer();
    this.o.decks.forEach((d, i) => {
      d.audio.removeEventListener('ended', this.endedFns[i as 0 | 1]);
      d.audio.removeEventListener('playing', this.playingFns[i as 0 | 1]);
    });
  }

  // ----------------------------------------------------------------------------- the timer's work

  /** One look at the clock. Called by the timer; tests call it by hand. */
  tick(): void {
    if (!this.running) return;
    const now = this.now();
    if (this.transition) {
      if (now >= this.transition.boundary) this.complete(false);
      return;
    }
    if (!this.queued || this.prefetchedId !== this.queued.id) return;
    const a = this.deck(this.active).audio;
    const dur = this.lengthOf(this.active);
    if (dur === null) return;
    const remaining = dur - a.currentTime;
    const boundary = now + remaining;
    const fade = this.fadeFor(dur, this.lengthOf(this.other(this.active)));
    if (now >= boundary - fade - this.latency) this.fire(now, boundary, fade);
  }

  private lengthOf(i: 0 | 1): number | null {
    const real = this.deck(i).audio.duration;
    if (Number.isFinite(real) && real > 0) return real;
    const hint = this.tracks[i]?.durationSec;
    return hint && hint > 0 ? hint : null;
  }

  /** The fade to use: the setting, never longer than half of either track. */
  private fadeFor(current: number, next: number | null): number {
    const want = Math.min(MAX_CROSSFADE_SEC, Math.max(0, this.o.crossfadeSec?.() ?? 0));
    if (!Number.isFinite(want) || want <= 0) return 0;
    return Math.min(want, current / 2, next !== null ? next / 2 : want);
  }

  private prefetch(): void {
    if (!this.queued || this.prefetchedId === this.queued.id) return;
    const i = this.other(this.active);
    const d = this.deck(i);
    this.silence(i);
    d.audio.pause();
    d.audio.preload = 'auto';
    d.audio.src = this.queued.src;
    d.audio.currentTime = 0;
    d.audio.load();
    this.tracks[i] = this.queued;
    this.prefetchedId = this.queued.id;
  }

  private silence(i: 0 | 1): void {
    const now = this.now();
    const g = this.deck(i).gain.gain;
    g.cancelScheduledValues(now);
    g.setValueAtTime(0, now);
  }

  /** Start the incoming deck and schedule the hand-over on the audio clock. */
  private fire(now: number, boundary: number, fade: number): void {
    const from = this.active;
    const to = this.other(from);
    const out = this.deck(from).gain.gain;
    const inn = this.deck(to).gain.gain;
    const t0 = Math.max(boundary - fade, now);
    out.cancelScheduledValues(now);
    inn.cancelScheduledValues(now);
    out.setValueAtTime(1, now);
    inn.setValueAtTime(0, now);
    const span = boundary - t0;
    if (fade > 0 && span > 0.001) {
      out.setValueCurveAtTime(fadeCurve('out'), t0, span);
      inn.setValueCurveAtTime(fadeCurve('in'), t0, span);
    } else {
      out.setValueAtTime(0, boundary);
      inn.setValueAtTime(1, boundary);
    }
    const next = this.deck(to).audio;
    next.currentTime = 0;
    void next.play();
    this.transition = { from, to, boundary, playedAt: now };
    this.playStart = { deck: to, at: now };
  }

  /** The hand-over is done (or forced): the incoming deck becomes the active one. */
  private complete(forced: boolean): void {
    const t = this.transition;
    if (!t) return;
    const now = this.now();
    this.transition = null;
    const out = this.deck(t.from);
    const inn = this.deck(t.to);
    out.audio.pause();
    out.gain.gain.cancelScheduledValues(now);
    out.gain.gain.setValueAtTime(0, now);
    if (forced) {
      inn.gain.gain.cancelScheduledValues(now);
      inn.gain.gain.setValueAtTime(1, now);
    }
    this.active = t.to;
    const track = this.tracks[t.to];
    this.tracks[t.from] = null;
    this.queued = this.after ?? null;
    this.after = undefined;
    this.prefetchedId = null;
    if (track) this.o.onAdvance(track);
    this.prefetch();
  }

  /** Drop a scheduled hand-over without sound changes beyond what is needed to stay coherent. */
  private abort(): void {
    if (!this.transition) return;
    const t = this.transition;
    this.transition = null;
    const now = this.now();
    this.deck(t.to).audio.pause();
    this.silence(t.to);
    const out = this.deck(t.from).gain.gain;
    out.cancelScheduledValues(now);
    out.setValueAtTime(1, now);
    this.tracks[t.to] = null;
    this.prefetchedId = null;
  }

  private handlePlaying(i: 0 | 1): void {
    const p = this.playStart;
    if (!p || p.deck !== i) return;
    this.playStart = null;
    // How long play() took to become audible; smooth it so one slow start does not swing the lead.
    const measured = Math.min(0.5, Math.max(0, this.now() - p.at));
    this.latency = this.latency * 0.5 + measured * 0.5;
  }

  private handleEnded(i: 0 | 1): void {
    if (this.transition) {
      if (this.transition.from === i) this.complete(false);
      return;
    }
    if (i !== this.active || !this.running) return;
    if (this.queued && this.prefetchedId === this.queued.id) {
      // The timer missed the boundary (a throttled tab): switch at once, a gap is better than silence.
      const next = this.queued;
      this.deck(this.active).audio.pause();
      this.silence(this.active);
      this.tracks[this.active] = null;
      this.active = this.other(this.active);
      const d = this.deck(this.active);
      d.gain.gain.cancelScheduledValues(this.now());
      d.gain.gain.setValueAtTime(1, this.now());
      d.audio.currentTime = 0;
      void d.audio.play();
      this.queued = null;
      this.prefetchedId = null;
      this.o.onAdvance(next);
      return;
    }
    this.running = false;
    this.o.onEnded?.();
  }
}
