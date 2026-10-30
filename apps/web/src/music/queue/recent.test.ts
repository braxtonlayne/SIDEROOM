import { describe, expect, it, vi } from 'vitest';
import { PLAYED_AFTER_MS, startRecentTracker } from './recent';
import type { PlayerPort, PortState } from './port';

function rig() {
  let state: PortState = {
    queueIds: ['a', 'b', 'c'],
    index: -1,
    eq: [],
    eqEnabled: true,
    crossfadeSec: 0,
  };
  const listeners = new Set<() => void>();
  const port: PlayerPort = {
    play: () => undefined,
    enqueue: () => undefined,
    setEq: () => undefined,
    setEqEnabled: () => undefined,
    setCrossfade: () => undefined,
    getState: () => state,
    subscribe: (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
  };
  const timers: { fn: () => void; ms: number; live: boolean }[] = [];
  const record = vi.fn(async () => undefined);
  const stop = startRecentTracker({
    record,
    port: () => port,
    setTimeout: (fn, ms) => {
      timers.push({ fn, ms, live: true });
      return timers.length - 1;
    },
    clearTimeout: (h) => {
      timers[h as number]!.live = false;
    },
  });
  const go = (index: number) => {
    state = { ...state, index };
    listeners.forEach((l) => l());
  };
  const fireLive = () => timers.filter((t) => t.live).forEach((t) => ((t.live = false), t.fn()));
  return { go, record, fireLive, stop, timers, listeners };
}

describe('recently played tracker', () => {
  it('records a track only after it has been current for a few seconds', () => {
    const r = rig();
    r.go(0);
    expect(r.timers.at(-1)!.ms).toBe(PLAYED_AFTER_MS);
    expect(r.record).not.toHaveBeenCalled();
    r.fireLive();
    expect(r.record).toHaveBeenCalledWith('a');
  });

  it('a skipped track is not recorded', () => {
    const r = rig();
    r.go(0);
    r.go(1); // skipped before the timer
    r.fireLive();
    expect(r.record.mock.calls).toEqual([['b']]);
  });

  it('does nothing for an empty queue and ignores repeated notifications for the same track', () => {
    const r = rig();
    r.go(-1);
    expect(r.timers).toHaveLength(0);
    r.go(2);
    r.go(2);
    expect(r.timers.filter((t) => t.live)).toHaveLength(1);
  });

  it('swallows a failed record and stops cleanly', async () => {
    const r = rig();
    r.record.mockRejectedValueOnce(new Error('404'));
    r.go(0);
    r.fireLive();
    await Promise.resolve();
    r.stop();
    expect(r.listeners.size).toBe(0);
  });
});
