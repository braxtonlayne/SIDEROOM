/**
 * Wave-30 integration: the player engine running music-lib's gapless decks (MUS-LIB-001) over a fake Web Audio
 * context and a fake clock. The single-element path (no Web Audio) is `engine.test.ts`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { dbToGain } from '../../music/eq/presets';
import { __resetPwaState, applyPendingUpdate, setUpdateReady } from '../../pwa/state';
import { createPlayerEngine, RESUME_KEY, type PlayerEngine } from '../engine';
import {
  __resetPlayerStore,
  enqueue,
  pause,
  play,
  setCrossfade,
  setEq,
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
  playCalls = 0;
  loads = 0;
  play = vi.fn(() => {
    this.playCalls++;
    this.paused = false;
    queueMicrotask(() => this.dispatchEvent(new Event('playing')));
    return Promise.resolve();
  });
  pause = vi.fn(() => {
    this.paused = true;
  });
  load = vi.fn(() => {
    this.loads++;
  });
  removeAttribute = vi.fn((name: string) => {
    if (name === 'src') this.src = '';
  });
  fire(type: string) {
    this.dispatchEvent(new Event(type));
  }
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
  sources: { el: unknown; node: FakeNode }[] = [];
  resume = vi.fn(() => Promise.resolve());
  createMediaElementSource(el: unknown) {
    const node = new FakeNode();
    this.sources.push({ el, node });
    return node;
  }
  createGain() {
    return Object.assign(new FakeNode(), { gain: new FakeParam(1) });
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

/** Let the playing deck reach `left` seconds before its end and the timer look at the clock. */
function nearEnd(el: FakeAudio, length: number, left: number) {
  el.duration = length;
  el.currentTime = length - left;
  tick();
}

describe('gapless engine', () => {
  it('builds both decks into one EQ chain and prefetches the next track into the idle deck', async () => {
    play([track(1), track(2)]);
    await flush();
    expect(engine.gapless()).toBe(true);
    expect(ctx.sources.map((s) => s.el)).toEqual([a, b]);
    const g = engine.graph()!;
    // Each deck: source -> its gain -> the EQ chain's input (the preamp).
    expect(ctx.sources[0]!.node.to).toEqual([g.deckGains[0]]);
    expect((g.deckGains[0] as unknown as FakeNode).to).toEqual([g.eq.input]);
    expect((g.deckGains[1] as unknown as FakeNode).to).toEqual([g.eq.input]);
    expect(a.src).toBe('/t1.mp3');
    expect(a.playCalls).toBe(1);
    expect(state().status).toBe('playing');
    expect(b.src).toBe('/t2.mp3');
    expect(b.loads).toBe(1);
    expect(b.playCalls).toBe(0);
  });

  it('hands over at the boundary on the audio clock, and the store follows without a reload', async () => {
    play([track(1), track(2), track(3)]);
    await flush();
    nearEnd(a, 30, 0.01);
    expect(b.playCalls).toBe(1); // started a measured latency early
    const gainA = engine.graph()!.deckGains[0]!.gain as unknown as FakeParam;
    expect(gainA.calls.some((c) => c.fn === 'set' && c.args[0] === 0)).toBe(true);
    ctx.currentTime += 0.05;
    tick();
    await flush();
    expect(state().index).toBe(1);
    expect(state().status).toBe('playing');
    expect(engine.activeAudio()).toBe(b);
    expect(b.src).toBe('/t2.mp3'); // not reloaded
    expect(b.playCalls).toBe(1);
    // The track after it is lined up in the deck that just went quiet.
    expect(a.src).toBe('/t3.mp3');
    // Clock reports now come from the new deck only.
    b.currentTime = 4;
    b.duration = 30;
    b.fire('timeupdate');
    a.currentTime = 99;
    a.fire('timeupdate');
    expect(state().position).toBe(4);
  });

  it('crossfades with the equal-power curves over the chosen seconds', async () => {
    setCrossfade(4);
    play([track(1), track(2)]);
    await flush();
    nearEnd(a, 30, 4);
    const [ga, gb] = engine.graph()!.deckGains.map((g) => g.gain as unknown as FakeParam);
    const outCurve = ga!.calls.find((c) => c.fn === 'curve');
    const inCurve = gb!.calls.find((c) => c.fn === 'curve');
    expect(outCurve?.args[2]).toBeCloseTo(4, 1);
    expect(inCurve?.args[2]).toBeCloseTo(4, 1);
    const out = outCurve!.args[0] as Float32Array;
    const inn = inCurve!.args[0] as Float32Array;
    expect(out[0]).toBeCloseTo(1);
    expect(inn[inn.length - 1]).toBeCloseTo(1);
    expect(out[10]! ** 2 + inn[10]! ** 2).toBeCloseTo(1);
    expect(b.playCalls).toBe(1);
  });

  it('a track added next replaces what was lined up', async () => {
    play([track(1), track(2)]);
    await flush();
    expect(b.src).toBe('/t2.mp3');
    enqueue(track(9), { next: true });
    await flush();
    expect(b.src).toBe('/t9.mp3');
  });

  it('pause and resume act on the deck that plays', async () => {
    play([track(1), track(2)]);
    await flush();
    pause();
    expect(a.pause).toHaveBeenCalled();
    play();
    await flush();
    expect(a.playCalls).toBe(2);
    expect(b.playCalls).toBe(0);
  });

  it('a deferred app update reloads at the hand-over, with the queue parked at the new track', async () => {
    const apply = vi.fn();
    setUpdateReady(apply);
    play([track(1), track(2)]);
    await flush();
    applyPendingUpdate();
    expect(apply).not.toHaveBeenCalled();
    nearEnd(a, 30, 0.01);
    ctx.currentTime += 0.05;
    tick();
    expect(apply).toHaveBeenCalledOnce();
    const parked = JSON.parse(sessionStorage.getItem(RESUME_KEY) ?? 'null');
    expect(parked.index).toBe(1);
    expect(state().status).toBe('paused');
  });

  it('an update asked for before the next track is lined up lets the track end, then reloads', async () => {
    const apply = vi.fn();
    setUpdateReady(apply);
    play([track(1)]);
    await flush();
    applyPendingUpdate();
    enqueue(track(2)); // nothing is lined up behind a deferred update
    await flush();
    expect(b.src).toBe('');
    a.fire('ended');
    expect(apply).toHaveBeenCalledOnce();
  });

  it('the EQ is music-lib chain driven by the store: preamp headroom and a glide', async () => {
    play([track(1)]);
    await flush();
    setEq([6, 0, 0, 0, 0, 0, 0, 0, 0, -3]);
    const g = engine.graph()!;
    const low = g.eq.bands[0]!.gain as unknown as FakeParam;
    const pre = g.eq.preamp.gain as unknown as FakeParam;
    expect(low.calls.at(-1)).toMatchObject({ fn: 'target' });
    expect(low.value).toBe(6);
    expect(pre.value).toBeCloseTo(dbToGain(-6));
    usePlayerStore.setState({ eqEnabled: false });
    expect(low.value).toBe(0);
    expect(pre.value).toBeCloseTo(1);
  });

  it('the last track ends the queue', async () => {
    play([track(1)]);
    await flush();
    a.fire('ended');
    expect(state().status).toBe('ended');
  });
});
