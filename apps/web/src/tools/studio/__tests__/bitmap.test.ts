import { describe, expect, it } from 'vitest';
import {
  clampRegion,
  createBitmap,
  crop,
  fitWithin,
  flatten,
  flip,
  MIN_BLOCK,
  pad,
  padToAspect,
  pixelate,
  redact,
  resize,
  rotate90,
  type Bitmap,
} from '../bitmap';

/** A w x h fixture whose pixel (x, y) is [x, y, 7, 255]: every pixel is unique and findable. */
function grid(w: number, h: number): Bitmap {
  const b = createBitmap(w, h);
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) b.data.set([x, y, 7, 255], (y * w + x) * 4);
  return b;
}
const px = (b: Bitmap, x: number, y: number) =>
  Array.from(b.data.subarray((y * b.width + x) * 4, (y * b.width + x) * 4 + 4));

describe('studio bitmap operations', () => {
  it('crops to a region and clamps one that sticks out', () => {
    const c = crop(grid(10, 8), { x: 2, y: 3, width: 4, height: 2 });
    expect([c.width, c.height]).toEqual([4, 2]);
    expect(px(c, 0, 0)).toEqual([2, 3, 7, 255]);
    expect(px(c, 3, 1)).toEqual([5, 4, 7, 255]);
    const edge = crop(grid(10, 8), { x: 8, y: 6, width: 10, height: 10 });
    expect([edge.width, edge.height]).toEqual([2, 2]);
    expect(clampRegion(grid(4, 4), { x: 9, y: 9, width: 3, height: 3 })).toBeNull();
    expect(clampRegion(grid(10, 10), { x: 5, y: 5, width: -3, height: -3 })).toEqual({
      x: 2,
      y: 2,
      width: 3,
      height: 3,
    });
  });

  it('resizes up and down, keeping a flat colour flat', () => {
    const flat = createBitmap(40, 20, [10, 200, 30, 255]);
    const small = resize(flat, 10, 5);
    expect([small.width, small.height]).toEqual([10, 5]);
    expect(px(small, 4, 2)).toEqual([10, 200, 30, 255]);
    const big = resize(flat, 80, 40);
    expect(px(big, 79, 39)).toEqual([10, 200, 30, 255]);
    const g = resize(grid(4, 4), 8, 8);
    expect(px(g, 0, 0)[0]).toBe(0);
    expect(px(g, 7, 7)[0]).toBe(3); // the far corner keeps the far colour
  });

  it('fits a large bitmap within the maximum edge and leaves a small one alone', () => {
    const b = createBitmap(400, 200);
    const fitted = fitWithin(b, 100);
    expect([fitted.width, fitted.height]).toEqual([100, 50]);
    expect(fitWithin(b, 1000)).toBe(b);
  });

  it('pads with a colour and keeps the image where it was', () => {
    const p = pad(grid(3, 2), { top: 1, right: 2, bottom: 3, left: 4 }, [9, 9, 9, 255]);
    expect([p.width, p.height]).toEqual([9, 6]);
    expect(px(p, 0, 0)).toEqual([9, 9, 9, 255]);
    expect(px(p, 4, 1)).toEqual([0, 0, 7, 255]);
    expect(px(p, 6, 2)).toEqual([2, 1, 7, 255]);
  });

  it('pads to a square', () => {
    const s = padToAspect(grid(6, 2), 1);
    expect([s.width, s.height]).toEqual([6, 6]);
    expect(px(s, 0, 2)).toEqual([0, 0, 7, 255]);
    const tall = padToAspect(grid(2, 6), 1);
    expect([tall.width, tall.height]).toEqual([6, 6]);
  });

  it('rotates a quarter turn and flips', () => {
    const r = rotate90(grid(3, 2));
    expect([r.width, r.height]).toEqual([2, 3]);
    expect(px(r, 1, 0)).toEqual([0, 0, 7, 255]); // top-left goes to top-right
    expect(px(rotate90(grid(3, 2), false), 0, 2)).toEqual([0, 0, 7, 255]);
    expect(px(flip(grid(3, 2), true), 0, 0)).toEqual([2, 0, 7, 255]);
    expect(px(flip(grid(3, 2), false), 0, 0)).toEqual([0, 1, 7, 255]);
  });

  it('pixelates a region to block averages and nothing outside it', () => {
    const b = grid(32, 32);
    const r = pixelate(b, { x: 8, y: 8, width: 16, height: 16 }, 8);
    expect(r).toEqual({ x: 8, y: 8, width: 16, height: 16 });
    expect(px(b, 8, 8)).toEqual(px(b, 15, 15)); // one block, one colour
    expect(px(b, 16, 8)).toEqual(px(b, 23, 15));
    expect(px(b, 8, 8)).not.toEqual(px(b, 16, 8));
    expect(px(b, 7, 8)).toEqual([7, 8, 7, 255]); // outside is untouched
    expect(px(b, 24, 24)).toEqual([24, 24, 7, 255]);
  });

  it('raises a too-fine block so a large region is still destroyed', () => {
    const b = grid(64, 64);
    pixelate(b, { x: 0, y: 0, width: 60, height: 60 }, 1); // block 1 asked: raised to 10
    expect(MIN_BLOCK).toBeGreaterThan(1);
    expect(px(b, 0, 0)).toEqual(px(b, 9, 9));
  });

  it('redacts a region with an opaque colour and returns null for nothing', () => {
    const b = grid(10, 10);
    expect(redact(b, { x: 2, y: 2, width: 3, height: 3 })).toEqual({
      x: 2,
      y: 2,
      width: 3,
      height: 3,
    });
    expect(px(b, 3, 3)).toEqual([0, 0, 0, 255]);
    expect(px(b, 5, 5)).toEqual([5, 5, 7, 255]);
    expect(redact(b, { x: 50, y: 50, width: 3, height: 3 })).toBeNull();
    expect(pixelate(b, { x: 50, y: 50, width: 3, height: 3 }, 5)).toBeNull();
  });

  it('flattens transparency onto a background', () => {
    const b = createBitmap(2, 1);
    b.data.set([255, 0, 0, 0, 0, 0, 255, 128]);
    const f = flatten(b);
    expect(px(f, 0, 0)).toEqual([255, 255, 255, 255]);
    expect(px(f, 1, 0)[3]).toBe(255);
    expect(px(f, 1, 0)[0]).toBeGreaterThan(120);
  });
});
