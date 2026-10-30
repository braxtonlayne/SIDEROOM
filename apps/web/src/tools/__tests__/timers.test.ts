import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  DEFAULT_POMODORO,
  advance,
  clampCountdown,
  elapsedMs,
  formatClock,
  fromWire,
  initialState,
  nextSegment,
  parseClock,
  parseWire,
  pause,
  remainingMs,
  reset,
  setCountdown,
  skip,
  start,
  statusLine,
  toWire,
} from '../timers/timerCore';
import { createTimerStore } from '../timers/timerStore';
import { applyVoiceTimerEvent, resetVoiceTimers, useVoiceTimers } from '../timers/voiceTimer';

const MIN = 60_000;
const memory = () => {
  const m = new Map<string, string>();
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
  };
};

describe('timer core', () => {
  it('a countdown runs, pauses without drifting and finishes', () => {
    let s = initialState('countdown', DEFAULT_POMODORO, 10_000);
    expect(remainingMs(s, 0)).toBe(10_000);
    s = start(s, 1000);
    expect(remainingMs(s, 4000)).toBe(7000);
    s = pause(s, 4000);
    expect(s.phase).toBe('paused');
    // Time passing while paused changes nothing.
    expect(remainingMs(s, 99_000)).toBe(7000);
    s = start(s, 100_000);
    expect(advance(s, 105_000).finished).toEqual([]);
    const done = advance(s, 107_000);
    expect(done.finished).toEqual(['focus']);
    expect(done.state.phase).toBe('done');
    expect(remainingMs(done.state, 999_999)).toBe(0);
    // Starting a finished countdown starts it over.
    const again = start(done.state, 200_000);
    expect(remainingMs(again, 200_000)).toBe(10_000);
  });

  it('a stopwatch counts up through pauses', () => {
    let s = initialState('stopwatch');
    expect(remainingMs(s, 0)).toBeNull();
    s = start(s, 0);
    s = pause(s, 2500);
    s = start(s, 10_000);
    expect(elapsedMs(s, 11_500)).toBe(4000);
    expect(advance(s, 10 ** 9).finished).toEqual([]);
  });

  it('a pomodoro goes focus, short, focus, ... then a long break after four', () => {
    const cfg = DEFAULT_POMODORO;
    let s = start(initialState('pomodoro', cfg), 0);
    const seen: string[] = [];
    let t = 0;
    for (let i = 0; i < 8; i++) {
      seen.push(s.segment);
      t += (s.durationMs ?? 0) + 1;
      s = advance(s, t, cfg).state;
    }
    expect(seen).toEqual(['focus', 'short', 'focus', 'short', 'focus', 'short', 'focus', 'long']);
    expect(nextSegment('focus', 4, cfg)).toBe('long');
    expect(nextSegment('focus', 3, cfg)).toBe('short');
    expect(nextSegment('long', 4, cfg)).toBe('focus');
  });

  it('a tab that slept through several segments wakes up in the right place', () => {
    const cfg = DEFAULT_POMODORO;
    const s = start(initialState('pomodoro', cfg), 0);
    // 25 + 5 + 25 minutes and 2 minutes into the next break.
    const r = advance(s, 57 * MIN, cfg);
    expect(r.finished).toEqual(['focus', 'short', 'focus']);
    expect(r.state.segment).toBe('short');
    expect(remainingMs(r.state, 57 * MIN)).toBe(3 * MIN);
    expect(r.state.completed).toBe(2);
  });

  it('without auto-continue a pomodoro waits at the next segment', () => {
    const cfg = { ...DEFAULT_POMODORO, autoContinue: false };
    const r = advance(start(initialState('pomodoro', cfg), 0), 26 * MIN, cfg);
    expect(r.state.phase).toBe('paused');
    expect(r.state.segment).toBe('short');
    expect(r.finished).toEqual(['focus']);
  });

  it('skip moves on, keeping the running state', () => {
    const cfg = DEFAULT_POMODORO;
    const running = skip(start(initialState('pomodoro', cfg), 0), 5000, cfg);
    expect(running.segment).toBe('short');
    expect(running.phase).toBe('running');
    expect(running.completed).toBe(1);
    expect(skip(initialState('stopwatch'), 0).kind).toBe('stopwatch');
  });

  it('reset and setCountdown', () => {
    const s = setCountdown(initialState('countdown'), 90_000);
    expect(s.durationMs).toBe(90_000);
    expect(reset(start(s, 0)).durationMs).toBe(90_000);
    // The length cannot change while running.
    expect(setCountdown(start(s, 0), 5000).durationMs).toBe(90_000);
    expect(clampCountdown(0)).toBe(1000);
    expect(clampCountdown(10 ** 12)).toBeLessThan(10 ** 9);
    expect(clampCountdown(Number.NaN)).toBe(1000);
  });

  it('formats and parses clocks', () => {
    expect(formatClock(25 * MIN)).toBe('25:00');
    expect(formatClock(3_723_000)).toBe('1:02:03');
    expect(formatClock(61_500, true)).toBe('01:01.5');
    expect(formatClock(-5)).toBe('00:00');
    expect(parseClock('5:30')).toBe(330_000);
    expect(parseClock('1:02:03')).toBe(3_723_000);
    expect(parseClock('90s')).toBe(90_000);
    expect(parseClock('1h30m')).toBe(5_400_000);
    expect(parseClock('10m')).toBe(600_000);
    expect(parseClock('10')).toBe(600_000);
    for (const bad of ['', 'abc', '0', '0:00', '-5', '1:2:3:4']) expect(parseClock(bad)).toBeNull();
  });

  it('the wire form round-trips and refuses nonsense', () => {
    const s = start(initialState('pomodoro'), 1234);
    const w = toWire(s)!;
    expect(fromWire(parseWire(JSON.parse(JSON.stringify(w)))!)).toEqual(s);
    expect(toWire(initialState('pomodoro'))).toBeNull();
    const p = pause(s, 5000);
    expect(toWire(p)!.startedMs).toBeNull();
    for (const bad of [
      null,
      'x',
      {},
      { ...w, kind: 'alarm' },
      { ...w, phase: 'idle' },
      { ...w, accumulatedMs: -1 },
      { ...w, startedMs: null },
      { ...w, durationMs: null },
    ]) {
      expect(parseWire(bad)).toBeNull();
    }
  });

  it('the status line names the segment and the time', () => {
    const labels = {
      focus: 'Focus',
      short: 'Short break',
      long: 'Long break',
      countdown: 'Countdown',
      stopwatch: 'Stopwatch',
    };
    const s = start(initialState('pomodoro'), 0);
    expect(statusLine(s, 60_000, labels)).toBe('Focus 24:00');
    expect(statusLine(pause(s, 60_000), 0, labels)).toBe('Focus 24:00 (paused)');
    expect(statusLine(start(initialState('stopwatch'), 0), 65_000, labels)).toBe('Stopwatch 01:05');
  });
});

describe('timer store', () => {
  afterEach(() => vi.useRealTimers());

  it('persists and restores across a reload, catching up on the way', () => {
    const storage = memory();
    let now = 1_000_000;
    const a = createTimerStore('countdown', { now: () => now, storage });
    a.setCountdownMs(60_000);
    a.start();
    now += 10_000;
    // A new page load (a new store over the same storage) sees a timer with 50 s left.
    const b = createTimerStore('countdown', { now: () => now, storage });
    expect(b.getState().phase).toBe('running');
    expect(remainingMs(b.getState(), now)).toBe(50_000);
    // And if the page was closed past the end, it has finished.
    now += 60_000;
    const c = createTimerStore('countdown', { now: () => now, storage });
    expect(c.getState().phase).toBe('done');
  });

  it('tells listeners when segments end, once', () => {
    let now = 0;
    const store = createTimerStore('pomodoro', { now: () => now, storage: null });
    const finished: string[][] = [];
    store.onFinish((f) => finished.push(f));
    store.start();
    now = 25 * MIN + 1;
    store.settle();
    store.settle();
    expect(finished).toEqual([['focus']]);
    expect(store.getState().segment).toBe('short');
  });

  it('a corrupt saved state starts the timer at rest', () => {
    const storage = { getItem: () => '{not json', setItem: () => undefined };
    expect(createTimerStore('stopwatch', { storage }).getState().phase).toBe('idle');
    const wrong = {
      getItem: () => JSON.stringify({ state: { kind: 'pomodoro' } }),
      setItem: () => undefined,
    };
    expect(createTimerStore('stopwatch', { storage: wrong }).getState().phase).toBe('idle');
  });

  it('works when storage throws', () => {
    const storage = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
    };
    const store = createTimerStore('stopwatch', { storage });
    expect(() => store.start()).not.toThrow();
    expect(store.getState().phase).toBe('running');
  });

  it('new pomodoro lengths apply at rest only', () => {
    const store = createTimerStore('pomodoro', { storage: null, now: () => 0 });
    store.setConfig({ focusMs: 10 * MIN });
    expect(store.getState().durationMs).toBe(10 * MIN);
    store.start();
    store.setConfig({ focusMs: 50 * MIN });
    expect(store.getState().durationMs).toBe(10 * MIN);
  });
});

describe('timers shared with a voice channel', () => {
  afterEach(() => resetVoiceTimers());
  const wire = toWire(start(initialState('pomodoro'), 5000))!;

  it('follows VOICE_TIMER_UPDATE with the server clock', () => {
    applyVoiceTimerEvent(
      'VOICE_TIMER_UPDATE',
      { id: '7', guild_id: '1', user_id: '20', server_ms: 10_000, timer: wire },
      4000,
    );
    const got = useVoiceTimers.getState().byChannel['7']!;
    expect(got.skewMs).toBe(6000);
    expect(got.userId).toBe('20');
    expect(got.timer.kind).toBe('pomodoro');
    applyVoiceTimerEvent('VOICE_TIMER_UPDATE', { id: '7', timer: null });
    expect(useVoiceTimers.getState().byChannel['7']).toBeUndefined();
  });

  it('the timer ends with the status line', () => {
    applyVoiceTimerEvent('VOICE_TIMER_UPDATE', { id: '7', server_ms: 1, timer: wire });
    applyVoiceTimerEvent('VOICE_CHANNEL_STATUS_UPDATE', { id: '7', status: 'Focus 24:00' });
    expect(useVoiceTimers.getState().byChannel['7']).toBeDefined();
    applyVoiceTimerEvent('VOICE_CHANNEL_STATUS_UPDATE', { id: '8', status: null });
    expect(useVoiceTimers.getState().byChannel['7']).toBeDefined();
    applyVoiceTimerEvent('VOICE_CHANNEL_STATUS_UPDATE', { id: '7', status: null });
    expect(useVoiceTimers.getState().byChannel['7']).toBeUndefined();
  });

  it('ignores other events and malformed ones', () => {
    applyVoiceTimerEvent('MESSAGE_CREATE', { id: '7', timer: wire });
    applyVoiceTimerEvent('VOICE_TIMER_UPDATE', { timer: wire });
    applyVoiceTimerEvent('VOICE_TIMER_UPDATE', { id: '7', timer: { kind: 'nope' } });
    expect(useVoiceTimers.getState().byChannel).toEqual({});
  });
});
