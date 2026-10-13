// Minimal blurhash decoder (https://github.com/woltapp/blurhash, MIT algorithm) so page placeholders need no
// dependency. `decodeBlurhash` returns RGBA pixels; `null` for a malformed hash.
const ALPHABET =
  '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~';

function decode83(str: string): number {
  let v = 0;
  for (const ch of str) {
    const d = ALPHABET.indexOf(ch);
    if (d < 0) return Number.NaN;
    v = v * 83 + d;
  }
  return v;
}

const srgbToLinear = (v: number) => {
  const x = v / 255;
  return x <= 0.04045 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4);
};
const linearToSrgb = (v: number) => {
  const x = Math.max(0, Math.min(1, v));
  return Math.round((x <= 0.0031308 ? x * 12.92 : 1.055 * Math.pow(x, 1 / 2.4) - 0.055) * 255);
};
const signPow = (v: number, e: number) => Math.sign(v) * Math.pow(Math.abs(v), e);

function dcColor(value: number): [number, number, number] {
  return [srgbToLinear(value >> 16), srgbToLinear((value >> 8) & 255), srgbToLinear(value & 255)];
}
function acColor(value: number, max: number): [number, number, number] {
  const q = (n: number) => signPow((n - 9) / 9, 2) * max;
  return [q(Math.floor(value / (19 * 19))), q(Math.floor(value / 19) % 19), q(value % 19)];
}

export function decodeBlurhash(
  hash: string,
  width: number,
  height: number,
): Uint8ClampedArray | null {
  if (hash.length < 6) return null;
  const sizeFlag = decode83(hash[0]!);
  const nx = (sizeFlag % 9) + 1;
  const ny = Math.floor(sizeFlag / 9) + 1;
  if (!Number.isFinite(sizeFlag) || hash.length !== 4 + 2 * nx * ny) return null;
  const maxAc = (decode83(hash[1]!) + 1) / 166 || 0;
  const colors: [number, number, number][] = [];
  for (let i = 0; i < nx * ny; i++) {
    if (i === 0) colors.push(dcColor(decode83(hash.slice(2, 6))));
    else colors.push(acColor(decode83(hash.slice(4 + i * 2, 6 + i * 2)), maxAc));
  }
  if (colors.some((c) => c.some((n) => !Number.isFinite(n)))) return null;
  const px = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      let r = 0;
      let g = 0;
      let b = 0;
      for (let j = 0; j < ny; j++) {
        for (let i = 0; i < nx; i++) {
          const basis = Math.cos((Math.PI * x * i) / width) * Math.cos((Math.PI * y * j) / height);
          const c = colors[i + j * nx]!;
          r += c[0] * basis;
          g += c[1] * basis;
          b += c[2] * basis;
        }
      }
      const o = 4 * (x + y * width);
      px[o] = linearToSrgb(r);
      px[o + 1] = linearToSrgb(g);
      px[o + 2] = linearToSrgb(b);
      px[o + 3] = 255;
    }
  }
  return px;
}
