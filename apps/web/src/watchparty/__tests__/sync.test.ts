import { describe, expect, it } from 'vitest';
import {
  ClockEstimator,
  DRIFT_SEEK_MS,
  clock,
  expectedPositionMs,
  planCorrection,
  waitingLabel,
} from '../sync';
import type { Playback } from '../types';

const pb = (o: Partial<Playback> = {}): Playback => ({
  version: 1,
  playing: true,
  position_ms: 10_000,
  rate: 1,
  at_ms: 1_000_000,
  server_time_ms: 1_000_000,
  expected_position_ms: 10_000,
  auto_paused: false,
  duration_ms: 100_000,
  live: false,
  ...o,
});

describe('expectedPositionMs', () => {
  it('advances by the server time elapsed times the rate while playing', () => {
    expect(expectedPositionMs(pb(), 1_005_000)).toBe(15_000);
    expect(expectedPositionMs(pb({ rate: 1.5 }), 1_004_000)).toBe(16_000);
  });
  it('holds a paused position', () => {
    expect(expectedPositionMs(pb({ playing: false }), 9_999_999)).toBe(10_000);
  });
  it('never goes past the end or before zero, and ignores a clock that runs backwards', () => {
    expect(expectedPositionMs(pb(), 9_000_000)).toBe(100_000);
    expect(expectedPositionMs(pb(), 900_000)).toBe(10_000);
    expect(expectedPositionMs(pb({ duration_ms: null }), 2_000_000)).toBe(1_010_000);
  });
});

describe('ClockEstimator', () => {
  it('is zero until it has a sample', () => {
    const c = new ClockEstimator();
    expect(c.ready).toBe(false);
    expect(c.offsetMs).toBe(0);
    expect(c.serverNow(500)).toBe(500);
  });
  it('recovers the offset from a symmetric exchange', () => {
    const c = new ClockEstimator();
    // Server is 5 s ahead; request took 40 ms each way.
    c.add({ sentMs: 1000, receivedMs: 1080, serverMs: 1040 + 5000 });
    expect(c.offsetMs).toBe(5000);
    expect(c.rttMs).toBe(80);
    expect(c.serverNow(2000)).toBe(7000);
  });
  it('trusts the low-latency samples over a slow outlier', () => {
    const c = new ClockEstimator();
    c.add({ sentMs: 0, receivedMs: 40, serverMs: 20 + 1000 });
    c.add({ sentMs: 100, receivedMs: 140, serverMs: 120 + 1000 });
    c.add({ sentMs: 200, receivedMs: 240, serverMs: 220 + 1000 });
    // A request that sat in a queue for 2 s: its offset estimate is 1000 ms off.
    c.add({ sentMs: 300, receivedMs: 2300, serverMs: 300 + 1000 });
    expect(c.offsetMs).toBe(1000);
  });
  it('keeps only recent samples', () => {
    const c = new ClockEstimator(3, 3);
    for (let i = 0; i < 3; i++) c.add({ sentMs: 0, receivedMs: 10, serverMs: 5 + 100 });
    for (let i = 0; i < 3; i++) c.add({ sentMs: 0, receivedMs: 10, serverMs: 5 + 900 });
    expect(c.offsetMs).toBe(900);
  });
});

describe('planCorrection (spec section 4)', () => {
  const plan = (drift: number, nudging = false) => planCorrection(drift, nudging, 1, 50_000);
  it('ignores a drift under 100 ms in either direction', () => {
    expect(plan(99)).toEqual({ kind: 'none' });
    expect(plan(-99)).toEqual({ kind: 'none' });
  });
  it('nudges the rate between 100 ms and 2 s: slower when ahead, faster when behind', () => {
    expect(plan(300)).toEqual({ kind: 'nudge', rate: 0.95 });
    expect(plan(-300)).toEqual({ kind: 'nudge', rate: 1.05 });
    expect(plan(1999)).toMatchObject({ kind: 'nudge' });
  });
  it('scales the nudge with the party rate', () => {
    expect(planCorrection(400, false, 2, 0)).toEqual({ kind: 'nudge', rate: 1.9 });
  });
  it('hard seeks above 2 s', () => {
    expect(plan(DRIFT_SEEK_MS + 1)).toEqual({ kind: 'seek', toMs: 50_000 });
    expect(plan(-5000)).toEqual({ kind: 'seek', toMs: 50_000 });
  });
  it('keeps nudging until converged, then settles (hysteresis)', () => {
    expect(plan(200, true)).toMatchObject({ kind: 'nudge' });
    expect(plan(39, true)).toEqual({ kind: 'settle' });
  });
});

describe('labels', () => {
  it('words who the party waits for', () => {
    expect(waitingLabel([])).toBeNull();
    expect(waitingLabel(['Ana'])).toBe('Waiting for Ana');
    expect(waitingLabel(['Ana', 'Bo'])).toBe('Waiting for Ana and Bo');
    expect(waitingLabel(['Ana', 'Bo', 'Cy', 'Di'])).toBe('Waiting for Ana and 3 others');
  });
  it('formats clocks', () => {
    expect(clock(0)).toBe('0:00');
    expect(clock(65_000)).toBe('1:05');
    expect(clock(3_725_000)).toBe('1:02:05');
    expect(clock(-5)).toBe('0:00');
  });
});
