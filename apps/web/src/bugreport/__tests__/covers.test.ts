import { describe, expect, it } from 'vitest';
import { collectPrivateRects, coversFromRects, frameMatchesViewport } from '../covers';

describe('coversFromRects', () => {
  const vp = { w: 1000, h: 500 };
  it('scales at DPR 1, 2 and 1.5', () => {
    const r = [{ x: 100, y: 50, w: 200, h: 100 }];
    expect(coversFromRects(r, vp, { w: 1000, h: 500 })).toEqual([
      { x: 100, y: 50, w: 200, h: 100 },
    ]);
    expect(coversFromRects(r, vp, { w: 2000, h: 1000 })).toEqual([
      { x: 200, y: 100, w: 400, h: 200 },
    ]);
    expect(coversFromRects(r, vp, { w: 1500, h: 750 })).toEqual([
      { x: 150, y: 75, w: 300, h: 150 },
    ]);
  });
  it('clips to the frame and drops empty or outside rectangles', () => {
    const out = coversFromRects(
      [
        { x: 900, y: 450, w: 300, h: 300 },
        { x: 10, y: 10, w: 0, h: 5 },
        { x: 2000, y: 0, w: 5, h: 5 },
        { x: -50, y: -50, w: 100, h: 100 },
      ],
      vp,
      { w: 1000, h: 500 },
    );
    expect(out).toEqual([
      { x: 900, y: 450, w: 100, h: 50 },
      { x: 0, y: 0, w: 50, h: 50 },
    ]);
  });
  it('returns nothing for a zero size', () => {
    expect(coversFromRects([{ x: 0, y: 0, w: 1, h: 1 }], { w: 0, h: 0 }, { w: 10, h: 10 })).toEqual(
      [],
    );
  });
});

describe('frameMatchesViewport', () => {
  it('accepts the same proportions at any scale', () => {
    expect(frameMatchesViewport({ w: 1280, h: 800 }, { w: 1280, h: 800 })).toBe(true);
    expect(frameMatchesViewport({ w: 1280, h: 800 }, { w: 2560, h: 1600 })).toBe(true);
    expect(frameMatchesViewport({ w: 1280, h: 800 }, { w: 1920, h: 1200 })).toBe(true);
    expect(frameMatchesViewport({ w: 1280, h: 800 }, { w: 1280, h: 806 })).toBe(true);
  });
  it('rejects a shared window or screen with other proportions', () => {
    expect(frameMatchesViewport({ w: 1280, h: 800 }, { w: 1920, h: 1080 })).toBe(false);
    expect(frameMatchesViewport({ w: 1280, h: 800 }, { w: 800, h: 800 })).toBe(false);
    expect(frameMatchesViewport({ w: 1280, h: 800 }, { w: 0, h: 0 })).toBe(false);
  });
});

describe('collectPrivateRects', () => {
  it('returns the visible private elements only', () => {
    document.body.innerHTML =
      '<div id="a" data-sr-private></div><div id="b" data-sr-private></div><div id="c"></div>';
    const box = (w: number, h: number) => () =>
      ({ left: 5, top: 6, width: w, height: h, right: 5 + w, bottom: 6 + h }) as DOMRect;
    document.getElementById('a')!.getBoundingClientRect = box(100, 40);
    document.getElementById('b')!.getBoundingClientRect = box(0, 0);
    document.getElementById('c')!.getBoundingClientRect = box(50, 50);
    expect(collectPrivateRects(document)).toEqual([{ x: 5, y: 6, w: 100, h: 40 }]);
  });
});
