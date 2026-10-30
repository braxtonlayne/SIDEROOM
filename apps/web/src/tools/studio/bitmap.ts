/**
 * UTL-IMG-001: the studio's pixel operations on plain RGBA buffers. Pure (no DOM), so each operation is tested
 * on a fixture. The editor holds ONE working bitmap; redaction and pixelation overwrite its pixels, so an export
 * can only ever contain the redacted pixels (the original is not kept next to it, and nothing here touches the
 * network).
 */

export interface Bitmap {
  width: number;
  height: number;
  /** RGBA, row-major, `width * height * 4` bytes. */
  data: Uint8ClampedArray;
}

/** A rectangle in pixels of a bitmap. */
export interface Region {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type Rgba = [number, number, number, number];

/** Longest edge the studio works on (a 4K photo is scaled down on load). */
export const MAX_EDGE = 4096;

export function createBitmap(width: number, height: number, fill: Rgba = [0, 0, 0, 0]): Bitmap {
  const w = Math.max(1, Math.floor(width));
  const h = Math.max(1, Math.floor(height));
  const data = new Uint8ClampedArray(w * h * 4);
  if (fill.some((v) => v !== 0)) for (let i = 0; i < data.length; i += 4) data.set(fill, i);
  return { width: w, height: h, data };
}

export const cloneBitmap = (b: Bitmap): Bitmap => ({
  width: b.width,
  height: b.height,
  data: new Uint8ClampedArray(b.data),
});

/** Clamp a rectangle (any corner order, fractional) to the bitmap; null when nothing is left. */
export function clampRegion(b: Bitmap, r: Region): Region | null {
  const x0 = Math.max(0, Math.floor(Math.min(r.x, r.x + r.width)));
  const y0 = Math.max(0, Math.floor(Math.min(r.y, r.y + r.height)));
  const x1 = Math.min(b.width, Math.ceil(Math.max(r.x, r.x + r.width)));
  const y1 = Math.min(b.height, Math.ceil(Math.max(r.y, r.y + r.height)));
  if (x1 <= x0 || y1 <= y0) return null;
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

/** A new bitmap holding only `region` of `b`. */
export function crop(b: Bitmap, region: Region): Bitmap {
  const r = clampRegion(b, region);
  if (!r) return cloneBitmap(b);
  const out = createBitmap(r.width, r.height);
  for (let y = 0; y < r.height; y++) {
    const from = ((r.y + y) * b.width + r.x) * 4;
    out.data.set(b.data.subarray(from, from + r.width * 4), y * r.width * 4);
  }
  return out;
}

function halve(b: Bitmap, horizontally: boolean, vertically: boolean): Bitmap {
  const w = horizontally ? Math.ceil(b.width / 2) : b.width;
  const h = vertically ? Math.ceil(b.height / 2) : b.height;
  const out = createBitmap(w, h);
  const kx = horizontally ? 2 : 1;
  const ky = vertically ? 2 : 1;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      for (let c = 0; c < 4; c++) {
        let sum = 0;
        let n = 0;
        for (let j = 0; j < ky; j++) {
          for (let i = 0; i < kx; i++) {
            const px = Math.min(b.width - 1, x * kx + i);
            const py = Math.min(b.height - 1, y * ky + j);
            sum += b.data[(py * b.width + px) * 4 + c]!;
            n++;
          }
        }
        out.data[(y * w + x) * 4 + c] = sum / n;
      }
    }
  }
  return out;
}

/** Bilinear resize (box-averaging first when shrinking by more than 2x, so a big photo does not alias). */
export function resize(b: Bitmap, width: number, height: number): Bitmap {
  const w = Math.max(1, Math.round(width));
  const h = Math.max(1, Math.round(height));
  if (w === b.width && h === b.height) return cloneBitmap(b);
  let src = b;
  while (src.width > w * 2 || src.height > h * 2) {
    src = halve(src, src.width > w * 2, src.height > h * 2);
  }
  const out = createBitmap(w, h);
  const sx = src.width / w;
  const sy = src.height / h;
  for (let y = 0; y < h; y++) {
    const fy = Math.max(0, Math.min(src.height - 1, (y + 0.5) * sy - 0.5));
    const y0 = Math.floor(fy);
    const y1 = Math.min(src.height - 1, y0 + 1);
    const ty = fy - y0;
    for (let x = 0; x < w; x++) {
      const fx = Math.max(0, Math.min(src.width - 1, (x + 0.5) * sx - 0.5));
      const x0 = Math.floor(fx);
      const x1 = Math.min(src.width - 1, x0 + 1);
      const tx = fx - x0;
      const o = (y * w + x) * 4;
      for (let c = 0; c < 4; c++) {
        const a = src.data[(y0 * src.width + x0) * 4 + c]!;
        const bb = src.data[(y0 * src.width + x1) * 4 + c]!;
        const cc = src.data[(y1 * src.width + x0) * 4 + c]!;
        const d = src.data[(y1 * src.width + x1) * 4 + c]!;
        out.data[o + c] = (a * (1 - tx) + bb * tx) * (1 - ty) + (cc * (1 - tx) + d * tx) * ty;
      }
    }
  }
  return out;
}

/** Scale to fit inside `max` x `max`, never up. */
export function fitWithin(b: Bitmap, max: number = MAX_EDGE): Bitmap {
  const scale = Math.min(1, max / Math.max(b.width, b.height));
  return scale === 1 ? b : resize(b, b.width * scale, b.height * scale);
}

export interface Padding {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

/** Add a border of `color` around the bitmap. */
export function pad(b: Bitmap, p: Padding, color: Rgba = [255, 255, 255, 255]): Bitmap {
  const t = Math.max(0, Math.round(p.top));
  const r = Math.max(0, Math.round(p.right));
  const bo = Math.max(0, Math.round(p.bottom));
  const l = Math.max(0, Math.round(p.left));
  const out = createBitmap(b.width + l + r, b.height + t + bo, color);
  for (let y = 0; y < b.height; y++) {
    out.data.set(
      b.data.subarray(y * b.width * 4, (y + 1) * b.width * 4),
      ((y + t) * out.width + l) * 4,
    );
  }
  return out;
}

/** Pad to a given aspect ratio (width / height) by growing the short side evenly. */
export function padToAspect(b: Bitmap, ratio: number, color: Rgba = [255, 255, 255, 255]): Bitmap {
  const wantW = b.height * ratio;
  if (wantW > b.width) {
    const extra = Math.round(wantW - b.width);
    const left = Math.floor(extra / 2);
    return pad(b, { top: 0, bottom: 0, left, right: extra - left }, color);
  }
  const extra = Math.round(b.width / ratio - b.height);
  const top = Math.floor(extra / 2);
  return pad(b, { left: 0, right: 0, top, bottom: extra - top }, color);
}

function remap(
  b: Bitmap,
  outW: number,
  outH: number,
  to: (x: number, y: number) => [number, number],
) {
  const out = createBitmap(outW, outH);
  for (let y = 0; y < b.height; y++) {
    for (let x = 0; x < b.width; x++) {
      const [nx, ny] = to(x, y);
      const from = (y * b.width + x) * 4;
      out.data.set(b.data.subarray(from, from + 4), (ny * outW + nx) * 4);
    }
  }
  return out;
}

export function rotate90(b: Bitmap, clockwise = true): Bitmap {
  return remap(b, b.height, b.width, (x, y) =>
    clockwise ? [b.height - 1 - y, x] : [y, b.width - 1 - x],
  );
}

export function flip(b: Bitmap, horizontal: boolean): Bitmap {
  return remap(b, b.width, b.height, (x, y) =>
    horizontal ? [b.width - 1 - x, y] : [x, b.height - 1 - y],
  );
}

/** Smallest pixelation block that still destroys detail. */
export const MIN_BLOCK = 4;

/**
 * Pixelate `region` in place: every block becomes its average colour. The block is raised to at least
 * [`MIN_BLOCK`] and to at least a sixth of the region's short side, so a small region cannot be "pixelated" at a
 * size that leaves text readable. Returns the region actually changed.
 */
export function pixelate(b: Bitmap, region: Region, block: number): Region | null {
  const r = clampRegion(b, region);
  if (!r) return null;
  const size = Math.max(MIN_BLOCK, Math.round(block), Math.floor(Math.min(r.width, r.height) / 6));
  for (let by = r.y; by < r.y + r.height; by += size) {
    for (let bx = r.x; bx < r.x + r.width; bx += size) {
      const x1 = Math.min(r.x + r.width, bx + size);
      const y1 = Math.min(r.y + r.height, by + size);
      const sum = [0, 0, 0, 0];
      let n = 0;
      for (let y = by; y < y1; y++) {
        for (let x = bx; x < x1; x++) {
          const o = (y * b.width + x) * 4;
          for (let c = 0; c < 4; c++) sum[c]! += b.data[o + c]!;
          n++;
        }
      }
      const avg = sum.map((s) => Math.round(s / n));
      for (let y = by; y < y1; y++) {
        for (let x = bx; x < x1; x++) b.data.set(avg, (y * b.width + x) * 4);
      }
    }
  }
  return r;
}

/** Cover `region` with an opaque colour (default black), in place. Nothing of the original survives in it. */
export function redact(
  b: Bitmap,
  region: Region,
  color: [number, number, number] = [0, 0, 0],
): Region | null {
  const r = clampRegion(b, region);
  if (!r) return null;
  const px = [color[0], color[1], color[2], 255];
  for (let y = r.y; y < r.y + r.height; y++) {
    for (let x = r.x; x < r.x + r.width; x++) b.data.set(px, (y * b.width + x) * 4);
  }
  return r;
}

/** Flatten transparency onto a background (JPEG and GIF have no alpha). */
export function flatten(b: Bitmap, background: [number, number, number] = [255, 255, 255]): Bitmap {
  const out = cloneBitmap(b);
  for (let i = 0; i < out.data.length; i += 4) {
    const a = out.data[i + 3]! / 255;
    for (let c = 0; c < 3; c++) out.data[i + c] = out.data[i + c]! * a + background[c]! * (1 - a);
    out.data[i + 3] = 255;
  }
  return out;
}

/** Scale a pointer rectangle measured on a canvas shown `shownWidth` CSS pixels wide to bitmap pixels. */
export function toBitmapRegion(b: Bitmap, shownWidth: number, r: Region): Region {
  const k = b.width / shownWidth;
  return { x: r.x * k, y: r.y * k, width: r.width * k, height: r.height * k };
}
