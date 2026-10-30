import { beforeEach, describe, expect, it } from 'vitest';
import {
  GaplessDecks,
  fadeCurve,
  type Deck,
  type DeckAudio,
  type DeckParam,
  type GaplessTrack,
} from './gapless';

/** A fake audio clock the test advances by hand; playing fake elements advance with it. */
class Clock {
  currentTime = 0;
  readonly audios: FakeAudio[] = [];
  advance(sec: number, tick: () => void, step = 0.005): void {
    const steps = Math.round(sec / step);
    for (let i = 0; i < steps; i++) {
      this.currentTime = Math.round((this.currentTime + step) * 1e6) / 1e6;
      for (const a of this.audios) a.step(step);
      tick();
    }
  }
}

type Listener = () => void;

class FakeAudio implements DeckAudio {
  src = '';
  currentTime = 0;
  duration = Number.NaN;
  paused = true;
  preload = '';
  loads = 0;
  /** Clock times at which play() was called. */
  playCalls: number[] = [];
  /** How long after play() the 'playing' event fires (the real start latency of the fake). */
  startDelay = 0.04;
  private startedAt: number | null = null;
  private heard = false;
  private ls = new Map<string, Set<Listener>>();
  constructor(
    private readonly clock: Clock,
    private readonly lengths: Record<string, number>,
  ) {
    clock.audios.push(this);
  }
  load(): void {
    this.loads++;
    this.duration = this.lengths[this.src] ?? Number.NaN;
  }
  play(): void {
    this.paused = false;
    this.playCalls.push(this.clock.currentTime);
    this.startedAt = this.clock.currentTime;
    this.heard = false;
    if (!Number.isFinite(this.duration)) this.duration = this.lengths[this.src] ?? Number.NaN;
  }
  pause(): void {
    this.paused = true;
  }
  addEventListener(type: string, fn: Listener): void {
    if (!this.ls.has(type)) this.ls.set(type, new Set());
    this.ls.get(type)!.add(fn);
  }
  removeEventListener(type: string, fn: Listener): void {
    this.ls.get(type)?.delete(fn);
  }
  emit(type: string): void {
    for (const f of this.ls.get(type) ?? []) f();
  }
  step(dt: number): void {
    if (this.paused || this.startedAt === null) return;
    if (!this.heard) {
      if (this.clock.currentTime - this.startedAt + 1e-9 < this.startDelay) return;
      this.heard = true;
      this.emit('playing');
    }
    this.currentTime += dt;
    if (Number.isFinite(this.duration) && this.currentTime >= this.duration) {
      this.currentTime = this.duration;
      this.paused = true;
      this.emit('ended');
    }
  }
}

interface Event {
  kind: 'set' | 'curve' | 'cancel';
  value?: number;
  at: number;
  dur?: number;
  curve?: Float32Array;
}

class FakeParam implements DeckParam {
  events: Event[] = [];
  cancelScheduledValues(at: number): void {
    this.events.push({ kind: 'cancel', at });
  }
  setValueAtTime(value: number, at: number): void {
    this.events.push({ kind: 'set', value, at });
  }
  setValueCurveAtTime(curve: Float32Array, at: number, dur: number): void {
    this.events.push({ kind: 'curve', at, dur, curve });
  }
  /** The last scheduled non-cancel events, for assertions. */
  get scheduled(): Event[] {
    return this.events.filter((e) => e.kind !== 'cancel');
  }
}

const A: GaplessTrack = { id: 'a', src: 'a.mp3' };
const B: GaplessTrack = { id: 'b', src: 'b.mp3' };
const C: GaplessTrack = { id: 'c', src: 'c.mp3' };

interface Rig {
  clock: Clock;
  decks: [FakeAudio, FakeAudio];
  gains: [FakeParam, FakeParam];
  engine: GaplessDecks;
  advanced: string[];
  ended: number;
  fade: { sec: number };
}

function rig(lengths: Record<string, number> = { 'a.mp3': 10, 'b.mp3': 8, 'c.mp3': 6 }): Rig {
  const clock = new Clock();
  const decks: [FakeAudio, FakeAudio] = [
    new FakeAudio(clock, lengths),
    new FakeAudio(clock, lengths),
  ];
  const gains: [FakeParam, FakeParam] = [new FakeParam(), new FakeParam()];
  const fade = { sec: 0 };
  const r = {
    clock,
    decks,
    gains,
    advanced: [] as string[],
    ended: 0,
    fade,
  } as Rig;
  const wire = (i: 0 | 1): Deck => ({ audio: decks[i], gain: { gain: gains[i] } });
  r.engine = new GaplessDecks({
    clock,
    decks: [wire(0), wire(1)],
    crossfadeSec: () => fade.sec,
    playLatencySec: 0.05,
    timers: { setInterval: () => 0, clearInterval: () => undefined },
    onAdvance: (t) => r.advanced.push(t.id),
    onEnded: () => r.ended++,
  });
  decks[0].startDelay = 0;
  return r;
}

const run = (r: Rig, sec: number) => r.clock.advance(sec, () => r.engine.tick());
/** The value a deck's gain holds at clock time `t` according to its scheduled `set` events. */
const gainAt = (p: FakeParam, t: number): number => {
  let v = 0;
  for (const e of p.scheduled) if (e.kind === 'set' && e.at <= t + 1e-9) v = e.value!;
  return v;
};

describe('gapless hand-over on the audio clock', () => {
  let r: Rig;
  beforeEach(() => {
    r = rig();
  });

  it('prefetches the next track into the idle deck without playing it', () => {
    r.engine.start(A);
    r.engine.setNext(B);
    expect(r.decks[1].src).toBe('b.mp3');
    expect(r.decks[1].preload).toBe('auto');
    expect(r.decks[1].loads).toBe(1);
    expect(r.decks[1].playCalls).toEqual([]);
    expect(r.gains[1].scheduled.at(-1)).toMatchObject({ kind: 'set', value: 0 });
    // Asking for the same track again does not load it twice.
    r.engine.setNext(B);
    expect(r.decks[1].loads).toBe(1);
  });

  it('starts the next deck one play-latency early and cuts on the boundary by the clock', () => {
    r.engine.start(A);
    r.engine.setNext(B);
    run(r, 9.97);
    expect(r.decks[1].playCalls).toHaveLength(1);
    // Boundary is 10 s on the clock; latency estimate 0.05: play() within one tick of 9.95.
    expect(r.decks[1].playCalls[0]).toBeGreaterThanOrEqual(9.95 - 0.006);
    expect(r.decks[1].playCalls[0]).toBeLessThanOrEqual(9.95 + 0.006);
    // The cut is scheduled, not timed: out falls to 0 and in rises to 1 exactly at 10.
    expect(r.gains[0].scheduled.at(-1)).toMatchObject({ kind: 'set', value: 0 });
    expect(r.gains[0].scheduled.at(-1)!.at).toBeCloseTo(10, 3);
    expect(r.gains[1].scheduled.at(-1)).toMatchObject({ kind: 'set', value: 1 });
    expect(r.gains[1].scheduled.at(-1)!.at).toBeCloseTo(10, 3);
    expect(gainAt(r.gains[0], 9.99)).toBe(1);
    expect(gainAt(r.gains[1], 9.99)).toBe(0);
    expect(r.advanced).toEqual([]);

    run(r, 0.2);
    expect(r.advanced).toEqual(['b']);
    expect(r.decks[0].paused).toBe(true);
    expect(r.engine.current?.id).toBe('b');
    expect(r.engine.activeDeck).toBe(1);
    expect(r.ended).toBe(0);
  });

  it('chains tracks: the owner queues the following track on advance and decks swap roles', () => {
    const order = [B, C];
    r.engine.start(A);
    r.engine.setNext(order[0]!);
    run(r, 10.2);
    expect(r.advanced).toEqual(['b']);
    r.engine.setNext(order[1]!);
    expect(r.decks[0].src).toBe('c.mp3');
    run(r, 8.2);
    expect(r.advanced).toEqual(['b', 'c']);
    expect(r.engine.activeDeck).toBe(0);
    run(r, 6.5);
    // Nothing queued after c: it ends, once.
    expect(r.ended).toBe(1);
    expect(r.advanced).toEqual(['b', 'c']);
  });

  it('crossfades with equal-power curves over the setting, starting early', () => {
    r.fade.sec = 3;
    r.engine.start(A);
    r.engine.setNext(B);
    run(r, 7.1);
    // Fire at boundary 10 - fade 3 - latency 0.05 = 6.95.
    expect(r.decks[1].playCalls).toHaveLength(1);
    expect(r.decks[1].playCalls[0]).toBeCloseTo(6.95, 1);
    const out = r.gains[0].scheduled.find((e) => e.kind === 'curve')!;
    const inn = r.gains[1].scheduled.find((e) => e.kind === 'curve')!;
    expect(out.at).toBeCloseTo(7, 1);
    expect(out.dur).toBeCloseTo(3, 1);
    expect(inn.at).toBeCloseTo(out.at, 6);
    expect(inn.dur).toBeCloseTo(out.dur!, 6);
    expect(out.curve![0]).toBeCloseTo(1, 5);
    expect(out.curve!.at(-1)).toBeCloseTo(0, 5);
    expect(inn.curve![0]).toBeCloseTo(0, 5);
    expect(inn.curve!.at(-1)).toBeCloseTo(1, 5);
    for (let i = 0; i < out.curve!.length; i++) {
      expect(out.curve![i]! ** 2 + inn.curve![i]! ** 2).toBeCloseTo(1, 5);
    }
    // Hand-over is announced when the outgoing track ends, not at the start of the fade.
    expect(r.advanced).toEqual([]);
    run(r, 3);
    expect(r.advanced).toEqual(['b']);
  });

  it('never fades over more than half of either track', () => {
    r.fade.sec = 12;
    // Current 10 s, next 8 s: half of the shorter is 4 s.
    r.engine.start(A);
    r.engine.setNext(B);
    run(r, 6.5);
    const c = r.gains[0].scheduled.find((e) => e.kind === 'curve')!;
    expect(c.dur).toBeCloseTo(4, 1);
    expect(c.at).toBeCloseTo(6, 1);
  });

  it('a setting above the cap is held to 12 seconds', () => {
    const long = rig({ 'a.mp3': 100, 'b.mp3': 100 });
    long.fade.sec = 99;
    long.engine.start(A);
    long.engine.setNext(B);
    run(long, 88.5);
    const c = long.gains[0].scheduled.find((e) => e.kind === 'curve')!;
    expect(c.dur).toBeCloseTo(12, 1);
  });

  it('a late next-track starts at once with a shorter fade, still ending on the boundary', () => {
    r.fade.sec = 3;
    r.engine.start(A);
    run(r, 8); // two seconds left, nothing queued
    r.engine.setNext(B);
    run(r, 0.1);
    const c = r.gains[0].scheduled.find((e) => e.kind === 'curve')!;
    expect(c.at + c.dur!).toBeCloseTo(10, 2);
    expect(c.dur).toBeLessThan(2.1);
    run(r, 2);
    expect(r.advanced).toEqual(['b']);
  });

  it('measures the real start latency and starts the next hand-over earlier', () => {
    r.engine.start(A);
    r.engine.setNext(B);
    r.decks[1].startDelay = 0.2;
    run(r, 10.3);
    expect(r.advanced).toEqual(['b']);
    // The estimate moved from 0.05 toward the measured 0.2.
    expect(r.engine.playLatency).toBeGreaterThan(0.1);
    expect(r.engine.playLatency).toBeLessThanOrEqual(0.2);
  });

  it('without a next track the end is reported once and nothing else starts', () => {
    r.engine.start(A);
    run(r, 10.5);
    expect(r.ended).toBe(1);
    expect(r.decks[1].playCalls).toEqual([]);
    expect(r.advanced).toEqual([]);
  });

  it('a timer that missed the boundary falls back to switching on the ended event', () => {
    r.engine.start(A);
    r.engine.setNext(B);
    // No ticks at all: only the element events.
    r.clock.advance(10.1, () => undefined);
    expect(r.advanced).toEqual(['b']);
    expect(r.decks[1].playCalls).toHaveLength(1);
    expect(gainAt(r.gains[1], r.clock.currentTime)).toBe(1);
    expect(gainAt(r.gains[0], r.clock.currentTime)).toBe(0);
  });

  it('seeking back before the hand-over moves the hand-over later', () => {
    r.engine.start(A);
    r.engine.setNext(B);
    run(r, 9);
    r.engine.seek(2);
    run(r, 1.5);
    expect(r.decks[1].playCalls).toEqual([]);
    run(r, 7.0);
    expect(r.decks[1].playCalls).toHaveLength(1);
  });

  it('pausing inside a crossfade completes the hand-over: the incoming track is the current one', () => {
    r.fade.sec = 3;
    r.engine.start(A);
    r.engine.setNext(B);
    run(r, 8.5);
    expect(r.engine.transitioning).toBe(true);
    r.engine.pause();
    expect(r.engine.transitioning).toBe(false);
    expect(r.advanced).toEqual(['b']);
    expect(r.decks[0].paused).toBe(true);
    expect(r.decks[1].paused).toBe(true);
    expect(gainAt(r.gains[1], r.clock.currentTime)).toBe(1);
    r.engine.resume();
    expect(r.decks[1].paused).toBe(false);
  });

  it('start() during a hand-over drops it and plays the new track alone', () => {
    r.fade.sec = 3;
    r.engine.start(A);
    r.engine.setNext(B);
    run(r, 8);
    expect(r.engine.transitioning).toBe(true);
    r.engine.start(C);
    expect(r.engine.transitioning).toBe(false);
    expect(r.decks[1].paused).toBe(true);
    expect(r.decks[0].src).toBe('c.mp3');
    expect(gainAt(r.gains[0], r.clock.currentTime)).toBe(1);
    expect(gainAt(r.gains[1], r.clock.currentTime)).toBe(0);
    run(r, 2);
    expect(r.advanced).toEqual([]);
  });

  it('stop() silences both decks and forgets the queued track', () => {
    r.engine.start(A);
    r.engine.setNext(B);
    r.engine.stop();
    expect(r.decks[0].paused).toBe(true);
    expect(r.engine.current).toBeNull();
    run(r, 12);
    expect(r.ended).toBe(0);
    expect(r.advanced).toEqual([]);
  });

  it('uses the duration hint until the element knows its length', () => {
    const lengths = { 'a.mp3': Number.NaN };
    const h = rig(lengths);
    h.engine.start({ id: 'a', src: 'a.mp3', durationSec: 4 });
    h.engine.setNext({ id: 'b', src: 'b.mp3', durationSec: 4 });
    // Make the fake element report no length at all (metadata not in yet).
    run(h, 3.97);
    expect(h.decks[1].playCalls).toHaveLength(1);
  });

  it('destroy stops reacting to the elements', () => {
    r.engine.destroy();
    r.engine.start(A);
    r.decks[0].emit('ended');
    expect(r.ended).toBe(0);
  });
});

describe('fade curves', () => {
  it('are monotone and equal-power', () => {
    const inn = fadeCurve('in');
    const out = fadeCurve('out');
    for (let i = 1; i < inn.length; i++) {
      expect(inn[i]!).toBeGreaterThanOrEqual(inn[i - 1]!);
      expect(out[i]!).toBeLessThanOrEqual(out[i - 1]!);
    }
  });
});
