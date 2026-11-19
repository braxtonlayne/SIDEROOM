import { describe, expect, it } from 'vitest';
import { HANG_MS } from '../../vad';
import { CLOSE_PROB, OPEN_PROB, ProbGate } from '../probGate';

/** Two frames in a row at `p`, 32 ms apart, the second at `t`. */
const open = (g: ProbGate, t: number, p = 0.9) => {
  g.process(p, t - 32);
  return g.process(p, t);
};

describe('ProbGate (D-1050)', () => {
  it('opens on the second of two consecutive frames at 0.5 or more, not on the first', () => {
    const g = new ProbGate();
    expect(g.process(OPEN_PROB, 0)).toBe(false);
    expect(g.process(OPEN_PROB, 32)).toBe(true);
    const h = new ProbGate();
    expect(h.process(OPEN_PROB - 0.01, 0)).toBe(false);
    expect(h.process(OPEN_PROB - 0.01, 32)).toBe(false);
  });

  it('a single frame at 0.9 between low frames never opens', () => {
    const g = new ProbGate();
    for (let t = 0; t < 3000; t += 64) {
      g.process(0.05, t);
      expect(g.process(0.9, t + 32)).toBe(false);
    }
    expect(g.process(0.2, 3040)).toBe(false);
  });

  it('a frame between the thresholds breaks the streak', () => {
    const g = new ProbGate();
    g.process(0.9, 0);
    g.process(0.4, 32);
    expect(g.process(0.9, 64)).toBe(false);
    expect(g.process(0.9, 96)).toBe(true);
  });

  it('closes only after the probability stayed under 0.35 for the hang time', () => {
    const g = new ProbGate();
    open(g, 32);
    expect(g.process(0.1, 64)).toBe(true);
    expect(g.process(0.1, 32 + HANG_MS)).toBe(true);
    expect(g.process(0.1, 32 + HANG_MS + 1)).toBe(false);
    expect(g.process(0.1, HANG_MS + 100)).toBe(false);
  });

  it('once open, a single frame at 0.5 or more keeps it open and restarts the hang', () => {
    const g = new ProbGate();
    open(g, 32);
    expect(g.process(0.1, 200)).toBe(true);
    g.process(0.9, 220);
    expect(g.process(0.1, 220 + HANG_MS)).toBe(true);
    expect(g.process(0.1, 220 + HANG_MS + 1)).toBe(false);
  });

  it('a flutter around 0.4 neither opens a closed gate nor closes an open one', () => {
    const closed = new ProbGate();
    for (let t = 0; t < 3000; t += 32) expect(closed.process(t % 64 ? 0.4 : 0.45, t)).toBe(false);
    const opened = new ProbGate();
    open(opened, 32);
    for (let t = 64; t < 3000; t += 32) expect(opened.process(t % 64 ? 0.36 : 0.45, t)).toBe(true);
    expect(CLOSE_PROB).toBeLessThan(0.4);
  });

  it('after a close, one frame is again not enough', () => {
    const g = new ProbGate();
    open(g, 32);
    g.process(0.05, 32 + HANG_MS + 1);
    expect(g.isOpen).toBe(false);
    expect(g.process(0.9, 600)).toBe(false);
    expect(g.process(0.9, 632)).toBe(true);
  });

  it('reset closes it and forgets a half-built streak', () => {
    const g = new ProbGate();
    open(g, 32);
    g.reset();
    expect(g.isOpen).toBe(false);
    g.process(0.9, 100);
    g.reset();
    expect(g.process(0.9, 132)).toBe(false);
  });
});
