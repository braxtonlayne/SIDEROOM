// Cover art the tagger found: stored as `covers/<hash>.jpg` in the same mount (MUS-IDX-001). The platform owns
// no media: the picture goes to the member's own bucket through the upload path, and the track points at it.
import type { MusicCover } from './types';

export const COVER_DIR = 'covers/';
/** The server accepts 8 to 64 lower-case hex characters. */
export const HASH_CHARS = 32;

export const coverKey = (hash: string) => `${COVER_DIR}${hash}.jpg`;

/** First 32 hex characters of the SHA-256 of `bytes`. */
export async function coverHash(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', bytes as BufferSource);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0'))
    .join('')
    .slice(0, HASH_CHARS);
}

/** JPEG bytes for a cover: a JPEG passes through, anything else is redrawn (canvas). Throws when it cannot. */
export async function toJpeg(c: MusicCover): Promise<Uint8Array> {
  if (c.mime === 'image/jpeg' || c.mime === 'image/jpg') return c.bytes;
  const bitmap = await createImageBitmap(new Blob([c.bytes as BlobPart], { type: c.mime }));
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('no 2d canvas');
  ctx.drawImage(bitmap, 0, 0);
  bitmap.close();
  const blob = await canvas.convertToBlob({ type: 'image/jpeg', quality: 0.85 });
  return new Uint8Array(await blob.arrayBuffer());
}
