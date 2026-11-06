/**
 * MUS-SKN-002 (D-525): the shared plumbing of the skins' extras. The rate and its pitch policy on both decks (and
 * across a gapless hand-over), the balance node in the graph, and the sleep timer's fade, pause and restore, over
 * a fake Web Audio context through the engine's `ContextFactory` seam.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { __resetPwaState } from '../../pwa/state';
import { createPlayerEngine, type PlayerEngine } from '../engine';
import { startSleepWatcher, SLEEP_FADE_MS, type SleepTimers } from '../sleep';
import {
  __resetPlayerStore,
  clear,
  play,
  setPan,
  setRate,
  setSleep,
  setVolume,
  stop,
  usePlayerStore,
} from '../store';
import type { PlayerTrack } from '../types';

class FakeAudio extends EventTarget {
  src = '';
  currentTime = 0;
  duration = Number.NaN;
  paused = true;
  volume = 1;
  preload = '';
  crossOrigin: string | null = null;
  error: { message: string } | null = null;
  playbackRate = 1;
  defaultPlaybackRate = 1;
  preservesPitch = true;
  webkitPreservesPitch?: boolean;
  playCalls = 0;
  play = vi.fn(() => {
    this.playCalls++;
    this.paused = false;
    queueMicrotask(() => this.dispatchEvent(new Event('playing')));
    return Promise.resolve();
  });
  pause = vi.fn(() => {
    this.paused = true;
  });
  // Loading a source puts the rate back to the default rate, as a real media element does.
  load = vi.fn(() => {
    this.playbackRate = this.defaultPlaybackRate;
  });
  removeAttribute = vi.fn((name: string) => {
    if (name === 'src') this.src = '';
  });
}

class FakeParam {
  value: number;
  calls: { fn: string; args: unknown[] }[] = [];
  constructor(v = 1) {
    this.value = v;
  }
  cancelScheduledValues(...args: unknown[]) {
    this.calls.push({ fn: 'cancel', args });
  }
  setValueAtTime(v: number, t: number) {
    this.calls.push({ fn: 'set', args: [v, t] });
    this.value = v;
  }
  linearRampToValueAtTime(v: number, t: number) {
    this.calls.push({ fn: 'ramp', args: [v, t] });
  }
  setValueCurveAtTime(c: Float32Array, t: number, d: number) {
    this.calls.push({ fn: 'curve', args: [c, t, d] });
  }
  setTargetAtTime(v: number, t: number, k: number) {
    this.calls.push({ fn: 'target', args: [v, t, k] });
    this.value = v;
  }
}

class FakeNode {
  to: unknown[] = [];
  connect(n: unknown) {
    this.to.push(n);
    return n;
  }
  disconnect() {
    this.to = [];
  }
}

class FakeContext {
  currentTime = 100;
  destination = new FakeNode();
  resume = vi.fn(() => Promise.resolve());
  createMediaElementSource() {
    return new FakeNode();
  }
  createGain() {
    return Object.assign(new FakeNode(), { gain: new FakeParam(1) });
  }
  createStereoPanner() {
    return Object.assign(new FakeNode(), { pan: new FakeParam(0) });
  }
  createBiquadFilter() {
    return Object.assign(new FakeNode(), {
      type: '',
      frequency: new FakeParam(0),
      Q: new FakeParam(0),
      gain: new FakeParam(0),
    });
  }
  createAnalyser() {
    return Object.assign(new FakeNode(), {
      fftSize: 0,
      smoothingTimeConstant: 0,
      getByteFrequencyData: () => undefined,
    });
  }
}

const track = (n: number): PlayerTrack => ({
  id: `t${n}`,
  title: `Track ${n}`,
  src: `/t${n}.mp3`,
  durationSec: 30,
});
const flush = () => new Promise((r) => setTimeout(r, 0));
const state = () => usePlayerStore.getState();

let a: FakeAudio;
let b: FakeAudio;
let ctx: FakeContext;
let tick: () => void;
let engine: PlayerEngine;

beforeEach(() => {
  sessionStorage.clear();
  __resetPwaState();
  __resetPlayerStore();
  a = new FakeAudio();
  b = new FakeAudio();
  ctx = new FakeContext();
  tick = () => undefined;
  engine = createPlayerEngine(a as unknown as HTMLAudioElement, {
    nextAudio: b as unknown as HTMLAudioElement,
    contextFactory: () => ctx as unknown as AudioContext,
    timers: {
      setInterval: (fn) => {
        tick = fn;
        return 1;
      },
      clearInterval: () => undefined,
    },
  });
});
afterEach(() => engine.destroy());

describe('rate', () => {
  it('applies the rate and the pitch policy to both decks, and keeps it across a load', async () => {
    setRate(1.1, { pitchFollows: true });
    for (const el of [a, b]) {
      expect(el.playbackRate).toBe(1.1);
      expect(el.defaultPlaybackRate).toBe(1.1);
      expect(el.preservesPitch).toBe(false);
      expect(el.webkitPreservesPitch).toBe(false);
    }
    play([track(1), track(2)]);
    await flush();
    // The idle deck was loaded with the next track: it still runs at the rate.
    expect(b.src).toBe('/t2.mp3');
    expect(b.playbackRate).toBe(1.1);
    setRate(0.96, { pitchFollows: false });
    expect(a.playbackRate).toBe(0.96);
    expect(a.preservesPitch).toBe(true);
    expect(b.webkitPreservesPitch).toBe(true);
  });

  it('starts the next deck of a gapless hand-over at the rate', async () => {
    play([track(1), track(2), track(3)]);
    await flush();
    setRate(1.2, { pitchFollows: true });
    // 2.4 media seconds left at 1.2x is 2 clock seconds: not yet inside the hand-over window.
    a.duration = 30;
    a.currentTime = 27.6;
    tick();
    expect(b.playCalls).toBe(0);
    a.currentTime = 30 - 0.01;
    tick();
    expect(b.playCalls).toBe(1);
    b.playbackRate = 1; // a browser that reset it under us
    ctx.currentTime += 0.05;
    tick();
    await flush();
    expect(state().index).toBe(1);
    expect(engine.activeAudio()).toBe(b);
    expect(b.playbackRate).toBe(1.2);
    // And the deck that went quiet was loaded with the track after, at the rate.
    expect(a.src).toBe('/t3.mp3');
    expect(a.playbackRate).toBe(1.2);
  });

  it('clamps, and a new queue keeps rate and balance', () => {
    setRate(9);
    expect(state().rate).toBe(2);
    setRate(Number.NaN);
    expect(state().rate).toBe(1);
    setRate(1.05, { pitchFollows: true });
    setPan(-0.4);
    play([track(1)]);
    expect(state().rate).toBe(1.05);
    expect(state().pitchFollowsRate).toBe(true);
    expect(state().pan).toBe(-0.4);
  });
});

describe('balance', () => {
  it('puts a stereo panner between the master gain and the analyser and follows the store', async () => {
    setPan(0.3);
    play([track(1)]);
    await flush();
    const g = engine.graph()!;
    expect(g.panner).not.toBeNull();
    expect((g.master as unknown as FakeNode).to).toEqual([g.panner]);
    expect((g.panner as unknown as FakeNode).to).toEqual([g.analyser]);
    expect(g.panner!.pan.value).toBe(0.3);
    setPan(-2);
    expect(g.panner!.pan.value).toBe(-1);
  });

  it('keeps the old chain where the browser has no stereo panner', async () => {
    engine.destroy();
    const plain = Object.assign(new FakeContext(), { createStereoPanner: undefined });
    const c = new FakeAudio();
    engine = createPlayerEngine(c as unknown as HTMLAudioElement, {
      contextFactory: () => plain as unknown as AudioContext,
    });
    play([track(1)]);
    await flush();
    const g = engine.graph()!;
    expect(g.panner).toBeNull();
    expect((g.master as unknown as FakeNode).to).toEqual([g.analyser]);
    setPan(0.5); // nothing to set, nothing breaks
  });
});

describe('sleep timer', () => {
  let now: number;
  let pending: { at: number; fn: () => void; id: number }[];
  let timers: SleepTimers;
  let stopWatch: () => void;
  const advance = (ms: number) => {
    now += ms;
    for (;;) {
      const due = pending.filter((p) => p.at <= now).sort((x, y) => x.at - y.at)[0];
      if (!due) break;
      pending = pending.filter((p) => p !== due);
      due.fn();
    }
  };

  beforeEach(async () => {
    now = 1_000_000;
    pending = [];
    let id = 0;
    timers = {
      setTimeout: (fn, ms) => {
        const p = { at: now + ms, fn, id: ++id };
        pending.push(p);
        return p.id;
      },
      clearTimeout: (h) => {
        pending = pending.filter((p) => p.id !== h);
      },
      now: () => now,
    };
    setVolume(0.6);
    play([track(1)]);
    await flush();
    stopWatch = startSleepWatcher(() => engine.graph(), timers);
  });
  afterEach(() => stopWatch());

  const masterGain = () => engine.graph()!.master.gain as unknown as FakeParam;

  it('fades the master gain over the last 30 seconds, pauses at the end and restores the gain', () => {
    setSleep(15, now);
    expect(state().sleepAt).toBe(now + 15 * 60_000);
    advance(15 * 60_000 - SLEEP_FADE_MS - 1);
    expect(masterGain().calls.some((c) => c.fn === 'ramp')).toBe(false);
    advance(1);
    const ramp = masterGain().calls.find((c) => c.fn === 'ramp');
    expect(ramp?.args).toEqual([0, ctx.currentTime + SLEEP_FADE_MS / 1000]);
    expect(state().status).toBe('playing');
    advance(SLEEP_FADE_MS);
    expect(state().status).toBe('paused');
    expect(state().sleepAt).toBeNull();
    expect(masterGain().value).toBe(0.6);
    expect(masterGain().calls.at(-1)).toEqual({ fn: 'cancel', args: [ctx.currentTime] });
  });

  it('turning it off mid-fade cancels the ramp and restores the gain at once', () => {
    setSleep(1, now);
    advance(60_000 - 10_000);
    expect(masterGain().calls.some((c) => c.fn === 'ramp')).toBe(true);
    masterGain().value = 0.2; // part-way down
    setSleep(null);
    expect(masterGain().calls.at(-1)?.fn).toBe('cancel');
    expect(masterGain().value).toBe(0.6);
    advance(60_000);
    expect(state().status).toBe('playing');
  });

  it('stop and close clear it', () => {
    setSleep(30, now);
    stop();
    expect(state().sleepAt).toBeNull();
    expect(pending).toHaveLength(0);
    setSleep(30, now);
    clear();
    expect(state().sleepAt).toBeNull();
    expect(pending).toHaveLength(0);
  });

  it('close resets the speed, the pitch policy and the balance, a new queue keeps them (D-530)', () => {
    setRate(1.25, { pitchFollows: true });
    setPan(-0.5);
    play([track(1), track(2)]);
    expect(state().rate).toBe(1.25);
    expect(state().pan).toBe(-0.5);
    clear();
    expect(state().rate).toBe(1);
    expect(state().pitchFollowsRate).toBe(false);
    expect(state().pan).toBe(0);
  });
});
