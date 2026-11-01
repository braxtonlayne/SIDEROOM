// Covers for the Study (STU-SHF-001), taken strictly from the file itself, in the browser. No external metadata
// service is ever asked:
//   audio  -> the ID3 / MP4 / FLAC picture, read with the `sr-music-wasm` facade from the first bytes of the file;
//   cbz    -> page 1 of the archive (range reads, the same reader as the shelf's CBZ reader);
//   epub   -> its cover image: the image entry named like a cover, else the first image in reading order;
//   video  -> a frame captured from a <video> element playing the presigned URL;
//   images -> the image itself.
// Everything is best effort: a file that yields nothing simply has no cover (the card shows its type).
import { openCbz, type PageLister } from '../library/reader/cbzRange';
import { readTrackTags } from '../music/index/readTrack';
import type { Fetch } from '../music/index/ranges';
import type { TagReader } from '../music/index/types';
import { extOf, typeOf } from './fileTypes';

export type CoverSource = 'audio' | 'cbz' | 'epub' | 'video' | 'image';

/** Which extractor a file gets, or null when it has none (a PDF, a ROM, a text file...). */
export function coverSource(key: string): CoverSource | null {
  const e = extOf(key);
  if (e === 'cbz') return 'cbz';
  if (e === 'epub') return 'epub';
  const t = typeOf(key);
  if (t === 'audio') return 'audio';
  if (t === 'video') return 'video';
  if (t === 'images') return 'image';
  return null;
}

export interface CoverDeps {
  fetchFn?: Fetch;
  /** The music facade (loaded lazily by the caller). */
  tags?: () => Promise<TagReader>;
  /** The library facade's page lister. */
  lister?: () => Promise<PageLister>;
  /** Captures a frame of a video; a seam for tests. */
  frame?: (url: string) => Promise<Blob | null>;
}

/** Audio: the picture inside the tags. */
export async function audioCover(
  url: string,
  size: number,
  container: string,
  d: CoverDeps,
): Promise<Blob | null> {
  if (!d.tags) return null;
  const reader = await d.tags();
  const out = await readTrackTags(url, size, container, {
    reader,
    fetchFn: d.fetchFn ?? fetch.bind(globalThis),
    warnOnce: () => undefined,
  });
  if (out.kind !== 'tags' || !out.tags.cover || out.tags.cover.bytes.length === 0) return null;
  return new Blob([out.tags.cover.bytes as BlobPart], { type: out.tags.cover.mime });
}

/** The page of an archive that stands for the book: an EPUB prefers an entry named like a cover. */
export function pickCoverPage(names: readonly string[], epub: boolean): number {
  if (names.length === 0) return -1;
  if (epub) {
    const i = names.findIndex((n) => /(^|[/_-])cover[^/]*$/i.test(n));
    if (i >= 0) return i;
  }
  return 0;
}

/** CBZ and EPUB: one image out of the archive by range reads. */
export async function archiveCover(url: string, epub: boolean, d: CoverDeps): Promise<Blob | null> {
  if (!d.lister) return null;
  const book = await openCbz(url, await d.lister(), d.fetchFn);
  const i = pickCoverPage(
    book.pages.map((p) => p.name),
    epub,
  );
  return i < 0 ? null : book.page(i);
}

/** Video: a frame from the <video> element, a little way in (a black first frame is common). */
export function captureFrame(
  url: string,
  doc: Document = document,
  timeoutMs = 15_000,
): Promise<Blob | null> {
  return new Promise((resolve) => {
    const v = doc.createElement('video');
    let done = false;
    const finish = (b: Blob | null) => {
      if (done) return;
      done = true;
      window.clearTimeout(timer);
      v.removeAttribute('src');
      v.load();
      resolve(b);
    };
    const timer = window.setTimeout(() => finish(null), timeoutMs);
    v.muted = true;
    v.preload = 'metadata';
    v.crossOrigin = 'anonymous';
    v.addEventListener('error', () => finish(null));
    v.addEventListener('loadedmetadata', () => {
      const d = Number.isFinite(v.duration) ? v.duration : 0;
      v.currentTime = Math.min(Math.max(d * 0.1, 1), Math.max(d - 0.1, 0));
    });
    v.addEventListener('seeked', () => {
      try {
        const w = v.videoWidth;
        const h = v.videoHeight;
        if (!w || !h) return finish(null);
        const scale = Math.min(1, 480 / w);
        const c = doc.createElement('canvas');
        c.width = Math.round(w * scale);
        c.height = Math.round(h * scale);
        const ctx = c.getContext('2d');
        if (!ctx) return finish(null);
        ctx.drawImage(v, 0, 0, c.width, c.height);
        c.toBlob((b) => finish(b), 'image/jpeg', 0.8);
      } catch {
        finish(null);
      }
    });
    v.src = url;
  });
}

/**
 * The cover bytes of one file, or null. `url` is a presigned GET of the file; `size` its length in bytes.
 * Errors are swallowed: no cover is not a failure.
 */
export async function extractCover(
  key: string,
  url: string,
  size: number,
  d: CoverDeps,
): Promise<Blob | null> {
  const src = coverSource(key);
  try {
    switch (src) {
      case 'audio':
        return await audioCover(url, size, extOf(key), d);
      case 'cbz':
        return await archiveCover(url, false, d);
      case 'epub':
        return await archiveCover(url, true, d);
      case 'video':
        return await (d.frame ?? ((u) => captureFrame(u)))(url);
      default:
        return null;
    }
  } catch {
    return null;
  }
}

/** A small queue so a folder of 200 files asks for covers two at a time. */
export class CoverQueue {
  private active = 0;
  private waiting: (() => void)[] = [];
  private cache = new Map<string, Promise<string | null>>();
  constructor(
    private limit = 2,
    private make: (b: Blob) => string = (b) => URL.createObjectURL(b),
  ) {}

  /** An object URL for the cover of `id`, extracted once. `run` does the extraction. */
  get(id: string, run: () => Promise<Blob | null>): Promise<string | null> {
    let hit = this.cache.get(id);
    if (!hit) {
      hit = this.slot().then(async () => {
        try {
          const b = await run();
          return b ? this.make(b) : null;
        } catch {
          return null;
        } finally {
          this.release();
        }
      });
      this.cache.set(id, hit);
    }
    return hit;
  }

  private slot(): Promise<void> {
    if (this.active < this.limit) {
      this.active += 1;
      return Promise.resolve();
    }
    return new Promise((r) => this.waiting.push(() => r()));
  }
  private release() {
    const next = this.waiting.shift();
    if (next) next();
    else this.active -= 1;
  }
}
