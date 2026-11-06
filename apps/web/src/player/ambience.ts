/**
 * MUS-SKN-002 (D-524): the Pastel Study Deck's ambience mixer, synthesised in the browser. No sound file, no
 * download, no licence: three generators made of noise, filters and slow modulation on their own small
 * `AudioContext`, apart from the player's graph (so the skin's meters and the vinyl follow the music only, and the
 * mixer keeps playing while the music is paused).
 *
 *   rain    = white noise -> band-pass ~1.5 kHz (Q 0.7) -> gain swayed by a 0.1 Hz LFO (depth 0.3)
 *             + white noise -> high shelf (the hiss), quieter
 *   crackle = brown noise bed at -24 dB + 2 to 6 ms noise bursts every 80 to 250 ms through a 3 kHz low-pass
 *   cafe    = brown noise -> low-pass 400 Hz -> gain swayed by a 0.05 Hz LFO (the murmur)
 *             + a soft sine clink (2 to 6 kHz, 60 ms, -30 dB) every 3 to 9 s
 *
 * Each slider is a gain (0..1) reached with a 50 ms ramp. All three at zero suspends the context; any above zero
 * resumes it. The context is created lazily by the caller (on the first non-zero level) and closed by `dispose`.
 * It ships in the Study skin's chunk only.
 */
import { defaultContextFactory, type ContextFactory } from './graph';
import { usePlayerStore } from './store';

export const AMBIENCE_KINDS = ['rain', 'crackle', 'cafe'] as const;
export type AmbienceKind = (typeof AMBIENCE_KINDS)[number];

export interface Ambience {
  /** Set one generator's level, 0..1. */
  set(kind: AmbienceKind, level: number): void;
  /** Stop everything and close the context. */
  dispose(): void;
}

export interface AmbienceTimers {
  setTimeout(fn: () => void, ms: number): unknown;
  clearTimeout(handle: unknown): void;
}

const realTimers: AmbienceTimers = {
  setTimeout: (fn, ms) => globalThis.setTimeout(fn, ms),
  clearTimeout: (h) => globalThis.clearTimeout(h as ReturnType<typeof setTimeout>),
};

/** How loud each generator is at full slider, so the three sit together under the music. */
const KIND_GAIN: Record<AmbienceKind, number> = { rain: 0.5, crackle: 0.9, cafe: 0.7 };
const RAMP_SEC = 0.05;
const dB = (db: number) => 10 ** (db / 20);

function noiseBuffer(ctx: AudioContext, seconds: number, brown: boolean, rng: () => number) {
  const n = Math.max(1, Math.floor(ctx.sampleRate * seconds));
  const buf = ctx.createBuffer(1, n, ctx.sampleRate);
  const data = buf.getChannelData(0);
  let last = 0;
  for (let i = 0; i < n; i++) {
    const white = rng() * 2 - 1;
    if (brown) {
      // Leaky integration of white noise, then scaled back up to about full range.
      last = (last + 0.02 * white) / 1.02;
      data[i] = last * 3.5;
    } else data[i] = white;
  }
  return buf;
}

export function createAmbience(
  makeContext: ContextFactory = defaultContextFactory,
  rng: () => number = Math.random,
  timers: AmbienceTimers = realTimers,
): Ambience | null {
  const ctx = makeContext();
  if (!ctx) return null;
  const white = noiseBuffer(ctx, 2, false, rng);
  const brown = noiseBuffer(ctx, 2, true, rng);
  const burst = noiseBuffer(ctx, 0.006, false, rng);
  const started: AudioScheduledSourceNode[] = [];
  const handles = new Set<unknown>();
  let disposed = false;

  const loop = (buffer: AudioBuffer) => {
    const src = ctx.createBufferSource();
    src.buffer = buffer;
    src.loop = true;
    return src;
  };
  const filter = (type: BiquadFilterType, freq: number, q?: number) => {
    const f = ctx.createBiquadFilter();
    f.type = type;
    f.frequency.value = freq;
    if (q !== undefined) f.Q.value = q;
    return f;
  };
  const gain = (v: number) => {
    const g = ctx.createGain();
    g.gain.value = v;
    return g;
  };
  /** A slow sine on `param`, around its value, `depth` either way. */
  const sway = (param: AudioParam, hz: number, depth: number) => {
    const lfo = ctx.createOscillator();
    lfo.frequency.value = hz;
    const amount = gain(depth);
    lfo.connect(amount);
    amount.connect(param);
    lfo.start();
    started.push(lfo);
  };
  const start = (...nodes: AudioScheduledSourceNode[]) => {
    for (const n of nodes) {
      n.start();
      started.push(n);
    }
  };

  // One output gain per generator (the slider), all into the destination.
  const out = {} as Record<AmbienceKind, GainNode>;
  for (const kind of AMBIENCE_KINDS) {
    out[kind] = gain(0);
    out[kind].connect(ctx.destination);
  }
  const levels: Record<AmbienceKind, number> = { rain: 0, crackle: 0, cafe: 0 };

  // Rain: a band of noise that swells and ebbs, with a little hiss on top.
  {
    const body = loop(white);
    const band = filter('bandpass', 1500, 0.7);
    const swell = gain(0.7);
    sway(swell.gain, 0.1, 0.3);
    body.connect(band).connect(swell).connect(out.rain);
    const hissSrc = loop(white);
    const hiss = filter('highshelf', 5000);
    hiss.gain.value = 6;
    const hissLevel = gain(0.12);
    hissSrc.connect(hiss).connect(hissLevel).connect(out.rain);
    start(body, hissSrc);
  }

  // Crackle: a quiet brown bed and sparse bursts, scheduled while the slider is up.
  const crackleIn = filter('lowpass', 3000);
  crackleIn.connect(out.crackle);
  {
    const bed = loop(brown);
    const bedLevel = gain(dB(-24));
    bed.connect(bedLevel).connect(out.crackle);
    start(bed);
  }
  const pop = () => {
    const src = ctx.createBufferSource();
    src.buffer = burst;
    const level = gain(0.3 + rng() * 0.7);
    src.connect(level).connect(crackleIn);
    src.start(ctx.currentTime, 0, 0.002 + rng() * 0.004);
  };

  // Café: a low murmur that drifts, and now and then a cup.
  {
    const room = loop(brown);
    const low = filter('lowpass', 400);
    const murmur = gain(0.7);
    sway(murmur.gain, 0.05, 0.3);
    room.connect(low).connect(murmur).connect(out.cafe);
    start(room);
  }
  const clink = () => {
    const osc = ctx.createOscillator();
    osc.type = 'sine';
    osc.frequency.value = 2000 + rng() * 4000;
    const env = gain(0);
    const t = ctx.currentTime;
    env.gain.setValueAtTime(0, t);
    env.gain.linearRampToValueAtTime(dB(-30), t + 0.004);
    env.gain.exponentialRampToValueAtTime(0.0001, t + 0.06);
    osc.connect(env).connect(out.cafe);
    osc.start(t);
    osc.stop(t + 0.07);
  };

  /** Fire `fn` again and again at random gaps while `kind` is up; stops by itself when it goes to zero. */
  const every = (kind: AmbienceKind, minMs: number, maxMs: number, fn: () => void) => {
    const tick = () => {
      if (disposed || levels[kind] <= 0) return;
      fn();
      schedule();
    };
    const schedule = () => {
      const h = timers.setTimeout(
        () => {
          handles.delete(h);
          tick();
        },
        minMs + rng() * (maxMs - minMs),
      );
      handles.add(h);
    };
    schedule();
  };

  return {
    set(kind, level) {
      if (disposed) return;
      const v = Math.min(1, Math.max(0, Number.isFinite(level) ? level : 0));
      const was = levels[kind];
      levels[kind] = v;
      const p = out[kind].gain;
      const t = ctx.currentTime;
      p.cancelScheduledValues(t);
      p.setValueAtTime(p.value, t);
      p.linearRampToValueAtTime(v * KIND_GAIN[kind], t + RAMP_SEC);
      if (was <= 0 && v > 0) {
        if (kind === 'crackle') every('crackle', 80, 250, pop);
        if (kind === 'cafe') every('cafe', 3000, 9000, clink);
      }
      const any = AMBIENCE_KINDS.some((k) => levels[k] > 0);
      void (any ? ctx.resume() : ctx.suspend()).catch(() => undefined);
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      for (const h of handles) timers.clearTimeout(h);
      handles.clear();
      for (const n of started) {
        try {
          n.stop();
        } catch {
          /* never started */
        }
      }
      void ctx.close().catch(() => undefined);
    },
  };
}

// ---- The mixer's state for the tab (the Study skin's sliders read and set it) ----

export type AmbienceLevels = Readonly<Record<AmbienceKind, number>>;
const SILENT: AmbienceLevels = { rain: 0, crackle: 0, cafe: 0 };

let levels: AmbienceLevels = SILENT;
let live: Ambience | null = null;
let unwatch: (() => void) | null = null;
let factory: () => Ambience | null = () => createAmbience();
const listeners = new Set<() => void>();
const emit = () => {
  for (const fn of listeners) fn();
};

export const ambienceLevels = (): AmbienceLevels => levels;
export function subscribeAmbience(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/**
 * Move one slider. The first level above zero creates the mixer (a browser gesture, so the context may start);
 * the levels survive a skin change in the tab. Closing the player (its queue emptied) stops and forgets it.
 */
export function setAmbienceLevel(kind: AmbienceKind, level: number): void {
  const v = Math.min(1, Math.max(0, Number.isFinite(level) ? level : 0));
  if (levels[kind] === v) return;
  levels = { ...levels, [kind]: v };
  emit();
  if (!live && v > 0) {
    live = factory();
    unwatch ??= usePlayerStore.subscribe((s) => {
      if (s.queue.length === 0) stopAmbience();
    });
    if (live) for (const k of AMBIENCE_KINDS) if (levels[k] > 0) live.set(k, levels[k]);
    return;
  }
  live?.set(kind, v);
}

/** Close the mixer and put every slider back to zero (the player closed). */
export function stopAmbience(): void {
  live?.dispose();
  live = null;
  unwatch?.();
  unwatch = null;
  if (levels !== SILENT) {
    levels = SILENT;
    emit();
  }
}

/** The skin is going away: with every slider at zero there is nothing to keep, so the context is closed. */
export function releaseAmbienceIfSilent(): void {
  if (AMBIENCE_KINDS.every((k) => levels[k] === 0)) stopAmbience();
}

/** Test hook: the mixer factory, and back to silence. */
export function __resetAmbience(make?: () => Ambience | null): void {
  stopAmbience();
  factory = make ?? (() => createAmbience());
}
