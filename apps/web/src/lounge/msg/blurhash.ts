/**
 * BlurHash (https://blurha.sh, MIT) encode and decode, written for this app so the web client needs no extra
 * package. Images get a hash when they are uploaded (the browser already decodes the file for the preview), and
 * the message list paints it as a placeholder until the picture itself has loaded (LNG-MSGUI-009).
 */

const DIGITS =
  '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~';

function decode83(str: string): number {
  let value = 0;
  for (const ch of str) {
    const d = DIGITS.indexOf(ch);
    if (d < 0) throw new Error('Not a BlurHash');
    value = value * 83 + d;
  }
  return value;
}

function encode83(value: number, length: number): string {
  let out = '';
  for (let i = 1; i <= length; i++) {
    const digit = Math.floor(value / 83 ** (length - i)) % 83;
    out += DIGITS[digit];
  }
  return out;
}

const sRGBToLinear = (v: number) => {
  const x = v / 255;
  return x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
};
const linearTosRGB = (v: number) => {
  const x = Math.max(0, Math.min(1, v));
  return x <= 0.0031308
    ? Math.trunc(x * 12.92 * 255 + 0.5)
    : Math.trunc((1.055 * x ** (1 / 2.4) - 0.055) * 255 + 0.5);
};
const signPow = (v: number, exp: number) => Math.sign(v) * Math.abs(v) ** exp;

/** Whether `hash` has the length its size byte promises. */
export function isValidBlurhash(hash: string): boolean {
  if (hash.length < 6) return false;
  try {
    const sizeFlag = decode83(hash[0]!);
    const numY = Math.floor(sizeFlag / 9) + 1;
    const numX = (sizeFlag % 9) + 1;
    return hash.length === 4 + 2 * numX * numY;
  } catch {
    return false;
  }
}

/** Decodes to RGBA pixels (`width * height * 4` bytes). Throws on a malformed hash. */
export function decodeBlurhash(
  hash: string,
  width: number,
  height: number,
  punch = 1,
): Uint8ClampedArray {
  if (!isValidBlurhash(hash)) throw new Error('Not a BlurHash');
  const sizeFlag = decode83(hash[0]!);
  const numY = Math.floor(sizeFlag / 9) + 1;
  const numX = (sizeFlag % 9) + 1;
  const maxValue = ((decode83(hash[1]!) + 1) / 166) * punch;

  const colors: [number, number, number][] = [];
  for (let i = 0; i < numX * numY; i++) {
    if (i === 0) {
      const v = decode83(hash.substring(2, 6));
      colors.push([sRGBToLinear(v >> 16), sRGBToLinear((v >> 8) & 255), sRGBToLinear(v & 255)]);
    } else {
      const v = decode83(hash.substring(4 + i * 2, 6 + i * 2));
      const qr = Math.floor(v / (19 * 19));
      const qg = Math.floor(v / 19) % 19;
      const qb = v % 19;
      colors.push([
        signPow((qr - 9) / 9, 2) * maxValue,
        signPow((qg - 9) / 9, 2) * maxValue,
        signPow((qb - 9) / 9, 2) * maxValue,
      ]);
    }
  }

  const pixels = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      let r = 0;
      let g = 0;
      let b = 0;
      for (let j = 0; j < numY; j++) {
        for (let i = 0; i < numX; i++) {
          const basis = Math.cos((Math.PI * x * i) / width) * Math.cos((Math.PI * y * j) / height);
          const c = colors[i + j * numX]!;
          r += c[0] * basis;
          g += c[1] * basis;
          b += c[2] * basis;
        }
      }
      const o = 4 * (x + y * width);
      pixels[o] = linearTosRGB(r);
      pixels[o + 1] = linearTosRGB(g);
      pixels[o + 2] = linearTosRGB(b);
      pixels[o + 3] = 255;
    }
  }
  return pixels;
}

/** Encodes RGBA pixels with `componentsX` by `componentsY` colour components (1 to 9 each). */
export function encodeBlurhash(
  pixels: Uint8ClampedArray,
  width: number,
  height: number,
  componentsX = 4,
  componentsY = 3,
): string {
  if (componentsX < 1 || componentsX > 9 || componentsY < 1 || componentsY > 9)
    throw new Error('BlurHash components must be between 1 and 9');
  if (pixels.length !== width * height * 4) throw new Error('Pixel buffer does not match the size');

  const factors: [number, number, number][] = [];
  for (let y = 0; y < componentsY; y++) {
    for (let x = 0; x < componentsX; x++) {
      const norm = x === 0 && y === 0 ? 1 : 2;
      let r = 0;
      let g = 0;
      let b = 0;
      for (let py = 0; py < height; py++) {
        for (let px = 0; px < width; px++) {
          const basis =
            norm * Math.cos((Math.PI * x * px) / width) * Math.cos((Math.PI * y * py) / height);
          const o = 4 * (px + py * width);
          r += basis * sRGBToLinear(pixels[o]!);
          g += basis * sRGBToLinear(pixels[o + 1]!);
          b += basis * sRGBToLinear(pixels[o + 2]!);
        }
      }
      const scale = 1 / (width * height);
      factors.push([r * scale, g * scale, b * scale]);
    }
  }

  const dc = factors[0]!;
  const ac = factors.slice(1);
  let hash = encode83(componentsX - 1 + (componentsY - 1) * 9, 1);
  let maxValue: number;
  if (ac.length > 0) {
    const actualMax = Math.max(...ac.flatMap((f) => f.map(Math.abs)));
    const quantised = Math.max(0, Math.min(82, Math.floor(actualMax * 166 - 0.5)));
    maxValue = (quantised + 1) / 166;
    hash += encode83(quantised, 1);
  } else {
    maxValue = 1;
    hash += encode83(0, 1);
  }
  hash += encode83(
    (linearTosRGB(dc[0]) << 16) + (linearTosRGB(dc[1]) << 8) + linearTosRGB(dc[2]),
    4,
  );
  const q = (v: number) =>
    Math.max(0, Math.min(18, Math.floor(signPow(v / maxValue, 0.5) * 9 + 9.5)));
  for (const f of ac) hash += encode83(q(f[0]) * 19 * 19 + q(f[1]) * 19 + q(f[2]), 2);
  return hash;
}

/** The size, and a BlurHash when the browser can draw the picture, of an image file. */
export interface ImageMeta {
  width: number;
  height: number;
  blurhash?: string;
}

/** Longest side of the thumbnail the hash is computed from. */
const SAMPLE = 32;

/**
 * Reads the pixel size of an image file and hashes a small copy of it. Resolves `null` where the browser
 * cannot decode the file (or has no canvas), so the upload simply goes without placeholder data.
 */
export async function imageMeta(file: Blob): Promise<ImageMeta | null> {
  if (!file.type.startsWith('image/') || file.type === 'image/svg+xml') return null;
  if (typeof createImageBitmap !== 'function') return null;
  let bitmap: ImageBitmap | undefined;
  try {
    bitmap = await createImageBitmap(file);
    const { width, height } = bitmap;
    if (!width || !height) return null;
    const meta: ImageMeta = { width, height };
    const scale = SAMPLE / Math.max(width, height);
    const w = Math.max(1, Math.round(width * scale));
    const h = Math.max(1, Math.round(height * scale));
    const canvas =
      typeof OffscreenCanvas !== 'undefined'
        ? new OffscreenCanvas(w, h)
        : document.createElement('canvas');
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext('2d') as
      CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null;
    if (ctx) {
      ctx.drawImage(bitmap, 0, 0, w, h);
      meta.blurhash = encodeBlurhash(ctx.getImageData(0, 0, w, h).data, w, h);
    }
    return meta;
  } catch {
    return null;
  } finally {
    bitmap?.close();
  }
}
