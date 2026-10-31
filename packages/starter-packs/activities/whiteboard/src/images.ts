// Getting an image onto the board: choose a file from a mount (or the device) or give an https address. A file
// is shrunk until its data URL fits one state entry; the board never fetches anything for the author.

import { LIMITS } from './log';

export interface ImageResult {
  src: string;
  w: number;
  h: number;
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error('decode'));
    img.src = src;
  });
}

/** Shrink a picked file to a data URL under the entry limit, or throw `too-big`. */
export async function fileToImage(file: File, maxSide = 900): Promise<ImageResult> {
  const url = URL.createObjectURL(file);
  try {
    const img = await loadImage(url);
    let scale = Math.min(1, maxSide / Math.max(img.naturalWidth, img.naturalHeight));
    const alpha =
      file.type === 'image/png' || file.type === 'image/gif' || file.type === 'image/webp';
    for (let attempt = 0; attempt < 8; attempt++) {
      const w = Math.max(1, Math.round(img.naturalWidth * scale));
      const h = Math.max(1, Math.round(img.naturalHeight * scale));
      const c = document.createElement('canvas');
      c.width = w;
      c.height = h;
      const ctx = c.getContext('2d');
      if (!ctx) throw new Error('canvas');
      ctx.drawImage(img, 0, 0, w, h);
      const quality = Math.max(0.35, 0.8 - attempt * 0.08);
      const src =
        alpha && attempt < 3
          ? c.toDataURL('image/webp', quality)
          : c.toDataURL('image/jpeg', quality);
      if (src.length <= LIMITS.maxImageChars) return { src, w, h };
      scale *= 0.75;
    }
    throw new Error('too-big');
  } finally {
    URL.revokeObjectURL(url);
  }
}

/** An https address: the picture is shown from there (a mount's presigned link works). */
export async function urlToImage(url: string): Promise<ImageResult> {
  if (!/^https:\/\//i.test(url)) throw new Error('https');
  const img = await loadImage(url);
  return { src: url, w: img.naturalWidth, h: img.naturalHeight };
}

/** Fit a picture into the page, at most 60 percent of it each way. */
export function fitImage(
  w: number,
  h: number,
  pageW: number,
  pageH: number,
): { w: number; h: number } {
  const s = Math.min(1, (pageW * 0.6) / w, (pageH * 0.6) / h);
  return { w: Math.max(1, Math.round(w * s)), h: Math.max(1, Math.round(h * s)) };
}
