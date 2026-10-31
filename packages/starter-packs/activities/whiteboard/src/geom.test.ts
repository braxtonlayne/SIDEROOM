import { describe, expect, it } from 'vitest';
import { bounds, hits, packStroke, pick, simplify } from './geom';
import { LIMITS, type DrawObject } from './log';

const obj = (o: Record<string, unknown>): DrawObject =>
  ({ id: 'a:1', author: 'a', pg: 'home', c: '#000000', w: 4, ...o }) as unknown as DrawObject;

describe('simplify and packStroke', () => {
  it('drops collinear points but keeps corners', () => {
    const out = simplify(
      [
        [0, 0],
        [5, 0],
        [10, 0],
        [10, 10],
      ],
      0.5,
    );
    expect(out).toEqual([
      [0, 0],
      [10, 0],
      [10, 10],
    ]);
  });

  it('a tap becomes a two-point dot', () => {
    expect(packStroke([[5.2, 6.7]])).toEqual([5, 7, 5, 7]);
    expect(packStroke([])).toEqual([]);
  });

  it('keeps long scribbles under the point limit', () => {
    const raw: Array<[number, number]> = [];
    for (let i = 0; i < 20000; i++) raw.push([i % 1500, Math.round(450 + 400 * Math.sin(i * 0.9))]);
    const out = packStroke(raw);
    expect(out.length).toBeLessThanOrEqual(LIMITS.maxPoints * 2);
    expect(out.length % 2).toBe(0);
  });
});

describe('hit testing', () => {
  const line = obj({ k: 'stroke', hl: false, pts: [0, 0, 100, 0] });
  it('touches a stroke within the eraser radius', () => {
    expect(hits(line, [50, 6], 5)).toBe(true);
    expect(hits(line, [50, 30], 5)).toBe(false);
  });
  it('rectangle outline vs fill', () => {
    const r = obj({ k: 'shape', s: 'rect', fill: false, a: [0, 0], b: [100, 100] });
    expect(hits(r, [50, 50], 4)).toBe(false);
    expect(hits(r, [0, 50], 4)).toBe(true);
    expect(hits({ ...r, fill: true } as DrawObject, [50, 50], 4)).toBe(true);
  });
  it('ellipse outline', () => {
    const e = obj({ k: 'shape', s: 'ellipse', fill: false, a: [0, 0], b: [100, 60] });
    expect(hits(e, [50, 0], 4)).toBe(true);
    expect(hits(e, [50, 30], 4)).toBe(false);
  });
  it('pick returns the topmost object of the page', () => {
    const below = obj({ id: 'a:1', k: 'stroke', hl: false, pts: [0, 0, 100, 0] });
    const above = obj({ id: 'a:2', k: 'stroke', hl: false, pts: [0, 0, 100, 0] });
    const other = obj({ id: 'a:3', pg: 'p2', k: 'stroke', hl: false, pts: [0, 0, 100, 0] });
    expect(pick([below, above, other], 'home', [10, 0], 3)?.id).toBe('a:2');
    expect(pick([below], 'p2', [10, 0], 3)).toBeNull();
  });
  it('bounds of an image and text', () => {
    expect(bounds(obj({ k: 'image', src: 'x', at: [10, 20], size: [30, 40] }))).toEqual({
      x0: 10,
      y0: 20,
      x1: 40,
      y1: 60,
    });
    const t = bounds(obj({ k: 'text', size: 20, at: [0, 0], text: 'ab\nabcd' }));
    expect(t.y1).toBe(50);
    expect(t.x1).toBeGreaterThan(40);
  });
});
