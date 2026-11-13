import { FALLBACK_PICTURE_SIDE, MAX_PICTURE_BYTES, MAX_PICTURE_SIDE } from './contract';
import type { Rect, Size } from './covers';

/** The size that fits `w` x `h` within `max` on the long side (never enlarged), whole pixels, at least 1. */
export function fitWithin(w: number, h: number, max: number): Size {
  if (!(w > 0 && h > 0 && max > 0)) return { w: 1, h: 1 };
  const k = Math.min(1, max / Math.max(w, h));
  return { w: Math.max(1, Math.round(w * k)), h: Math.max(1, Math.round(h * k)) };
}

/** The part of a canvas `renderJpeg` uses, so a test can stand in for it. */
export interface CanvasLike {
  draw(source: unknown, w: number, h: number): void;
  fillBlack(r: Rect): void;
  toJpeg(quality: number): Promise<Blob | null>;
}

export class PictureTooLargeError extends Error {
  constructor() {
    super('The picture is too large to send.');
    this.name = 'PictureTooLargeError';
  }
}

export interface RenderedJpeg {
  blob: Blob;
  width: number;
  height: number;
}

/**
 * Draw `source` (`sw` x `sh` pixels) scaled to at most 1600 px on the long side, paint each cover black AFTER the draw
 * (covers are in source pixels and are scaled with the picture), encode JPEG at 0.85; over 2 MiB: 0.7, then 1280 px.
 * Re-encoding is also what strips an attached file's metadata. Throws `PictureTooLargeError` when nothing fits.
 */
export async function renderJpeg(
  source: unknown,
  sw: number,
  sh: number,
  covers: readonly Rect[],
  canvasFactory: (w: number, h: number) => CanvasLike,
): Promise<RenderedJpeg> {
  const ladder: [number, number][] = [
    [MAX_PICTURE_SIDE, 0.85],
    [MAX_PICTURE_SIDE, 0.7],
    [FALLBACK_PICTURE_SIDE, 0.7],
  ];
  for (const [side, quality] of ladder) {
    const size = fitWithin(sw, sh, side);
    const canvas = canvasFactory(size.w, size.h);
    canvas.draw(source, size.w, size.h);
    const kx = size.w / sw;
    const ky = size.h / sh;
    for (const c of covers) {
      const x0 = Math.max(0, Math.floor(c.x * kx));
      const y0 = Math.max(0, Math.floor(c.y * ky));
      const x1 = Math.min(size.w, Math.ceil((c.x + c.w) * kx));
      const y1 = Math.min(size.h, Math.ceil((c.y + c.h) * ky));
      if (x1 > x0 && y1 > y0) canvas.fillBlack({ x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
    }
    const blob = await canvas.toJpeg(quality);
    if (blob && blob.size > 0 && blob.size <= MAX_PICTURE_BYTES) {
      return { blob, width: size.w, height: size.h };
    }
  }
  throw new PictureTooLargeError();
}

/** The real canvas behind `CanvasLike` (the dialog's factory). */
export function domCanvas(w: number, h: number): CanvasLike {
  const el = document.createElement('canvas');
  el.width = w;
  el.height = h;
  const ctx = el.getContext('2d');
  if (!ctx) throw new Error('No 2d canvas');
  return {
    draw: (source, dw, dh) => ctx.drawImage(source as CanvasImageSource, 0, 0, dw, dh),
    fillBlack: (r) => {
      ctx.fillStyle = '#000';
      ctx.fillRect(r.x, r.y, r.w, r.h);
    },
    toJpeg: (q) => new Promise((res) => el.toBlob(res, 'image/jpeg', q)),
  };
}
