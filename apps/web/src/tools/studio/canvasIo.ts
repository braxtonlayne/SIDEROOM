/**
 * UTL-IMG-001: the thin canvas layer of the studio: decode a file to a {@link Bitmap}, paint one on a canvas,
 * paint captions into one, encode one as PNG, JPEG or WebP. Everything stays in the page: no network call is
 * made here, and an exported file is encoded from the working bitmap only (a canvas re-encode also drops EXIF
 * and any other metadata of the source file).
 */
import { MAX_EDGE, flatten, type Bitmap } from './bitmap';
import { drawCaptions, type CaptionOptions } from './caption';

export type ExportFormat = 'png' | 'jpeg' | 'webp';

export const FORMAT_MIME: Record<ExportFormat, string> = {
  png: 'image/png',
  jpeg: 'image/jpeg',
  webp: 'image/webp',
};
export const FORMAT_EXT: Record<ExportFormat, string> = { png: 'png', jpeg: 'jpg', webp: 'webp' };

type Canvas2D = HTMLCanvasElement | OffscreenCanvas;

function makeCanvas(width: number, height: number): Canvas2D {
  if (typeof document !== 'undefined') {
    const c = document.createElement('canvas');
    c.width = width;
    c.height = height;
    return c;
  }
  return new OffscreenCanvas(width, height);
}

function context(c: Canvas2D) {
  const ctx = c.getContext('2d', { willReadFrequently: true } as CanvasRenderingContext2DSettings) as
    | CanvasRenderingContext2D
    | OffscreenCanvasRenderingContext2D
    | null;
  if (!ctx) throw new Error('canvas-unavailable');
  return ctx;
}

/** Read the pixels of any drawable (image, video frame, canvas) into a bitmap no larger than MAX_EDGE. */
export function drawableToBitmap(
  src: CanvasImageSource,
  width: number,
  height: number,
  max = MAX_EDGE,
): Bitmap {
  const scale = Math.min(1, max / Math.max(width, height));
  const w = Math.max(1, Math.round(width * scale));
  const h = Math.max(1, Math.round(height * scale));
  const c = makeCanvas(w, h);
  const ctx = context(c);
  ctx.drawImage(src, 0, 0, w, h);
  const id = ctx.getImageData(0, 0, w, h);
  return { width: w, height: h, data: id.data };
}

/** Decode an image file or blob. Rejects with `not-an-image` when the browser cannot read it. */
export async function decodeImage(blob: Blob): Promise<Bitmap> {
  if (typeof createImageBitmap === 'function') {
    try {
      const bmp = await createImageBitmap(blob);
      const out = drawableToBitmap(bmp, bmp.width, bmp.height);
      bmp.close?.();
      return out;
    } catch {
      /* fall through to the <img> path (some browsers refuse SVG here) */
    }
  }
  const url = URL.createObjectURL(blob);
  try {
    const img = await new Promise<HTMLImageElement>((resolve, reject) => {
      const el = new Image();
      el.onload = () => resolve(el);
      el.onerror = () => reject(new Error('not-an-image'));
      el.src = url;
    });
    return drawableToBitmap(img, img.naturalWidth, img.naturalHeight);
  } finally {
    URL.revokeObjectURL(url);
  }
}

/** Paint a bitmap onto a canvas, resizing the canvas to fit. */
export function paintBitmap(canvas: HTMLCanvasElement, b: Bitmap): void {
  canvas.width = b.width;
  canvas.height = b.height;
  const ctx = context(canvas);
  ctx.putImageData(new ImageData(b.data, b.width, b.height), 0, 0);
}

/** A bitmap with the captions burned in (the original is untouched; the editor keeps the result). */
export function applyCaptions(b: Bitmap, options: CaptionOptions): Bitmap {
  const c = makeCanvas(b.width, b.height);
  const ctx = context(c);
  ctx.putImageData(new ImageData(new Uint8ClampedArray(b.data), b.width, b.height), 0, 0);
  drawCaptions(ctx, b.width, b.height, options);
  const id = ctx.getImageData(0, 0, b.width, b.height);
  return { width: b.width, height: b.height, data: id.data };
}

function toBlob(c: Canvas2D, type: string, quality?: number): Promise<Blob> {
  if ('convertToBlob' in c) return c.convertToBlob({ type, ...(quality === undefined ? {} : { quality }) });
  return new Promise((resolve, reject) =>
    c.toBlob((b) => (b ? resolve(b) : reject(new Error('encode-failed'))), type, quality),
  );
}

/**
 * Encode the bitmap. JPEG has no alpha, so it is flattened onto white first. When the browser does not support
 * the format (WebP in an old Safari) it silently returns PNG; read `blob.type` for what you got.
 */
export async function exportBitmap(b: Bitmap, format: ExportFormat, quality = 0.92): Promise<Blob> {
  const src = format === 'jpeg' ? flatten(b) : b;
  const c = makeCanvas(src.width, src.height);
  context(c).putImageData(
    new ImageData(new Uint8ClampedArray(src.data), src.width, src.height),
    0,
    0,
  );
  return toBlob(c, FORMAT_MIME[format], format === 'png' ? undefined : quality);
}
