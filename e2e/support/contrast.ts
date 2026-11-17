/**
 * Shared contrast helper (wave 90, SEC-AUD-384). Reads what Chromium returns for a computed colour: `rgb()`,
 * `rgba()`, `color(srgb r g b)` and `color(srgb r g b / a)` (channels 0 to 1, as `color-mix()` gives), and hex.
 * Channels are returned as 0-255. Unknown input throws: it never returns a number for a colour it could not read.
 */
export interface Rgba {
  r: number;
  g: number;
  b: number;
  a: number;
}

const NUM = String.raw`(-?[\d.]+(?:e-?\d+)?)(%?)`;

export function parseColor(css: string): Rgba {
  const s = css.trim().toLowerCase();
  const hex = /^#([0-9a-f]{3,8})$/.exec(s);
  if (hex) {
    let h = hex[1]!;
    if (h.length === 3 || h.length === 4) h = [...h].map((c) => c + c).join('');
    if (h.length !== 6 && h.length !== 8) throw new Error(`unreadable colour: ${css}`);
    return {
      r: parseInt(h.slice(0, 2), 16),
      g: parseInt(h.slice(2, 4), 16),
      b: parseInt(h.slice(4, 6), 16),
      a: h.length === 8 ? parseInt(h.slice(6, 8), 16) / 255 : 1,
    };
  }
  const alpha = (txt: string | undefined, pct: string | undefined): number => {
    if (txt === undefined) return 1;
    const v = Number(txt);
    return pct ? v / 100 : v;
  };
  const rgb = new RegExp(
    String.raw`^rgba?\(\s*${NUM}\s*[, ]\s*${NUM}\s*[, ]\s*${NUM}\s*(?:[,/]\s*${NUM}\s*)?\)$`,
  ).exec(s);
  if (rgb) {
    const ch = (t: string, p: string) => (p ? (Number(t) / 100) * 255 : Number(t));
    const out = {
      r: ch(rgb[1]!, rgb[2]!),
      g: ch(rgb[3]!, rgb[4]!),
      b: ch(rgb[5]!, rgb[6]!),
      a: alpha(rgb[7], rgb[8]),
    };
    if ([out.r, out.g, out.b, out.a].some((n) => !Number.isFinite(n)))
      throw new Error(`unreadable colour: ${css}`);
    return out;
  }
  const srgb = new RegExp(
    String.raw`^color\(\s*srgb\s+${NUM}\s+${NUM}\s+${NUM}\s*(?:/\s*${NUM}\s*)?\)$`,
  ).exec(s);
  if (srgb) {
    const ch = (t: string, p: string) => (p ? Number(t) / 100 : Number(t)) * 255;
    const out = {
      r: ch(srgb[1]!, srgb[2]!),
      g: ch(srgb[3]!, srgb[4]!),
      b: ch(srgb[5]!, srgb[6]!),
      a: alpha(srgb[7], srgb[8]),
    };
    if ([out.r, out.g, out.b, out.a].some((n) => !Number.isFinite(n)))
      throw new Error(`unreadable colour: ${css}`);
    return out;
  }
  throw new Error(`unreadable colour: ${css}`);
}

/** `top` laid over `ground` (the result keeps the ground's alpha-composited opacity). */
export function over(top: Rgba, ground: Rgba): Rgba {
  const a = top.a + ground.a * (1 - top.a);
  if (a === 0) return { r: 0, g: 0, b: 0, a: 0 };
  const mix = (t: number, g: number) => (t * top.a + g * ground.a * (1 - top.a)) / a;
  return { r: mix(top.r, ground.r), g: mix(top.g, ground.g), b: mix(top.b, ground.b), a };
}

export function luminance(c: Rgba): number {
  const f = (x: number) => {
    const y = x / 255;
    return y <= 0.03928 ? y / 12.92 : Math.pow((y + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
}

/** WCAG ratio of two opaque colours (lay a colour with alpha over its ground with `over` first). */
export function ratio(a: Rgba, b: Rgba): number {
  const l1 = luminance(a);
  const l2 = luminance(b);
  return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
}

/** Ratio of a text colour on a ground, both as CSS strings; the text's alpha is laid over the ground first. */
export function textRatio(textCss: string, groundCss: string): number {
  const ground = parseColor(groundCss);
  const text = over(parseColor(textCss), ground);
  return ratio(text, ground);
}
