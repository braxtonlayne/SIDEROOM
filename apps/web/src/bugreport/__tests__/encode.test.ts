import { describe, expect, it } from 'vitest';
import { fitWithin, PictureTooLargeError, renderJpeg, type CanvasLike } from '../encode';

describe('fitWithin', () => {
  it('scales the long side down, never up', () => {
    expect(fitWithin(3200, 1600, 1600)).toEqual({ w: 1600, h: 800 });
    expect(fitWithin(1000, 2000, 1600)).toEqual({ w: 800, h: 1600 });
    expect(fitWithin(800, 600, 1600)).toEqual({ w: 800, h: 600 });
    expect(fitWithin(0, 5, 10)).toEqual({ w: 1, h: 1 });
  });
});

function harness(sizes: number[]) {
  const calls: string[] = [];
  const dims: [number, number][] = [];
  let n = 0;
  const factory = (w: number, h: number): CanvasLike => {
    dims.push([w, h]);
    return {
      draw: (_s, dw, dh) => calls.push(`draw ${dw}x${dh}`),
      fillBlack: (r) => calls.push(`fill ${r.x},${r.y},${r.w},${r.h}`),
      toJpeg: async (q) => {
        calls.push(`jpeg ${q}`);
        return new Blob([new Uint8Array(sizes[n++] ?? 1)], { type: 'image/jpeg' });
      },
    };
  };
  return { calls, dims, factory };
}

describe('renderJpeg', () => {
  it('draws, then fills the scaled covers, then encodes at 0.85', async () => {
    const h = harness([1000]);
    const r = await renderJpeg('src', 3200, 1600, [{ x: 320, y: 160, w: 640, h: 320 }], h.factory);
    expect(h.calls).toEqual(['draw 1600x800', 'fill 160,80,320,160', 'jpeg 0.85']);
    expect(r.width).toBe(1600);
    expect(r.blob.size).toBe(1000);
  });
  it('retries at 0.7 and then at 1280 px, repainting the covers each time', async () => {
    const big = 3 * 1024 * 1024;
    const h = harness([big, big, 500]);
    const r = await renderJpeg('src', 3200, 1600, [{ x: 0, y: 0, w: 3200, h: 100 }], h.factory);
    expect(h.dims).toEqual([
      [1600, 800],
      [1600, 800],
      [1280, 640],
    ]);
    expect(h.calls.filter((c) => c.startsWith('jpeg'))).toEqual([
      'jpeg 0.85',
      'jpeg 0.7',
      'jpeg 0.7',
    ]);
    expect(h.calls[h.calls.length - 2]).toBe('fill 0,0,1280,40');
    expect(r.width).toBe(1280);
  });
  it('gives up with an error the dialog can show', async () => {
    const big = 3 * 1024 * 1024;
    await expect(
      renderJpeg('s', 100, 100, [], harness([big, big, big]).factory),
    ).rejects.toBeInstanceOf(PictureTooLargeError);
  });
});
