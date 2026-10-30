// Reads one track's tags with range requests only (MUS-IDX-001).
//
//   1. the first 1 MiB -> `readTags`;
//   2. an M4A whose `moov` follows `mdat` beyond that: the facade throws `{ code: 'NEED_MOOV', offset }`
//      (offset of `moov`, or of the next top-level box). Walk the boxes with 16-byte reads (at most 32),
//      fetch `moov` alone when it is at most 8 MiB, and call `readTags` on `ftyp + moov`. A bigger `moov`
//      counts as untagged and is logged once;
//   3. an MP3 without a usable ID3v2 tag: the last 128 bytes -> `readTagsTail` (ID3v1).
import { concat, locateMoov, parseBox, readRange, type Fetch } from './ranges';
import type { MusicNeedMoovError, MusicTags, TagReader } from './types';

export const HEAD_BYTES = 1024 * 1024;
export const TAIL_BYTES = 128;
export const MAX_MOOV_BYTES = 8 * 1024 * 1024;

export type ReadOutcome =
  | { kind: 'tags'; tags: MusicTags }
  /** The file carries no tags we can read (or its `moov` is too big to fetch). */
  | { kind: 'none'; reason: 'no-tags' | 'moov-too-large' | 'moov-not-found' };

export interface ReadDeps {
  reader: TagReader;
  fetchFn: Fetch;
  /** Logged once per reason for the life of the tagger. */
  warnOnce: (reason: string, detail: string) => void;
}

function needsMoov(e: unknown): e is MusicNeedMoovError {
  return (
    typeof e === 'object' &&
    e !== null &&
    (e as { code?: unknown }).code === 'NEED_MOOV' &&
    typeof (e as { offset?: unknown }).offset === 'number'
  );
}

const hasAnyTag = (t: MusicTags) =>
  Boolean(t.title || t.artist || t.album || t.albumArtist || t.cover || t.track || t.year);

function tryRead(fn: () => MusicTags): MusicTags | Error {
  try {
    return fn();
  } catch (e) {
    return e instanceof Error ? e : new Error(String(e));
  }
}

async function readMoov(
  url: string,
  head: Uint8Array,
  need: MusicNeedMoovError,
  size: number,
  d: ReadDeps,
): Promise<ReadOutcome> {
  const read = (s: number, n: number) => readRange(d.fetchFn, url, s, n);
  const moov = await locateMoov(read, need.offset, size);
  if (!moov) {
    d.warnOnce('moov-not-found', 'no moov box within 32 top-level boxes');
    return { kind: 'none', reason: 'moov-not-found' };
  }
  if (moov.size > MAX_MOOV_BYTES) {
    d.warnOnce('moov-too-large', `moov box of ${moov.size} bytes is over ${MAX_MOOV_BYTES}`);
    return { kind: 'none', reason: 'moov-too-large' };
  }
  // The brand box comes first in the file: take it from the head we already have.
  const ftyp = parseBox(head, size);
  const ftypBytes =
    ftyp && ftyp.type === 'ftyp' && ftyp.size <= HEAD_BYTES
      ? head.subarray(0, Math.min(ftyp.size, head.length))
      : new Uint8Array(0);
  const moovBytes = await read(moov.offset, moov.size);
  const r = tryRead(() => d.reader.readTags(concat([ftypBytes, moovBytes])));
  return r instanceof Error ? { kind: 'none', reason: 'no-tags' } : { kind: 'tags', tags: r };
}

/** Read the tags of a track of `size` bytes behind the presigned `url`. Network errors propagate. */
export async function readTrackTags(
  url: string,
  size: number,
  container: string,
  d: ReadDeps,
): Promise<ReadOutcome> {
  const head = await readRange(d.fetchFn, url, 0, Math.min(size, HEAD_BYTES));
  const first = tryRead(() => d.reader.readTags(head));
  if (!(first instanceof Error)) {
    if (hasAnyTag(first) || container !== 'mp3') return { kind: 'tags', tags: first };
  } else if (needsMoov(first)) {
    return readMoov(url, head, first, size, d);
  }
  // MP3: ID3v1 lives in the last 128 bytes.
  if (container === 'mp3' && size > TAIL_BYTES) {
    const tail = await readRange(d.fetchFn, url, size - TAIL_BYTES, TAIL_BYTES);
    const t = tryRead(() => d.reader.readTagsTail(tail));
    if (!(t instanceof Error) && hasAnyTag(t)) return { kind: 'tags', tags: t };
  }
  return { kind: 'none', reason: 'no-tags' };
}
