import { describe, expect, it } from 'vitest';
import { computePosition } from './position';

describe('computePosition', () => {
  const vp = { width: 1280, height: 800 };
  const float = { width: 200, height: 560 };

  it('puts a tall bottom-end menu by its trigger: inline-end edges equal, pinned on the block axis (D-870)', () => {
    const r = computePosition({ x: 989, y: 316, width: 28, height: 28 }, float, vp, {
      placement: 'bottom-end',
      offset: 4,
    });
    expect(r.x).toBe(817);
    expect(r.y).toBe(232);
    expect(r.x + float.width).toBe(989 + 28);
  });

  it('is the corner for an anchor box of zeros (why such a box must never be used)', () => {
    const r = computePosition({ x: 0, y: 0, width: 0, height: 0 }, float, vp, {
      placement: 'bottom-end',
      offset: 4,
    });
    expect([r.x, r.y]).toEqual([8, 4]);
  });
});
