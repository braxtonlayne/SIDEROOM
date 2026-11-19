import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { DEFAULT_VOICE_SETTINGS, sanitizeVoiceSettings, useVoiceSettings } from '../settings';
import { VadEngineHost, type VadDetector, type VadDetectorState } from '../vad';
import { ProbGate } from '../silero/probGate';

class FakeDetector implements VadDetector {
  gate = new ProbGate();
  state: VadDetectorState = 'idle';
  reason: string | null = null;
  frames = 0;
  speechFrames = 0;
  meanMs = 1;
  maxMs = 2;
  started: unknown[] = [];
  stops = 0;
  probCbs = new Set<(p: number) => void>();
  stateCbs = new Set<(s: VadDetectorState, r: string | null) => void>();
  start(ctx: AudioContext, source: AudioNode) {
    this.started.push([ctx, source]);
    this.to('loading');
  }
  stop() {
    this.stops++;
  }
  reset() {}
  onProb(cb: (p: number) => void) {
    this.probCbs.add(cb);
    return () => this.probCbs.delete(cb);
  }
  onState(cb: (s: VadDetectorState, r: string | null) => void) {
    this.stateCbs.add(cb);
    return () => this.stateCbs.delete(cb);
  }
  to(s: VadDetectorState, r: string | null = null) {
    this.state = s;
    this.reason = r;
    for (const cb of [...this.stateCbs]) cb(s, r);
  }
  prob(p: number) {
    for (const cb of [...this.probCbs]) cb(p);
  }
}

function rig() {
  const flags = { wanted: false, loadFails: false };
  const dets: FakeDetector[] = [];
  const clock = { t: 0 };
  const host = new VadEngineHost({
    wanted: () => flags.wanted,
    engine: () => (flags.wanted ? 'silero' : 'energy'),
    load: async () => {
      if (flags.loadFails) throw new Error('chunk');
      const d = new FakeDetector();
      dets.push(d);
      return d;
    },
    now: () => clock.t,
  });
  const seen: string[] = [];
  host.subscribe(() => seen.push(host.state + (host.open ? '+open' : '')));
  const tap = () => ({ ctx: {} as AudioContext, source: {} as AudioNode });
  return { host, flags, dets, clock, seen, tap };
}
const tick = () => new Promise((r) => setTimeout(r, 0));

describe('VadEngineHost (D-1048, D-1050, D-1051)', () => {
  it('with the Standard engine nothing is loaded and nothing drives', async () => {
    const r = rig();
    const t = r.tap();
    r.host.setTap({}, 'call', t.ctx, t.source);
    await tick();
    expect(r.dets).toHaveLength(0);
    expect(r.host.state).toBe('idle');
    expect(r.host.drives).toBe(false);
    expect(r.host.snapshot()).toMatchObject({
      engine: 'energy',
      state: 'idle',
      frames: 0,
      fallbackReason: null,
    });
  });

  it('loads only once chosen and tapped; the energy gate drives until ready; then the gate follows the probabilities', async () => {
    const r = rig();
    r.flags.wanted = true;
    const t = r.tap();
    r.host.setTap({}, 'call', t.ctx, t.source);
    expect(r.host.state).toBe('loading');
    expect(r.host.drives).toBe(false);
    await tick();
    const d = r.dets[0]!;
    expect(d.started).toEqual([[t.ctx, t.source]]);
    d.to('ready');
    expect(r.host.drives).toBe(true);
    d.prob(0.9);
    expect(r.host.open).toBe(false); // one frame is not enough
    d.prob(0.9);
    expect(r.host.open).toBe(true);
    r.clock.t = 1000;
    d.prob(0.05);
    expect(r.host.open).toBe(false);
    expect(r.seen).toEqual(['loading', 'ready', 'ready+open', 'ready']);
  });

  it('falls back when the detector says so, holds for the call and tries again on a new one', async () => {
    const r = rig();
    r.flags.wanted = true;
    const t = r.tap();
    const owner = {};
    r.host.setTap(owner, 'call', t.ctx, t.source);
    await tick();
    r.dets[0]!.to('ready');
    r.dets[0]!.prob(0.9);
    r.dets[0]!.prob(0.9);
    r.dets[0]!.to('fallback', 'slow');
    expect(r.host.state).toBe('fallback');
    expect(r.host.reason).toBe('slow');
    expect(r.host.drives).toBe(false);
    expect(r.host.open).toBe(false);
    expect(r.dets[0]!.stops).toBeGreaterThan(0);
    // a device change in the same call builds a new graph for the same owner: still the energy gate
    const t2 = r.tap();
    r.host.clearTap(owner);
    r.host.setTap(owner, 'call', t2.ctx, t2.source);
    await tick();
    expect(r.dets).toHaveLength(1);
    expect(r.host.state).toBe('fallback');
    // the next call tries again
    r.host.resetBlock();
    await tick();
    expect(r.dets).toHaveLength(2);
    expect(r.host.state).toBe('loading');
  });

  it('a chunk that cannot be fetched is a fallback with the reason "chunk"', async () => {
    const r = rig();
    r.flags.wanted = true;
    r.flags.loadFails = true;
    const t = r.tap();
    r.host.setTap({}, 'call', t.ctx, t.source);
    await tick();
    expect([r.host.state, r.host.reason, r.host.snapshot().fallbackReason]).toEqual([
      'fallback',
      'chunk',
      'chunk',
    ]);
  });

  it('stops when the engine or mode is switched off or the tap goes, and restarts cleanly', async () => {
    const r = rig();
    r.flags.wanted = true;
    const t = r.tap();
    const owner = {};
    r.host.setTap(owner, 'call', t.ctx, t.source);
    await tick();
    r.dets[0]!.to('ready');
    r.flags.wanted = false;
    r.host.refresh();
    expect(r.host.state).toBe('idle');
    expect(r.dets[0]!.stops).toBe(1);
    r.flags.wanted = true;
    r.host.refresh();
    await tick();
    expect(r.dets).toHaveLength(2);
    r.host.clearTap(owner);
    expect(r.host.state).toBe('idle');
    expect(r.dets[1]!.stops).toBe(1);
  });

  it('the call keeps the detector when the settings test opens and closes a microphone', async () => {
    const r = rig();
    r.flags.wanted = true;
    const call = {};
    const test = {};
    const c = r.tap();
    r.host.setTap(call, 'call', c.ctx, c.source);
    await tick();
    const t = r.tap();
    r.host.setTap(test, 'test', t.ctx, t.source);
    await tick();
    expect(r.dets).toHaveLength(1);
    r.host.clearTap(test);
    expect(r.dets[0]!.stops).toBe(0);
    expect(r.host.role).toBe('call');
  });

  it('a late load for a tap that is gone is dropped', async () => {
    const r = rig();
    r.flags.wanted = true;
    const owner = {};
    const t = r.tap();
    r.host.setTap(owner, 'call', t.ctx, t.source);
    r.host.clearTap(owner);
    await tick();
    expect(r.dets[0]?.started ?? []).toEqual([]);
    expect(r.host.state).toBe('idle');
  });
});

describe('vadEngine setting (D-1048)', () => {
  beforeEach(() => localStorage.clear());
  afterEach(() => useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS }));

  it('defaults to energy, keeps silero, and turns anything else into energy', () => {
    expect(DEFAULT_VOICE_SETTINGS.vadEngine).toBe('energy');
    expect(sanitizeVoiceSettings({ vadEngine: 'silero' }).vadEngine).toBe('silero');
    for (const bad of ['webrtc', 3, null, undefined, {}])
      expect(sanitizeVoiceSettings({ vadEngine: bad }).vadEngine).toBe('energy');
    expect(sanitizeVoiceSettings(undefined).vadEngine).toBe('energy');
  });

  it('a store written before the field (version 1) loads as energy and keeps its other values', async () => {
    localStorage.setItem(
      'sr.voice.settings',
      JSON.stringify({ state: { settings: { inputMode: 'ptt', inputVolume: 150 } }, version: 1 }),
    );
    await useVoiceSettings.persist.rehydrate();
    const s = useVoiceSettings.getState().settings;
    expect(s.vadEngine).toBe('energy');
    expect(s.inputMode).toBe('ptt');
    expect(s.inputVolume).toBe(150);
  });

  it('persists the choice at version 2', () => {
    useVoiceSettings.getState().update({ vadEngine: 'silero' });
    const stored = JSON.parse(localStorage.getItem('sr.voice.settings') ?? '{}');
    expect(stored.version).toBe(2);
    expect(stored.state.settings.vadEngine).toBe('silero');
  });
});
