// Reads a CBZ that sits in someone's bucket by range requests (BYOS-CAT-002): the table of contents from the last
// bytes of the file (one request, a second when the directory is longer than the first guess), then one request
// per page. Nothing goes through the server and nothing is converted: the page's own bytes are inflated here.
// The listing and its ordering come from `sr-library-wasm` (`listPages`), the same rules as the server's archive
// reader: junk and unsafe names out, natural page order.
import type { ZipProbe } from '@sideroom/wasm';

/** The part of `LibraryApi` the reader needs; tests pass a stand-in. */
export interface PageLister {
  firstTail: number;
  listPages(
    tail: Uint8Array,
    fileLen: number,
  ): { need_tail: number } | { ok: true; pages: ZipProbe[] } | { ok: false; message: string };
}

export interface CbzBook {
  /** Pages in reading order. */
  readonly pages: readonly ZipProbe[];
  /** The bytes of one page, inflated. */
  page(index: number): Promise<Blob>;
}

export class CbzError extends Error {}

type Fetch = (input: string, init?: RequestInit) => Promise<Response>;

/** Room after a local header for its name and extra field, so a page usually costs one request. */
const HEADER_ROOM = 30 + 1024;
const MAX_TAIL_ROUNDS = 3;
const MAX_PAGE_BYTES = 64 * 1024 * 1024;

const MIME: Record<string, string> = {
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  png: 'image/png',
  webp: 'image/webp',
  gif: 'image/gif',
};
const mimeOf = (name: string) => MIME[name.split('.').pop()?.toLowerCase() ?? ''] ?? 'image/jpeg';

/** Total size from `Content-Range: bytes a-b/total`. */
function totalOf(res: Response, got: number): number {
  const m = /\/(\d+)\s*$/.exec(res.headers.get('content-range') ?? '');
  return m ? Number(m[1]) : got;
}

async function range(f: Fetch, url: string, spec: string): Promise<{ bytes: Uint8Array; total: number }> {
  const res = await f(url, { headers: { Range: spec } });
  if (res.status !== 206 && res.status !== 200) throw new CbzError(`range ${res.status}`);
  const buf = new Uint8Array(await res.arrayBuffer());
  return { bytes: buf, total: totalOf(res, buf.length) };
}

async function inflate(raw: Uint8Array, method: number): Promise<Uint8Array> {
  if (method === 0) return raw;
  if (method !== 8 || typeof DecompressionStream === 'undefined') {
    throw new CbzError('unsupported compression');
  }
  const stream = new Blob([raw as BlobPart]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** Open a CBZ at a presigned `url`. Throws `CbzError` when the file cannot be listed. */
export async function openCbz(url: string, api: PageLister, f: Fetch = fetch.bind(globalThis)): Promise<CbzBook> {
  let want = api.firstTail;
  let pages: ZipProbe[] | null = null;
  let total = 0;
  for (let round = 0; round < MAX_TAIL_ROUNDS && !pages; round++) {
    const r = await range(f, url, `bytes=-${want}`);
    total = r.total;
    const listed = api.listPages(r.bytes, total);
    if ('need_tail' in listed) {
      want = Math.max(listed.need_tail, want + 1);
      continue;
    }
    if (!listed.ok) throw new CbzError(listed.message);
    pages = listed.pages;
  }
  if (!pages) throw new CbzError('directory too large');
  const list = pages;
  const fileLen = total;
  const cache = new Map<number, Promise<Blob>>();
  const fetchPage = async (i: number): Promise<Blob> => {
    const p = list[i];
    if (!p) throw new CbzError('no such page');
    if (p.size > MAX_PAGE_BYTES) throw new CbzError('page too large');
    const end = Math.min(fileLen, p.local_offset + HEADER_ROOM + p.compressed) - 1;
    let { bytes } = await range(f, url, `bytes=${p.local_offset}-${end}`);
    const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    if (bytes.length < 30 || dv.getUint32(0, true) !== 0x04034b50) throw new CbzError('bad local header');
    const start = 30 + dv.getUint16(26, true) + dv.getUint16(28, true);
    if (start + p.compressed > bytes.length) {
      bytes = (await range(f, url, `bytes=${p.local_offset}-${p.local_offset + start + p.compressed - 1}`)).bytes;
    }
    const data = await inflate(bytes.subarray(start, start + p.compressed), p.method);
    return new Blob([data as BlobPart], { type: mimeOf(p.name) });
  };
  return {
    pages: list,
    page(i: number) {
      let hit = cache.get(i);
      if (!hit) {
        hit = fetchPage(i);
        cache.set(i, hit);
        hit.catch(() => cache.delete(i));
        // Keep a handful: the reader asks for the page it shows and the next one.
        if (cache.size > 6) cache.delete(cache.keys().next().value as number);
      }
      return hit;
    },
  };
}
