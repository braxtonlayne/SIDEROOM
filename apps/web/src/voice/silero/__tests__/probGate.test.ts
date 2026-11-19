import { describe, expect, it } from 'vitest';
import { HANG_MS } from '../../vad';
import { CLOSE_PROB, OPEN_PROB, ProbGate } from '../probGate';

describe('ProbGate (D-1050)', () => {
  it('opens at 0.5 and not under it', () => {
    const g = new ProbGate();
    expect(g.process(OPEN_PROB - 0.01, 0)).toBe(false);
    expect(g.process(OPEN_PROB, 32)).toBe(true);
  });

  it('closes only after the probability stayed under 0.35 for the hang time', () => {
    const g = new ProbGate();
    g.process(0.9, 0);
    expect(g.process(0.1, 32)).toBe(true);
    expect(g.process(0.1, HANG_MS)).toBe(true);
    expect(g.process(0.1, HANG_MS + 1)).toBe(false);
    expect(g.process(0.1, HANG_MS + 40)).toBe(false);
  });

  it('a single spike opens it, then the hang closes it', () => {
    const g = new ProbGate();
    for (let t = 0; t < 1000; t += 32) g.process(0.05, t);
    expect(g.isOpen).toBe(false);
    expect(g.process(0.97, 1000)).toBe(true);
    expect(g.process(0.05, 1032)).toBe(true);
    expect(g.process(0.05, 1000 + HANG_MS + 1)).toBe(false);
  });

  it('a flutter around 0.4 neither opens a closed gate nor closes an open one', () => {
    const closed = new ProbGate();
    for (let t = 0; t < 3000; t += 32) expect(closed.process(t % 64 ? 0.4 : 0.45, t)).toBe(false);
    const open = new ProbGate();
    open.process(0.8, 0);
    for (let t = 32; t < 3000; t += 32) expect(open.process(t % 64 ? 0.36 : 0.45, t)).toBe(true);
    expect(CLOSE_PROB).toBeLessThan(0.4);
  });

  it('a dip under 0.35 shorter than the hang does not close it, and resets on speech', () => {
    const g = new ProbGate();
    g.process(0.9, 0);
    expect(g.process(0.1, 200)).toBe(true);
    g.process(0.9, 220);
    expect(g.process(0.1, 220 + HANG_MS)).toBe(true);
    expect(g.process(0.1, 220 + HANG_MS + 1)).toBe(false);
  });

  it('reset closes it', () => {
    const g = new ProbGate();
    g.process(0.9, 0);
    g.reset();
    expect(g.isOpen).toBe(false);
    expect(g.process(0.1, 10)).toBe(false);
  });
});
