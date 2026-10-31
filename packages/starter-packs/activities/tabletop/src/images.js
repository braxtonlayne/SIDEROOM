// Picture handling in the browser: shrink a picked file to something the shared state can carry, make token
// thumbnails, and cache decoded images for drawing. The Activity CSP allows only `self`, `data:` and `blob:`
// images, so pictures travel as data URLs inside the shared state, never as links to other hosts.

import { CHUNK_CHARS, MAX_DATA_URL_CHARS, MAX_IMG_CHUNKS } from './table.js';

const cache = new Map();

/** Decode a URL to an image once; resolves `null` when it cannot be loaded (blocked, missing, not a picture). */
export function loadImage(url) {
  if (!url) return Promise.resolve(null);
  let p = cache.get(url);
  if (!p) {
    p = new Promise((resolve) => {
      const img = new Image();
      img.onload = () => resolve(img);
      img.onerror = () => resolve(null);
      img.src = url;
    });
    cache.set(url, p);
    if (cache.size > 300) cache.delete(cache.keys().next().value);
  }
  return p;
}

async function decodeFile(file) {
  const url = URL.createObjectURL(file);
  try {
    const img = await loadImage(url);
    if (!img) throw new Error('decode');
    return { img, w: img.naturalWidth, h: img.naturalHeight };
  } finally {
    // The decoded image stays in the cache by object URL; revoke after use.
    setTimeout(() => URL.revokeObjectURL(url), 5000);
  }
}

function drawScaled(img, w, h, scale, type, quality) {
  const cw = Math.max(Math.round(w * scale), 1);
  const ch = Math.max(Math.round(h * scale), 1);
  const canvas = document.createElement('canvas');
  canvas.width = cw;
  canvas.height = ch;
  const ctx = canvas.getContext('2d');
  ctx.drawImage(img, 0, 0, cw, ch);
  return { url: canvas.toDataURL(type, quality), width: cw, height: ch };
}

/**
 * Shrink a map picture until it fits `MAX_IMG_CHUNKS` chunks: first the longest side down to 1600 px, then
 * lower JPEG quality and size in steps. Resolves { url, width, height } or throws `too-big` / `decode`.
 */
export async function mapFromFile(file) {
  const { img, w, h } = await decodeFile(file);
  const limit = CHUNK_CHARS * MAX_IMG_CHUNKS;
  let scale = Math.min(1, 1600 / Math.max(w, h));
  for (const q of [0.82, 0.7, 0.55, 0.4]) {
    for (let step = 0; step < 4; step++) {
      const out = drawScaled(img, w, h, scale * 0.8 ** step, 'image/jpeg', q);
      if (out.url.length <= limit) return out;
    }
  }
  throw new Error('too-big');
}

/** A square thumbnail (cover-cropped) for a token: JPEG so it stays within the token picture limit. */
export async function thumbFromImage(img, side = 64) {
  const w = img.naturalWidth;
  const h = img.naturalHeight;
  const s = Math.min(w, h);
  const canvas = document.createElement('canvas');
  canvas.width = side;
  canvas.height = side;
  canvas.getContext('2d').drawImage(img, (w - s) / 2, (h - s) / 2, s, s, 0, 0, side, side);
  for (const q of [0.8, 0.6, 0.4, 0.25]) {
    const url = canvas.toDataURL('image/jpeg', q);
    if (url.length <= MAX_DATA_URL_CHARS) return url;
  }
  return '';
}

export async function thumbFromFile(file) {
  const { img } = await decodeFile(file);
  return thumbFromImage(img);
}

/**
 * A member's avatar as a thumbnail data URL, or '' when it cannot be had (the Activity CSP may block the
 * avatar host, or it may not allow reading its pixels). The token then falls back to a coloured initial.
 */
export async function thumbFromAvatar(url) {
  if (!url) return '';
  try {
    const img = await new Promise((resolve, reject) => {
      const i = new Image();
      i.crossOrigin = 'anonymous';
      i.onload = () => resolve(i);
      i.onerror = reject;
      i.src = url;
    });
    return await thumbFromImage(img);
  } catch {
    return '';
  }
}

export function newId(prefix = 'i') {
  const a = new Uint8Array(6);
  crypto.getRandomValues(a);
  return prefix + [...a].map((b) => b.toString(36).padStart(2, '0')).join('').slice(0, 10);
}
