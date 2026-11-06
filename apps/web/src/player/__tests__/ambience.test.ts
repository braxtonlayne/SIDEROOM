/**
 * MUS-SKN-002 (D-524): the Study skin's ambience mixer, synthesised with Web Audio. A fake context checks that
 * the three generators are built, the slider gains follow the levels, the context suspends at zero and resumes
 * above it, the bursts and clinks run only while their slider is up, and `dispose` closes everything.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  __resetAmbience,
  ambienceLevels,
  createAmbience,
  releaseAmbienceIfSilent,
  setAmbienceLevel,
  type Ambience,
  type AmbienceTimers,
} from '../ambience';
import { __resetPlayerStore, clear, play, usePlayerStore } from '../store';

class Param {
  value: number;
  calls: { fn: string; args: number[] }[] = [];
  constructor(v = 0) {
    this.value = v;
  }
  cancelScheduledValues(t: number) {
    this.calls.push({ fn: 'cancel', args: [t] });
  }
  setValueAtTime(v: number, t: number) {
    this.calls.push({ fn: 'set', args: [v, t] });
    this.value = v;
  }
  linearRampToValueAtTime(v: number, t: number) {
    this.calls.push({ fn: 'ramp', args: [v, t] });
    this.value = v;
  }
  exponentialRampToValueAtTime(v: number, t: number) {
    this.calls.push({ fn: 'exp', args: [v, t] });
  }
}

class Node {
  to: unknown[] = [];
  connect(n: unknown) {
    this.to.push(n);
    return n;
  }
}

class Source extends Node {
  started = 0;
  stopped = 0;
  buffer: unknown = null;
  loop = false;
  start = vi.fn(() => {
    this.started++;
  });
  stop = vi.fn(() => {
    this.stopped++;
  });
}

class Ctx {
  sampleRate = 8000;
  currentTime = 5;
  destination = new Node();
  gains: (Node & { gain: Param })[] = [];
  filters: (Node & { type: string; frequency: Param; Q: Param; gain: Param })[] = [];
  sources: Source[] = [];
  oscillators: (Source & { type: string; frequency: Param })[] = [];
  suspend = vi.fn(() => Promise.resolve());
  resume = vi.fn(() => Promise.resolve());
  close = vi.fn(() => Promise.resolve());
  createBuffer(_ch: number, n: number) {
    const data = new Float32Array(n);
    return { getChannelData: () => data, length: n };
  }
  createBufferSource() {
    const s = new Source();
    this.sources.push(s);
    return s;
  }
  createOscillator() {
    const o = Object.assign(new Source(), { type: 'sine', frequency: new Param(440) });
    this.oscillators.push(o);
    return o;
  }
  createBiquadFilter() {
    const f = Object.assign(new Node(), {
      type: '',
      frequency: new Param(),
      Q: new Param(),
      gain: new Param(),
    });
    this.filters.push(f);
    return f;
  }
  createGain() {
    const g = Object.assign(new Node(), { gain: new Param(1) });
    this.gains.push(g);
    return g;
  }
}

let ctx: Ctx;
let pending: { fn: () => void; id: number; ms: number }[];
let timers: AmbienceTimers;
let mixer: Ambience;

/** The three slider gains are the gains wired straight to the destination. */
const outputs = () => ctx.gains.filter((g) => g.to.includes(ctx.destination));

beforeEach(() => {
  ctx = new Ctx();
  pending = [];
  let id = 0;
  timers = {
    setTimeout: (fn, ms) => {
      const p = { fn, id: ++id, ms };
      pending.push(p);
      return p.id;
    },
    clearTimeout: (h) => {
      pending = pending.filter((p) => p.id !== h);
    },
  };
  mixer = createAmbience(
    () => ctx as unknown as AudioContext,
    () => 0.5,
    timers,
  )!;
});

describe('createAmbience', () => {
  it('builds the three generators, silent, without any sound file', () => {
    expect(outputs()).toHaveLength(3);
    for (const g of outputs()) expect(g.gain.value).toBe(0);
    const types = ctx.filters.map((f) => `${f.type}@${f.frequency.value}`);
    expect(types).toEqual(
      expect.arrayContaining(['bandpass@1500', 'highshelf@5000', 'lowpass@3000', 'lowpass@400']),
    );
    // Looping noise for rain (body and hiss), the crackle bed and the café room; two slow LFOs.
    expect(ctx.sources.filter((s) => s.loop && s.started === 1)).toHaveLength(4);
    expect(ctx.oscillators.map((o) => o.frequency.value).sort()).toEqual([0.05, 0.1]);
    expect(ctx.oscillators.every((o) => o.started === 1)).toBe(true);
  });

  it('ramps a slider gain in 50 ms, resumes above zero and suspends when all are at zero', () => {
    const [rain, crackle] = outputs();
    mixer.set('rain', 0.65);
    const ramp = rain!.gain.calls.find((c) => c.fn === 'ramp');
    expect(ramp?.args[1]).toBeCloseTo(ctx.currentTime + 0.05);
    expect(ramp!.args[0]).toBeGreaterThan(0);
    expect(ctx.resume).toHaveBeenCalledTimes(1);
    mixer.set('crackle', 0.4);
    expect(crackle!.gain.value).toBeGreaterThan(0);
    mixer.set('rain', 0);
    expect(ctx.suspend).not.toHaveBeenCalled();
    mixer.set('crackle', 0);
    expect(ctx.suspend).toHaveBeenCalledTimes(1);
    expect(rain!.gain.value).toBe(0);
  });

  it('pops crackle bursts and café clinks only while their slider is up', () => {
    mixer.set('rain', 1);
    expect(pending).toHaveLength(0);
    mixer.set('crackle', 0.5);
    expect(pending).toHaveLength(1);
    expect(pending[0]!.ms).toBeGreaterThanOrEqual(80);
    expect(pending[0]!.ms).toBeLessThanOrEqual(250);
    const before = ctx.sources.length;
    pending.shift()!.fn();
    expect(ctx.sources.length).toBe(before + 1); // one burst
    expect(pending).toHaveLength(1); // and the next one scheduled
    mixer.set('crackle', 0);
    pending.shift()!.fn();
    expect(pending).toHaveLength(0);

    mixer.set('cafe', 0.2);
    expect(pending[0]!.ms).toBeGreaterThanOrEqual(3000);
    expect(pending[0]!.ms).toBeLessThanOrEqual(9000);
    const oscs = ctx.oscillators.length;
    pending.shift()!.fn();
    expect(ctx.oscillators.length).toBe(oscs + 1);
    expect(ctx.oscillators.at(-1)!.frequency.value).toBeGreaterThanOrEqual(2000);
    expect(ctx.oscillators.at(-1)!.stop).toHaveBeenCalled();
  });

  it('dispose stops the sources, cancels the timers and closes the context', () => {
    mixer.set('crackle', 1);
    mixer.set('cafe', 1);
    mixer.dispose();
    expect(pending).toHaveLength(0);
    expect(ctx.close).toHaveBeenCalledTimes(1);
    expect(ctx.sources.filter((s) => s.loop).every((s) => s.stopped === 1)).toBe(true);
    mixer.set('rain', 1); // a no-op once disposed
    expect(ctx.resume).toHaveBeenCalledTimes(2);
  });

  it('returns null where the browser has no Web Audio', () => {
    expect(createAmbience(() => null)).toBeNull();
  });
});

describe('the mixer for the tab', () => {
  let made: { set: ReturnType<typeof vi.fn>; dispose: ReturnType<typeof vi.fn> }[];
  beforeEach(() => {
    __resetPlayerStore();
    made = [];
    __resetAmbience(() => {
      const m = { set: vi.fn(), dispose: vi.fn() };
      made.push(m);
      return m;
    });
    play([{ id: 'a', title: 'A', src: '/a.mp3' }]);
  });
  afterEach(() => __resetAmbience());

  it('is created on the first level above zero and keeps the levels', () => {
    setAmbienceLevel('rain', 0);
    expect(made).toHaveLength(0);
    setAmbienceLevel('rain', 0.65);
    expect(made).toHaveLength(1);
    expect(made[0]!.set).toHaveBeenCalledWith('rain', 0.65);
    setAmbienceLevel('cafe', 0.15);
    expect(made).toHaveLength(1);
    expect(ambienceLevels()).toEqual({ rain: 0.65, crackle: 0, cafe: 0.15 });
  });

  it('stays when the skin goes away with a level up, closes when all are at zero', () => {
    setAmbienceLevel('crackle', 0.4);
    releaseAmbienceIfSilent();
    expect(made[0]!.dispose).not.toHaveBeenCalled();
    setAmbienceLevel('crackle', 0);
    releaseAmbienceIfSilent();
    expect(made[0]!.dispose).toHaveBeenCalledTimes(1);
  });

  it('keeps playing while the music pauses and stops when the player closes', () => {
    setAmbienceLevel('rain', 0.5);
    usePlayerStore.setState({ status: 'paused' });
    expect(made[0]!.dispose).not.toHaveBeenCalled();
    clear();
    expect(made[0]!.dispose).toHaveBeenCalledTimes(1);
    expect(ambienceLevels()).toEqual({ rain: 0, crackle: 0, cafe: 0 });
  });
});
