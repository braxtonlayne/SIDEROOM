import { describe, expect, it } from 'vitest';
import { MAX_SECONDS, planFrames } from '../videoFrames';

describe('GIF frame planning', () => {
  it('spaces frames by the delay and starts at the chosen time', () => {
    const p = planFrames(60, 10, 2, 10);
    expect(p.delayCs).toBe(10);
    expect(p.times).toHaveLength(20);
    expect(p.times[0]).toBeCloseTo(10);
    expect(p.times[19]).toBeCloseTo(11.9);
  });

  it('never plans more than 15 seconds, however long the clip is asked to be', () => {
    const p = planFrames(300, 0, 120, 10);
    expect(p.times.length * p.delayCs * 10).toBeLessThanOrEqual(MAX_SECONDS * 1000);
    expect(p.times).toHaveLength(150);
  });

  it('stops at the end of the video and keeps at least one frame', () => {
    const p = planFrames(5, 4, 10, 10);
    expect(p.times.at(-1)!).toBeLessThan(5);
    expect(p.times.length).toBeLessThanOrEqual(10);
    expect(planFrames(5, 99, 10, 10).times.length).toBeGreaterThanOrEqual(1);
  });

  it('caps the rate at 50 frames per second (a 2 cs delay)', () => {
    expect(planFrames(10, 0, 1, 500).delayCs).toBe(2);
    expect(planFrames(10, 0, 1, 0).delayCs).toBe(100);
  });
});
