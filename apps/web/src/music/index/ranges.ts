// Range reads of a presigned URL, and the M4A box walk (MUS-IDX-001). The tagger never downloads a whole file:
// the first 1 MiB (or the last 128 bytes for ID3v1), and for an M4A whose `moov` comes after `mdat`, the box
// headers (16 bytes each) up to `moov` and then `moov` alone.
import { HttpError } from './backoff';

export type Fetch = (input: string, init?: RequestInit) => Promise<Response>;

/** Bytes `[start, start + length)` of the object behind `url`. */
export async function readRange(
  f: Fetch,
  url: string,
  start: number,
  length: number,
): Promise<Uint8Array> {
  if (length <= 0) return new Uint8Array(0);
  const res = await f(url, { headers: { Range: `bytes=${start}-${start + length - 1}` } });
  if (res.status === 206) return new Uint8Array(await res.arrayBuffer());
  if (res.status === 200 && start === 0 && res.body) {
    // A bucket that ignores Range sends the whole file: keep what is asked for, drop the rest.
    const reader = res.body.getReader();
    const chunks: Uint8Array[] = [];
    let got = 0;
    while (got < length) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      got += value.length;
    }
    await reader.cancel().catch(() => undefined);
    return concat(chunks).subarray(0, length);
  }
  throw new HttpError(res.status);
}

export function concat(parts: readonly Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

export interface BoxHeader {
  /** Box size including the header. */
  size: number;
  type: string;
}

/** The size and type of the box starting at `b[0]`; `null` when `b` is too short or the box is nonsense. */
export function parseBox(b: Uint8Array, remaining: number): BoxHeader | null {
  if (b.length < 8) return null;
  const v = new DataView(b.buffer, b.byteOffset, b.byteLength);
  let size = v.getUint32(0);
  const type = String.fromCharCode(b[4]!, b[5]!, b[6]!, b[7]!);
  if (size === 1) {
    if (b.length < 16) return null;
    size = Number(v.getBigUint64(8));
  } else if (size === 0) {
    size = remaining; // runs to the end of the file
  }
  return size >= 8 && Number.isSafeInteger(size) ? { size, type } : null;
}

export const MAX_BOX_STEPS = 32;

/**
 * Walk the top-level boxes from `from` with 16-byte reads until `moov`. Gives up (null) after `maxSteps` reads,
 * at the end of the file, or at a box that makes no sense.
 */
export async function locateMoov(
  read: (start: number, length: number) => Promise<Uint8Array>,
  from: number,
  fileSize: number,
  maxSteps = MAX_BOX_STEPS,
): Promise<{ offset: number; size: number } | null> {
  let offset = from;
  for (let step = 0; step < maxSteps; step++) {
    if (offset + 8 > fileSize) return null;
    const head = await read(offset, Math.min(16, fileSize - offset));
    const box = parseBox(head, fileSize - offset);
    if (!box) return null;
    if (box.type === 'moov') return { offset, size: Math.min(box.size, fileSize - offset) };
    offset += box.size;
  }
  return null;
}
